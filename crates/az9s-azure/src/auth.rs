use crate::Result;
use azure_core::auth::TokenCredential;
use azure_identity::{AzureCliCredential, DefaultAzureCredential};
use std::sync::Arc;
use tracing::{info, warn};

/// Authentication provider trait
pub trait AuthProvider: Send + Sync {
    fn credential(&self) -> Arc<dyn TokenCredential>;
}

/// Azure authentication manager
pub struct AzureAuth {
    credential: Arc<dyn TokenCredential>,
}

impl AzureAuth {
    /// Create a new authentication instance using Azure CLI
    pub fn new_cli() -> Result<Self> {
        info!("Initializing Azure CLI authentication");
        let credential = Arc::new(AzureCliCredential::new());
        Ok(Self { credential })
    }

    /// Create a new authentication instance using default credential chain
    pub fn new_default() -> Result<Self> {
        info!("Initializing default Azure authentication");
        let credential = Arc::new(DefaultAzureCredential::default());
        Ok(Self { credential })
    }

    /// Try multiple authentication methods in order
    pub async fn new_auto() -> Result<Self> {
        info!("Attempting automatic authentication");

        // Try Azure CLI first (most common for operators)
        match Self::new_cli() {
            Ok(auth) => {
                info!("Using Azure CLI authentication");
                return Ok(auth);
            }
            Err(e) => {
                warn!("Azure CLI authentication not available: {}", e);
            }
        }

        // Fall back to default credential chain
        info!("Falling back to default credential chain");
        Self::new_default()
    }
}

impl AuthProvider for AzureAuth {
    fn credential(&self) -> Arc<dyn TokenCredential> {
        self.credential.clone()
    }
}
