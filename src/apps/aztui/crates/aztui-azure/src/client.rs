use crate::{auth::AuthProvider, models::*, AzureError, Result};
use azure_mgmt_resources::Client as ResourcesClient;
use serde::Deserialize;
use std::sync::Arc;
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Azure API client
pub struct AzureClient {
    auth: Arc<dyn AuthProvider>,
}

impl AzureClient {
    /// Create a new Azure client
    pub fn new(auth: Arc<dyn AuthProvider>) -> Self {
        Self { auth }
    }

    /// List all subscriptions
    pub async fn list_subscriptions(&self) -> Result<Vec<AzureSubscription>> {
        info!("Fetching subscriptions from Azure CLI");

        let credential = self.auth.credential();
        let _client = ResourcesClient::builder(credential.clone())
            .build()
            .map_err(|e| AzureError::ApiError(format!("Failed to create Azure client: {}", e)))?;

        let output = Command::new("az")
            .args(["account", "list", "--all", "--output", "json"])
            .output()
            .await
            .map_err(|e| AzureError::ApiError(format!("Failed to run Azure CLI: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AzureError::ApiError(format!(
                "Azure CLI subscription query failed: {}",
                stderr.trim()
            )));
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct AzureCliSubscription {
            id: String,
            name: String,
            state: String,
            tenant_id: String,
        }

        let cli_subscriptions: Vec<AzureCliSubscription> = serde_json::from_slice(&output.stdout)
            .map_err(|e| AzureError::ApiError(format!("Failed to parse Azure CLI subscriptions: {}", e)))?;

        let subscriptions = cli_subscriptions
            .into_iter()
            .map(|subscription| AzureSubscription {
                id: subscription.id,
                name: subscription.name,
                state: subscription.state,
                tenant_id: subscription.tenant_id,
            })
            .collect::<Vec<_>>();

        info!("Successfully fetched {} subscriptions", subscriptions.len());
        Ok(subscriptions)
    }

    /// List resource groups in a subscription
    pub async fn list_resource_groups(
        &self,
        subscription_id: &str,
    ) -> Result<Vec<AzureResourceGroup>> {
        info!("Fetching resource groups for subscription {}", subscription_id);
        
        let credential = self.auth.credential();
        
        // Create a resources client for the specific subscription
        let client = ResourcesClient::builder(credential.clone())
            .build()
            .map_err(|e| AzureError::ApiError(format!("Failed to create Azure client: {}", e)))?;
        
        let mut resource_groups = Vec::new();
        let mut stream = client
            .resource_groups_client()
            .list(subscription_id)
            .into_stream();
        
        use futures::StreamExt;
        while let Some(result) = stream.next().await {
            match result {
                Ok(page) => {
                    for rg in page.value {
                        let name = rg.name.as_deref().unwrap_or("unnamed");
                        let location = rg.location.as_str();
                        debug!("Found resource group: {} in {}", name, location);
                        
                        // Parse tags from JSON Value to HashMap
                        let tags = if let Some(tags_value) = &rg.tags {
                            if let Some(obj) = tags_value.as_object() {
                                obj.iter()
                                    .filter_map(|(k, v)| {
                                        v.as_str().map(|s| (k.clone(), s.to_string()))
                                    })
                                    .collect()
                            } else {
                                std::collections::HashMap::new()
                            }
                        } else {
                            std::collections::HashMap::new()
                        };
                        
                        let azure_rg = AzureResourceGroup {
                            id: rg.id.clone().unwrap_or_default(),
                            name: rg.name.clone().unwrap_or_else(|| "Unnamed".to_string()),
                            location: rg.location.clone(),
                            subscription_id: subscription_id.to_string(),
                            tags,
                        };
                        resource_groups.push(azure_rg);
                    }
                }
                Err(e) => {
                    warn!("Error fetching resource group page: {}", e);
                    return Err(AzureError::ApiError(format!("Failed to list resource groups: {}", e)));
                }
            }
        }
        
        info!("Successfully fetched {} resource groups", resource_groups.len());
        Ok(resource_groups)
    }

    /// List resources in a subscription
    pub async fn list_resources(&self, subscription_id: &str) -> Result<Vec<AzureResource>> {
        info!("Fetching resources for subscription {}", subscription_id);
        
        let credential = self.auth.credential();
        
        let client = ResourcesClient::builder(credential.clone())
            .build()
            .map_err(|e| AzureError::ApiError(format!("Failed to create Azure client: {}", e)))?;
        
        let mut resources = Vec::new();
        let mut stream = client
            .resources_client()
            .list(subscription_id)
            .into_stream();
        
        use futures::StreamExt;
        while let Some(result) = stream.next().await {
            match result {
                Ok(page) => {
                    for resource in page.value {
                        // Access nested fields through the GenericResourceExpanded structure
                        let res = &resource.generic_resource.resource;
                        
                        debug!("Found resource: {} ({})", 
                            res.name.as_deref().unwrap_or("unnamed"),
                            res.type_.as_deref().unwrap_or("unknown")
                        );
                        
                        // Extract resource group from resource ID
                        let resource_group = res.id.as_ref()
                            .and_then(|id| extract_resource_group(id))
                            .unwrap_or_else(|| "Unknown".to_string());
                        
                        // Parse tags from JSON Value to HashMap
                        let tags = if let Some(tags_value) = &res.tags {
                            if let Some(obj) = tags_value.as_object() {
                                obj.iter()
                                    .filter_map(|(k, v)| {
                                        v.as_str().map(|s| (k.clone(), s.to_string()))
                                    })
                                    .collect()
                            } else {
                                std::collections::HashMap::new()
                            }
                        } else {
                            std::collections::HashMap::new()
                        };
                        
                        let azure_resource = AzureResource {
                            id: res.id.clone().unwrap_or_default(),
                            name: res.name.clone().unwrap_or_else(|| "Unnamed".to_string()),
                            resource_type: res.type_.clone().unwrap_or_else(|| "Unknown".to_string()),
                            location: res.location.clone().unwrap_or_else(|| "Unknown".to_string()),
                            resource_group,
                            subscription_id: subscription_id.to_string(),
                            tags,
                        };
                        resources.push(azure_resource);
                    }
                }
                Err(e) => {
                    warn!("Error fetching resource page: {}", e);
                    return Err(AzureError::ApiError(format!("Failed to list resources: {}", e)));
                }
            }
        }
        
        info!("Successfully fetched {} resources", resources.len());
        Ok(resources)
    }
}

/// Extract resource group name from Azure resource ID
fn extract_resource_group(resource_id: &str) -> Option<String> {
    resource_id
        .split('/')
        .enumerate()
        .find(|(i, part)| *i > 0 && part.eq_ignore_ascii_case("resourceGroups"))
        .and_then(|(i, _)| resource_id.split('/').nth(i + 1))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_resource_group() {
        let id = "/subscriptions/sub-id/resourceGroups/my-rg/providers/Microsoft.Compute/virtualMachines/my-vm";
        assert_eq!(extract_resource_group(id), Some("my-rg".to_string()));
    }

    #[test]
    fn test_extract_resource_group_no_rg() {
        let id = "/subscriptions/sub-id/providers/Microsoft.Compute/virtualMachines/my-vm";
        assert_eq!(extract_resource_group(id), None);
    }

    #[test]
    fn test_extract_resource_group_empty() {
        let id = "";
        assert_eq!(extract_resource_group(id), None);
    }
}
