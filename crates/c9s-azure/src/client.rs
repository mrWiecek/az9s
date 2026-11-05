use crate::resource_graph::{CredentialTokenSource, TokenSource, DEFAULT_ENDPOINT};
use crate::{auth::AuthProvider, models::*, AzureError, Result};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Azure Resource Manager lists (the `C9S_BACKEND=arm` fallback) and the
/// CLI's subscriptions.
pub struct AzureClient {
    http: reqwest::Client,
    endpoint: String,
    tokens: Arc<dyn TokenSource>,
}

impl AzureClient {
    /// ARM with the Azure CLI's login.
    pub fn new(auth: Arc<dyn AuthProvider>) -> Self {
        Self::with_endpoint(
            DEFAULT_ENDPOINT,
            Arc::new(CredentialTokenSource(auth.credential())),
        )
    }

    /// Another endpoint (tests).
    pub fn with_endpoint(endpoint: &str, tokens: Arc<dyn TokenSource>) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint: endpoint.trim_end_matches('/').to_string(),
            tokens,
        }
    }

    /// List all subscriptions
    pub async fn list_subscriptions(&self) -> Result<Vec<AzureSubscription>> {
        info!("Fetching subscriptions from Azure CLI");

        let output =
            crate::cli::az_command("az", &["account", "list", "--all", "--output", "json"])
                .map_err(|e| AzureError::ApiError(crate::cli::start_error(&e)))?
                .output()
                .await
                .map_err(|e| AzureError::ApiError(crate::cli::start_error(&e)))?;

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
            .map_err(|e| {
            AzureError::ApiError(format!("Failed to parse Azure CLI subscriptions: {}", e))
        })?;

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
        info!("Fetching resource groups for subscription {subscription_id}");
        let path = format!(
            "/subscriptions/{}/resourcegroups?api-version={ARM_API_VERSION}",
            segment(subscription_id)?
        );
        let groups = self
            .list_all(&path)
            .await?
            .into_iter()
            .map(|rg| AzureResourceGroup {
                id: text(&rg, "id"),
                name: Some(text(&rg, "name"))
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "Unnamed".into()),
                location: text(&rg, "location"),
                subscription_id: subscription_id.to_string(),
                tags: tags_to_map(rg.get("tags")),
            })
            .collect::<Vec<_>>();
        info!("Fetched {} resource groups", groups.len());
        Ok(groups)
    }

    /// List resources in a subscription
    pub async fn list_resources(&self, subscription_id: &str) -> Result<Vec<AzureResource>> {
        info!("Fetching resources for subscription {subscription_id}");
        let path = format!(
            "/subscriptions/{}/resources?api-version={ARM_API_VERSION}",
            segment(subscription_id)?
        );
        Ok(resources(self.list_all(&path).await?, subscription_id))
    }

    /// List resources in a single resource group (server-side filtered)
    pub async fn list_resources_in_group(
        &self,
        subscription_id: &str,
        resource_group: &str,
    ) -> Result<Vec<AzureResource>> {
        info!("Fetching resources for resource group {resource_group} in {subscription_id}");
        let path = format!(
            "/subscriptions/{}/resourceGroups/{}/resources?api-version={ARM_API_VERSION}",
            segment(subscription_id)?,
            segment(resource_group)?
        );
        Ok(resources(self.list_all(&path).await?, subscription_id))
    }

    /// Every page of an ARM list (`value`, `nextLink`). A `nextLink` must
    /// stay on the ARM endpoint: the token is never sent elsewhere.
    async fn list_all(&self, path: &str) -> Result<Vec<serde_json::Value>> {
        let token = self.tokens.token().await?;
        let mut url = format!("{}{path}", self.endpoint);
        let mut items = Vec::new();
        loop {
            let response = self
                .http
                .get(&url)
                .bearer_auth(&token)
                .send()
                .await
                .map_err(|e| AzureError::ApiError(format!("ARM request failed: {e}")))?;
            let status = response.status();
            let body = response
                .text()
                .await
                .map_err(|e| AzureError::ApiError(format!("ARM response: {e}")))?;
            if !status.is_success() {
                warn!("ARM list failed: HTTP {}", status.as_u16());
                return Err(AzureError::Http {
                    status: status.as_u16(),
                    message: crate::resource_graph::error_message(&body),
                });
            }
            let page: serde_json::Value = serde_json::from_str(&body)
                .map_err(|e| AzureError::ApiError(format!("invalid ARM response: {e}")))?;
            if let Some(serde_json::Value::Array(values)) = page.get("value") {
                items.extend(values.iter().cloned());
            }
            match page.get("nextLink").and_then(|l| l.as_str()) {
                Some(next) if !next.is_empty() => url = next_page(next, &self.endpoint)?,
                _ => break,
            }
        }
        debug!("Fetched {} items", items.len());
        Ok(items)
    }
}

/// ARM's API version for resource and resource group lists.
const ARM_API_VERSION: &str = "2021-04-01";

/// A subscription ID or resource group name as one URL path segment:
/// refused if it could change the path (`/`, `?`, `#`, `..`).
fn segment(value: &str) -> Result<&str> {
    let bad = value.is_empty()
        || value == "."
        || value == ".."
        || value
            .chars()
            .any(|c| matches!(c, '/' | '\\' | '?' | '#' | '%') || c.is_control());
    if bad {
        Err(AzureError::ApiError(format!(
            "invalid name in an Azure path: {value:?}"
        )))
    } else {
        Ok(value)
    }
}

/// The next page's URL, if it stays on `endpoint`'s scheme and host: a
/// paging link from a response must never carry the token elsewhere.
pub fn next_page(next: &str, endpoint: &str) -> Result<String> {
    if same_origin(next, endpoint) {
        Ok(next.to_string())
    } else {
        Err(AzureError::ApiError(format!(
            "refused to follow a nextLink to another host: {next}"
        )))
    }
}

/// An ARM ID from a response, as a path below the endpoint: it must start
/// with `/` and can't leave the path (`?`, `#`, `..`, `//`, `@`, `\\`).
pub fn arm_path(id: &str) -> Result<&str> {
    let ok = id.starts_with('/')
        && !id.starts_with("//")
        && !id.contains("/../")
        && !id.ends_with("/..")
        && !id
            .chars()
            .any(|c| matches!(c, '?' | '#' | '@' | '\\' | '%') || c.is_control() || c == ' ');
    if ok {
        Ok(id)
    } else {
        Err(AzureError::InvalidConfig(format!("not an ARM ID: {id}")))
    }
}

/// Whether `url` is on the same scheme and host as `endpoint`.
pub fn same_origin(url: &str, endpoint: &str) -> bool {
    fn origin(u: &str) -> Option<(&str, &str)> {
        let (scheme, rest) = u.split_once("://")?;
        let host = rest.split(['/', '?', '#']).next()?;
        Some((scheme, host))
    }
    match (origin(url), origin(endpoint)) {
        (Some((s1, h1)), Some((s2, h2))) => {
            s1.eq_ignore_ascii_case(s2) && h1.eq_ignore_ascii_case(h2) && !h1.contains('@')
        }
        _ => false,
    }
}

fn text(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string()
}

/// ARM resources as `AzureResource`s.
fn resources(items: Vec<serde_json::Value>, subscription_id: &str) -> Vec<AzureResource> {
    items
        .iter()
        .map(|r| {
            let id = text(r, "id");
            AzureResource {
                resource_group: extract_resource_group(&id).unwrap_or_else(|| "Unknown".into()),
                name: Some(text(r, "name"))
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "Unnamed".into()),
                resource_type: Some(text(r, "type"))
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| "Unknown".into()),
                location: Some(text(r, "location"))
                    .filter(|l| !l.is_empty())
                    .unwrap_or_else(|| "Unknown".into()),
                subscription_id: subscription_id.to_string(),
                tags: tags_to_map(r.get("tags")),
                id,
            }
        })
        .collect()
}

/// Convert ARM's JSON tag object into a string map (non-string values are dropped)
fn tags_to_map(tags: Option<&serde_json::Value>) -> std::collections::HashMap<String, String> {
    tags.and_then(|t| t.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
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
    fn next_links_stay_on_the_endpoint() {
        let arm = "https://management.azure.com";
        assert!(same_origin(
            "https://management.azure.com/subscriptions/s/resources?$skiptoken=x",
            arm
        ));
        assert!(!same_origin("https://evil.example/steal", arm));
        assert!(!same_origin("http://management.azure.com/x", arm));
        assert!(!same_origin(
            "https://management.azure.com@evil.example/x",
            arm
        ));
        assert!(!same_origin(
            "https://management.azure.com.evil.example/x",
            arm
        ));
    }

    #[test]
    fn arm_ids_stay_paths_on_the_endpoint() {
        let id = "/providers/Microsoft.Authorization/roleDefinitions/acdd72a7";
        assert_eq!(arm_path(id).unwrap(), id);
        for bad in [
            "@evil.example/providers/Microsoft.Authorization/roleDefinitions/x",
            "//evil.example/x",
            "/a/../../x",
            "/x?y=1",
            "/x#y",
            "x/y",
        ] {
            assert!(arm_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn path_segments_cannot_change_the_path() {
        assert!(segment("rg-prod").is_ok());
        assert!(segment("00000000-0000-0000-0000-000000000000").is_ok());
        for bad in ["", "..", "a/b", "a?b", "a#b", "%2e%2e"] {
            assert!(segment(bad).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn resources_are_listed_across_pages_on_the_endpoint_only() {
        use crate::testing::{ok, serve_paths, StaticToken};
        use std::sync::Mutex;
        let me = Arc::new(Mutex::new(String::new()));
        let base = me.clone();
        let (endpoint, _) = serve_paths(move |path, _| {
            if path.contains("skiptoken") {
                return ok(serde_json::json!({"value": [
                    {"id": "/subscriptions/s/resourceGroups/rg/providers/A/b/two", "name": "two",
                     "type": "A/b", "location": "westeurope", "tags": {"env": "prod"}}
                ]}));
            }
            let next = format!(
                "{}/subscriptions/s/resources?$skiptoken=2",
                base.lock().unwrap()
            );
            ok(serde_json::json!({"value": [
                {"id": "/subscriptions/s/resourceGroups/rg/providers/A/b/one", "name": "one",
                 "type": "A/b", "location": "westeurope"}
            ], "nextLink": next}))
        })
        .await;
        *me.lock().unwrap() = endpoint.clone();
        let client = AzureClient::with_endpoint(&endpoint, Arc::new(StaticToken("t".into())));
        let found = client.list_resources("s").await.unwrap();
        assert_eq!(found.len(), 2, "both pages");
        assert_eq!(found[1].resource_group, "rg");
        assert_eq!(found[1].tags.get("env").map(String::as_str), Some("prod"));

        // A nextLink to another host is refused rather than followed with
        // the token.
        let (evil, _) = serve_paths(|_, _| {
            ok(serde_json::json!({"value": [], "nextLink": "https://evil.example/x"}))
        })
        .await;
        let client = AzureClient::with_endpoint(&evil, Arc::new(StaticToken("t".into())));
        let err = client.list_resources("s").await.unwrap_err();
        assert!(err.to_string().contains("another host"), "{err}");
    }

    #[test]
    fn test_extract_resource_group_empty() {
        let id = "";
        assert_eq!(extract_resource_group(id), None);
    }
}
