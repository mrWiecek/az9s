use crate::{
    cache::Cache,
    state::{AppState, DataSource},
    timing, CoreError, Result,
};
use anyhow::Context;
use az9s_azure::{AzureAuth, AzureClient};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

/// Main application coordinator
pub struct App {
    state: AppState,
    cache: Cache,
    azure_client: Option<Arc<AzureClient>>,
}

impl App {
    /// Create a new application instance
    pub async fn new() -> anyhow::Result<Self> {
        let _t = timing::timer("startup.app_init");
        info!("Initializing az9s application");

        // Determine cache directory
        let cache_dir = Self::cache_dir()?;
        std::fs::create_dir_all(&cache_dir).context("Failed to create cache directory")?;

        // Initialize cache
        let cache_path = cache_dir.join("az9s.db");
        let cache = Cache::new(&cache_path)?;

        // Initialize state
        let mut state = AppState::new();

        // Attempt Azure authentication and load subscriptions
        let azure_client = match Self::initialize_azure(&cache, &mut state).await {
            Ok(client) => Some(client),
            Err(e) => {
                warn!("Azure initialization failed: {}", e);
                state.set_error(Some(format!("Azure authentication failed: {}", e)));
                None
            }
        };

        Ok(Self {
            state,
            cache,
            azure_client,
        })
    }

    /// Create an application with no Azure connection, backed by `cache`.
    ///
    /// Used by tests and benchmarks today; also the basis for rendering cached
    /// data before Azure answers (M1 "cache-first").
    pub fn offline(cache: Cache) -> Self {
        Self {
            state: AppState::new(),
            cache,
            azure_client: None,
        }
    }

    /// Initialize Azure authentication and load subscriptions
    async fn initialize_azure(cache: &Cache, state: &mut AppState) -> Result<Arc<AzureClient>> {
        info!("Initializing Azure authentication");

        // Try automatic authentication (Azure CLI first, then default credential chain)
        let auth = AzureAuth::new_auto().await?;
        let client = Arc::new(AzureClient::new(Arc::new(auth)));

        // Load subscriptions on startup
        info!("Loading subscriptions from Azure");
        state.set_loading(true);

        let fetched = {
            let _t = timing::timer("azure.list_subscriptions");
            client.list_subscriptions().await
        };
        match fetched {
            Ok(azure_subscriptions) => {
                info!(
                    "Successfully loaded {} subscriptions",
                    azure_subscriptions.len()
                );

                // Convert Azure subscriptions to core model
                let subscriptions: Vec<crate::models::Subscription> = azure_subscriptions
                    .iter()
                    .map(|s| crate::models::Subscription {
                        id: s.id.clone(),
                        name: s.name.clone(),
                        state: s.state.clone(),
                        tenant_id: s.tenant_id.clone(),
                    })
                    .collect();

                // Cache subscriptions for offline access
                let repo = cache.repository();
                let cached = {
                    let _t = timing::timer("cache.cache_subscriptions");
                    repo.cache_subscriptions(&subscriptions)
                };
                if let Err(e) = cached {
                    warn!("Failed to cache subscriptions: {}", e);
                }

                // Update state with loaded subscriptions
                state.subscriptions = subscriptions;
                state.set_loading(false);
                state.set_data_source(DataSource::Azure);

                Ok(client)
            }
            Err(e) => {
                warn!("Failed to load subscriptions from Azure: {}", e);

                // Try to load from cache as fallback
                let repo = cache.repository();
                match repo.get_subscriptions() {
                    Ok(cached_subs) if !cached_subs.is_empty() => {
                        info!("Loaded {} subscriptions from cache", cached_subs.len());
                        state.subscriptions = cached_subs;
                        state.set_loading(false);
                        state.set_data_source(DataSource::Cache);
                        state.set_error(Some(format!(
                            "Using cached data (Azure API unavailable: {})",
                            e
                        )));
                        Ok(client)
                    }
                    _ => {
                        state.set_loading(false);
                        Err(CoreError::Azure(e))
                    }
                }
            }
        }
    }

    /// Get the application state
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Get mutable application state
    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    /// Get the cache
    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// Get the Azure client if available
    pub fn azure_client(&self) -> Option<Arc<AzureClient>> {
        self.azure_client.clone()
    }

    /// Refresh subscriptions from Azure
    pub async fn refresh_subscriptions(&mut self) -> Result<()> {
        let client = self
            .azure_client
            .as_ref()
            .ok_or_else(|| CoreError::InvalidState("Azure client not initialized".to_string()))?;

        info!("Refreshing subscriptions");
        self.state.set_loading(true);
        self.state.clear_error();

        let fetched = {
            let _t = timing::timer("azure.list_subscriptions");
            client.list_subscriptions().await
        };
        match fetched {
            Ok(azure_subscriptions) => {
                let subscriptions: Vec<crate::models::Subscription> = azure_subscriptions
                    .iter()
                    .map(|s| crate::models::Subscription {
                        id: s.id.clone(),
                        name: s.name.clone(),
                        state: s.state.clone(),
                        tenant_id: s.tenant_id.clone(),
                    })
                    .collect();

                // Cache the refreshed subscriptions
                let repo = self.cache.repository();
                {
                    let _t = timing::timer("cache.cache_subscriptions");
                    repo.cache_subscriptions(&subscriptions)
                }?;

                // Update state
                self.state.subscriptions = subscriptions;
                self.state.set_loading(false);
                self.state.set_data_source(DataSource::Azure);

                info!("Successfully refreshed subscriptions");
                Ok(())
            }
            Err(e) => {
                self.state.set_loading(false);
                self.state
                    .set_error(Some(format!("Failed to refresh: {}", e)));
                Err(CoreError::Azure(e))
            }
        }
    }

    /// Load resource groups for a subscription
    pub async fn load_resource_groups(&mut self, subscription_id: &str) -> Result<()> {
        let client = self
            .azure_client
            .as_ref()
            .ok_or_else(|| CoreError::InvalidState("Azure client not initialized".to_string()))?;

        info!(
            "Loading resource groups for subscription {}",
            subscription_id
        );
        self.state.set_loading(true);
        self.state.clear_error();

        let fetched = {
            let _t = timing::timer("azure.list_resource_groups");
            client.list_resource_groups(subscription_id).await
        };
        match fetched {
            Ok(azure_rgs) => {
                let resource_groups: Vec<crate::models::ResourceGroup> = azure_rgs
                    .iter()
                    .map(|rg| crate::models::ResourceGroup {
                        id: rg.id.clone(),
                        name: rg.name.clone(),
                        location: rg.location.clone(),
                        subscription_id: rg.subscription_id.clone(),
                        tags: rg.tags.clone(),
                    })
                    .collect();

                // Cache resource groups
                let repo = self.cache.repository();
                {
                    let _t = timing::timer("cache.cache_resource_groups");
                    repo.cache_resource_groups(&resource_groups)
                }?;

                // Update state with loaded resource groups
                self.state.resource_groups = resource_groups;
                self.state.set_loading(false);
                self.state.set_data_source(DataSource::Azure);

                info!(
                    "Successfully loaded {} resource groups",
                    self.state.resource_groups.len()
                );
                Ok(())
            }
            Err(e) => {
                self.state.set_loading(false);
                self.state
                    .set_error(Some(format!("Failed to load resource groups: {}", e)));
                Err(CoreError::Azure(e))
            }
        }
    }

    /// Load resources for a subscription
    pub async fn load_resources(&mut self, subscription_id: &str) -> Result<()> {
        let client = self
            .azure_client
            .as_ref()
            .ok_or_else(|| CoreError::InvalidState("Azure client not initialized".to_string()))?;

        info!("Loading resources for subscription {}", subscription_id);
        self.state.set_loading(true);
        self.state.clear_error();

        let fetched = {
            let _t = timing::timer("azure.list_resources");
            client.list_resources(subscription_id).await
        };
        match fetched {
            Ok(azure_resources) => {
                let resources: Vec<crate::models::Resource> = azure_resources
                    .iter()
                    .map(|r| crate::models::Resource {
                        id: r.id.clone(),
                        name: r.name.clone(),
                        resource_type: r.resource_type.clone(),
                        location: r.location.clone(),
                        resource_group: r.resource_group.clone(),
                        subscription_id: r.subscription_id.clone(),
                        tags: r.tags.clone(),
                    })
                    .collect();

                // Cache resources
                let repo = self.cache.repository();
                {
                    let _t = timing::timer("cache.cache_resources");
                    repo.cache_resources(&resources)
                }?;

                // Update state
                self.state.resources = resources;
                self.state.set_loading(false);
                self.state.set_data_source(DataSource::Azure);

                info!(
                    "Successfully loaded {} resources",
                    self.state.resources.len()
                );
                Ok(())
            }
            Err(e) => {
                self.state.set_loading(false);
                self.state
                    .set_error(Some(format!("Failed to load resources: {}", e)));
                Err(CoreError::Azure(e))
            }
        }
    }

    /// Determine the cache directory (platform-specific local data dir)
    fn cache_dir() -> anyhow::Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("", "", "az9s")
            .context("Could not determine data directory")?;
        Ok(dirs.data_local_dir().to_path_buf())
    }
}
