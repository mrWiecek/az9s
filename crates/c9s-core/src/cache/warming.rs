// Cache warming strategies for optimal startup performance
//
// This module provides strategies for pre-loading cache data
// to ensure fast UI rendering on application startup.

use crate::{cache::Cache, models::*, Result};
use tracing::{debug, info, warn};

/// Cache warming strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarmingStrategy {
    /// Load only subscriptions (fastest)
    Minimal,
    /// Load subscriptions and resource groups
    Standard,
    /// Load subscriptions, resource groups, and resources
    Full,
}

/// Cache warming statistics
#[derive(Debug, Clone, Default)]
pub struct WarmingStats {
    pub subscriptions_loaded: usize,
    pub resource_groups_loaded: usize,
    pub resources_loaded: usize,
    pub duration_ms: u64,
}

/// Cache warmer for pre-loading data
pub struct CacheWarmer<'a> {
    cache: &'a Cache,
}

impl<'a> CacheWarmer<'a> {
    /// Create a new cache warmer
    pub fn new(cache: &'a Cache) -> Self {
        Self { cache }
    }

    /// Warm cache with subscriptions from cache
    pub fn warm_subscriptions(&self) -> Result<Vec<Subscription>> {
        debug!("Warming cache with subscriptions");
        let repo = self.cache.repository();
        let subscriptions = repo.get_subscriptions()?;
        info!("Loaded {} subscriptions from cache", subscriptions.len());
        Ok(subscriptions)
    }

    /// Warm cache with resource groups for a subscription
    pub fn warm_resource_groups(&self, subscription_id: &str) -> Result<Vec<ResourceGroup>> {
        debug!("Warming cache with resource groups for {}", subscription_id);
        let repo = self.cache.repository();
        let resource_groups = repo.get_resource_groups(subscription_id)?;
        info!(
            "Loaded {} resource groups from cache",
            resource_groups.len()
        );
        Ok(resource_groups)
    }

    /// Warm cache with resources for a subscription
    pub fn warm_resources(&self, subscription_id: &str) -> Result<Vec<Resource>> {
        debug!("Warming cache with resources for {}", subscription_id);
        let repo = self.cache.repository();
        let filter = crate::cache::ResourceFilter::new().subscription(subscription_id);
        let resources = repo.get_resources(&filter)?;
        info!("Loaded {} resources from cache", resources.len());
        Ok(resources)
    }

    /// Warm cache based on strategy
    pub fn warm(&self, strategy: WarmingStrategy) -> Result<WarmingStats> {
        let start = std::time::Instant::now();
        let mut stats = WarmingStats::default();

        // Always load subscriptions
        let subscriptions = self.warm_subscriptions()?;
        stats.subscriptions_loaded = subscriptions.len();

        if strategy == WarmingStrategy::Minimal {
            stats.duration_ms = start.elapsed().as_millis() as u64;
            return Ok(stats);
        }

        // Load resource groups for each subscription
        for sub in &subscriptions {
            match self.warm_resource_groups(&sub.id) {
                Ok(rgs) => stats.resource_groups_loaded += rgs.len(),
                Err(e) => warn!("Failed to warm resource groups for {}: {}", sub.name, e),
            }
        }

        if strategy == WarmingStrategy::Standard {
            stats.duration_ms = start.elapsed().as_millis() as u64;
            return Ok(stats);
        }

        // Load resources for each subscription
        for sub in &subscriptions {
            match self.warm_resources(&sub.id) {
                Ok(resources) => stats.resources_loaded += resources.len(),
                Err(e) => warn!("Failed to warm resources for {}: {}", sub.name, e),
            }
        }

        stats.duration_ms = start.elapsed().as_millis() as u64;
        info!(
            "Cache warming completed in {}ms: {} subs, {} rgs, {} resources",
            stats.duration_ms,
            stats.subscriptions_loaded,
            stats.resource_groups_loaded,
            stats.resources_loaded
        );

        Ok(stats)
    }

    /// Check if cache has data for a subscription
    pub fn has_subscription_data(&self, subscription_id: &str) -> bool {
        let repo = self.cache.repository();
        repo.get_subscription(subscription_id)
            .map(|opt| opt.is_some())
            .unwrap_or(false)
    }

    /// Check if cache is empty
    pub fn is_cache_empty(&self) -> bool {
        self.cache
            .stats()
            .map(|stats| {
                stats.subscription_count == 0
                    && stats.resource_group_count == 0
                    && stats.resource_count == 0
            })
            .unwrap_or(true)
    }

    /// Get cache age for a subscription (seconds since last update)
    ///
    /// Note: This is a placeholder for future enhancement.
    /// Currently returns None as we don't have a query to get cache timestamp.
    pub fn get_cache_age(&self, _subscription_id: &str) -> Option<i64> {
        // Future enhancement: Add repository method to get cache timestamp
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Cache;
    use std::collections::HashMap;
    use tempfile::NamedTempFile;

    fn setup_cache_with_data() -> (NamedTempFile, Cache) {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = Cache::new(temp_file.path()).unwrap();
        let repo = cache.repository();

        // Add test subscription
        let sub = Subscription {
            id: "sub-1".to_string(),
            name: "Test Sub".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "tenant-1".to_string(),
        };
        repo.cache_subscription(&sub).unwrap();

        // Add test resource group
        let rg = ResourceGroup {
            id: "rg-1".to_string(),
            name: "test-rg".to_string(),
            location: "eastus".to_string(),
            subscription_id: "sub-1".to_string(),
            tags: HashMap::new(),
        };
        repo.cache_resource_group(&rg).unwrap();

        // Add test resource
        let resource = Resource {
            id: "res-1".to_string(),
            name: "test-vm".to_string(),
            resource_type: "Microsoft.Compute/virtualMachines".to_string(),
            location: "eastus".to_string(),
            resource_group: "test-rg".to_string(),
            subscription_id: "sub-1".to_string(),
            tags: HashMap::new(),
        };
        repo.cache_resource(&resource).unwrap();

        (temp_file, cache)
    }

    #[test]
    fn test_warm_minimal() {
        let (_temp, cache) = setup_cache_with_data();
        let warmer = CacheWarmer::new(&cache);

        let stats = warmer.warm(WarmingStrategy::Minimal).unwrap();
        assert_eq!(stats.subscriptions_loaded, 1);
        assert_eq!(stats.resource_groups_loaded, 0);
        assert_eq!(stats.resources_loaded, 0);
    }

    #[test]
    fn test_warm_standard() {
        let (_temp, cache) = setup_cache_with_data();
        let warmer = CacheWarmer::new(&cache);

        let stats = warmer.warm(WarmingStrategy::Standard).unwrap();
        assert_eq!(stats.subscriptions_loaded, 1);
        assert_eq!(stats.resource_groups_loaded, 1);
        assert_eq!(stats.resources_loaded, 0);
    }

    #[test]
    fn test_warm_full() {
        let (_temp, cache) = setup_cache_with_data();
        let warmer = CacheWarmer::new(&cache);

        let stats = warmer.warm(WarmingStrategy::Full).unwrap();
        assert_eq!(stats.subscriptions_loaded, 1);
        assert_eq!(stats.resource_groups_loaded, 1);
        assert_eq!(stats.resources_loaded, 1);
    }

    #[test]
    fn test_has_subscription_data() {
        let (_temp, cache) = setup_cache_with_data();
        let warmer = CacheWarmer::new(&cache);

        assert!(warmer.has_subscription_data("sub-1"));
        assert!(!warmer.has_subscription_data("sub-999"));
    }

    #[test]
    fn test_is_cache_empty() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = Cache::new(temp_file.path()).unwrap();
        let warmer = CacheWarmer::new(&cache);

        assert!(warmer.is_cache_empty());

        // Add data
        let repo = cache.repository();
        let sub = Subscription {
            id: "sub-1".to_string(),
            name: "Test".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "tenant-1".to_string(),
        };
        repo.cache_subscription(&sub).unwrap();

        assert!(!warmer.is_cache_empty());
    }
}
