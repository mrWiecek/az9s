// Cache module for SQLite-backed Azure resource caching
//
// This module provides:
// - Schema migrations with versioning
// - Repository pattern for data access
// - TTL-based cache invalidation
// - Optimized queries for terminal rendering

mod migrations;
mod repository;

use crate::{models::*, Result};
use rusqlite::Connection;
use std::path::Path;
use tracing::{debug, info};

pub use repository::{CacheRepository, ResourceFilter};

// ==================== Cache ====================

/// SQLite-backed cache for Azure resources with migration support
pub struct Cache {
    conn: Connection,
}

impl Cache {
    /// Create a new cache instance with automatic migrations
    pub fn new(path: &Path) -> Result<Self> {
        info!("Opening cache at {:?}", path);
        let conn = Connection::open(path)?;
        
        let cache = Self { conn };
        cache.run_migrations()?;
        
        Ok(cache)
    }

    /// Run database migrations
    fn run_migrations(&self) -> Result<()> {
        migrations::run_migrations(&self.conn)?;
        Ok(())
    }

    /// Get a repository for data access
    pub fn repository(&self) -> CacheRepository {
        CacheRepository::new(&self.conn)
    }

    /// Clear all cached data
    pub fn clear_all(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            DELETE FROM resources;
            DELETE FROM resource_groups;
            DELETE FROM subscriptions;
            "#,
        )?;
        info!("Cleared all cached data");
        Ok(())
    }

    /// Clear expired cache entries based on TTL
    pub fn clear_expired(&self, ttl_seconds: i64) -> Result<usize> {
        let cutoff = chrono::Utc::now().timestamp() - ttl_seconds;
        
        let mut deleted = 0;
        deleted += self.conn.execute(
            "DELETE FROM subscriptions WHERE cached_at < ?1",
            [cutoff],
        )?;
        deleted += self.conn.execute(
            "DELETE FROM resource_groups WHERE cached_at < ?1",
            [cutoff],
        )?;
        deleted += self.conn.execute(
            "DELETE FROM resources WHERE cached_at < ?1",
            [cutoff],
        )?;
        
        if deleted > 0 {
            info!("Cleared {} expired cache entries", deleted);
        }
        
        Ok(deleted)
    }

    /// Get cache statistics
    pub fn stats(&self) -> Result<CacheStats> {
        let subscription_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM subscriptions",
            [],
            |row| row.get(0),
        )?;
        
        let resource_group_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM resource_groups",
            [],
            |row| row.get(0),
        )?;
        
        let resource_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM resources",
            [],
            |row| row.get(0),
        )?;
        
        let db_size: i64 = self.conn.query_row(
            "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
            [],
            |row| row.get(0),
        )?;
        
        Ok(CacheStats {
            subscription_count: subscription_count as usize,
            resource_group_count: resource_group_count as usize,
            resource_count: resource_count as usize,
            db_size_bytes: db_size as usize,
        })
    }

    /// Vacuum the database to reclaim space
    pub fn vacuum(&self) -> Result<()> {
        info!("Vacuuming database");
        self.conn.execute_batch("VACUUM")?;
        Ok(())
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub subscription_count: usize,
    pub resource_group_count: usize,
    pub resource_count: usize,
    pub db_size_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::collections::HashMap;

    #[test]
    fn test_cache_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = Cache::new(temp_file.path()).unwrap();
        
        let stats = cache.stats().unwrap();
        assert_eq!(stats.subscription_count, 0);
        assert_eq!(stats.resource_group_count, 0);
        assert_eq!(stats.resource_count, 0);
    }

    #[test]
    fn test_cache_clear() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = Cache::new(temp_file.path()).unwrap();
        
        // Add some data
        let repo = cache.repository();
        let sub = Subscription {
            id: "sub-1".to_string(),
            name: "Test".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "tenant-1".to_string(),
        };
        repo.cache_subscription(&sub).unwrap();
        
        // Clear and verify
        cache.clear_all().unwrap();
        let stats = cache.stats().unwrap();
        assert_eq!(stats.subscription_count, 0);
    }

    #[test]
    fn test_cache_ttl() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = Cache::new(temp_file.path()).unwrap();
        
        // Add data
        let repo = cache.repository();
        let sub = Subscription {
            id: "sub-1".to_string(),
            name: "Test".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "tenant-1".to_string(),
        };
        repo.cache_subscription(&sub).unwrap();
        
        // Clear with future TTL (nothing should be deleted)
        let deleted = cache.clear_expired(3600).unwrap();
        assert_eq!(deleted, 0);
        
        // Clear with past TTL (everything should be deleted)
        let deleted = cache.clear_expired(-1).unwrap();
        assert_eq!(deleted, 1);
    }

    #[test]
    fn test_repository_integration() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = Cache::new(temp_file.path()).unwrap();
        let repo = cache.repository();
        
        // Test subscription
        let sub = Subscription {
            id: "sub-1".to_string(),
            name: "Test Sub".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "tenant-1".to_string(),
        };
        repo.cache_subscription(&sub).unwrap();
        
        // Test resource group
        let rg = ResourceGroup {
            id: "rg-1".to_string(),
            name: "test-rg".to_string(),
            location: "eastus".to_string(),
            subscription_id: "sub-1".to_string(),
            tags: HashMap::new(),
        };
        repo.cache_resource_group(&rg).unwrap();
        
        // Test resource
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
        
        // Verify stats
        let stats = cache.stats().unwrap();
        assert_eq!(stats.subscription_count, 1);
        assert_eq!(stats.resource_group_count, 1);
        assert_eq!(stats.resource_count, 1);
    }

    #[test]
    fn test_resource_filter() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = Cache::new(temp_file.path()).unwrap();
        let repo = cache.repository();
        
        let resource = Resource {
            id: "res-1".to_string(),
            name: "test-vm".to_string(),
            resource_type: "Microsoft.Compute/virtualMachines".to_string(),
            location: "eastus".to_string(),
            resource_group: "rg-1".to_string(),
            subscription_id: "sub-1".to_string(),
            tags: HashMap::new(),
        };
        repo.cache_resource(&resource).unwrap();
        
        let filter = ResourceFilter::new()
            .subscription("sub-1")
            .resource_type("Microsoft.Compute/virtualMachines");
        
        let resources = repo.get_resources(&filter).unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].name, "test-vm");
    }

    #[test]
    fn test_migrations_idempotent() {
        let temp_file = NamedTempFile::new().unwrap();
        
        // Create cache twice - migrations should be idempotent
        let cache1 = Cache::new(temp_file.path()).unwrap();
        drop(cache1);
        
        let cache2 = Cache::new(temp_file.path()).unwrap();
        let stats = cache2.stats().unwrap();
        assert_eq!(stats.subscription_count, 0);
    }

    #[test]
    fn test_vacuum() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = Cache::new(temp_file.path()).unwrap();
        
        // Add and remove data to create fragmentation
        let repo = cache.repository();
        let sub = Subscription {
            id: "sub-1".to_string(),
            name: "Test".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "tenant-1".to_string(),
        };
        repo.cache_subscription(&sub).unwrap();
        cache.clear_all().unwrap();
        
        // Vacuum should succeed
        cache.vacuum().unwrap();
    }
}
