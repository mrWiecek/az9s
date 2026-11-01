use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Azure subscription from API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureSubscription {
    pub id: String,
    pub name: String,
    pub state: String,
    pub tenant_id: String,
}

/// Azure resource group from API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureResourceGroup {
    pub id: String,
    pub name: String,
    pub location: String,
    pub subscription_id: String,
    pub tags: HashMap<String, String>,
}

/// Azure resource from API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureResource {
    pub id: String,
    pub name: String,
    pub resource_type: String,
    pub location: String,
    pub resource_group: String,
    pub subscription_id: String,
    pub tags: HashMap<String, String>,
}
