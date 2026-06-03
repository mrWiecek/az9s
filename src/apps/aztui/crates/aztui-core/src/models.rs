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
