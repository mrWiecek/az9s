use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Azure subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub name: String,
    pub state: String,
    pub tenant_id: String,
}

/// Azure resource group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceGroup {
    pub id: String,
    pub name: String,
    pub location: String,
    pub subscription_id: String,
    pub tags: HashMap<String, String>,
}

/// Generic Azure resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub name: String,
    pub resource_type: String,
    pub location: String,
    pub resource_group: String,
    pub subscription_id: String,
    pub tags: HashMap<String, String>,
}

/// Resource detail with extended properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDetail {
    pub resource: Resource,
    pub properties: serde_json::Value,
    pub sku: Option<String>,
    pub kind: Option<String>,
}

impl From<&az9s_azure::models::AzureSubscription> for Subscription {
    fn from(s: &az9s_azure::models::AzureSubscription) -> Self {
        Self {
            id: s.id.clone(),
            name: s.name.clone(),
            state: s.state.clone(),
            tenant_id: s.tenant_id.clone(),
        }
    }
}

impl From<&az9s_azure::models::AzureResourceGroup> for ResourceGroup {
    fn from(rg: &az9s_azure::models::AzureResourceGroup) -> Self {
        Self {
            id: rg.id.clone(),
            name: rg.name.clone(),
            location: rg.location.clone(),
            subscription_id: rg.subscription_id.clone(),
            tags: rg.tags.clone(),
        }
    }
}

impl From<&az9s_azure::models::AzureResource> for Resource {
    fn from(r: &az9s_azure::models::AzureResource) -> Self {
        Self {
            id: r.id.clone(),
            name: r.name.clone(),
            resource_type: r.resource_type.clone(),
            location: r.location.clone(),
            resource_group: r.resource_group.clone(),
            subscription_id: r.subscription_id.clone(),
            tags: r.tags.clone(),
        }
    }
}
