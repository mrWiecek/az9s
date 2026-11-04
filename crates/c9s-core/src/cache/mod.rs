// Cache module for SQLite-backed Azure resource caching
//
// This module provides:
// - Schema migrations with versioning
// - Repository pattern for data access
// - TTL-based cache invalidation
// - Optimized queries for terminal rendering
// - Cache warming strategies

mod migrations;
mod repository;
mod scoped;
mod warming;
mod writer;

use crate::Result;
use rusqlite::Connection;
use std::path::Path;
use tracing::info;

pub use repository::{CacheRepository, ResourceFilter};
pub use scoped::Cached;
pub use warming::{CacheWarmer, WarmingStats, WarmingStrategy};
pub use writer::WriteJob;

// ==================== Cache ====================

/// SQLite-backed cache for Azure resources with migration support
pub struct Cache {
    conn: Connection,
    /// Background writer for file-backed caches (None for `:memory:`)
    writer: Option<writer::Writer>,
}

/// Switch the database to WAL mode (stored in the file, so this is a no-op
/// after the first run). Changing the journal mode fails at once with "database
/// is locked" while another connection uses the file — the busy timeout does
/// not cover it — so when two processes start together on a new cache file,
/// retry briefly.
fn enable_wal(conn: &Connection) -> Result<()> {
    let mut attempts = 0;
    loop {
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        if mode.eq_ignore_ascii_case("wal") {
            return Ok(());
        }
        match conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get::<_, String>(0)) {
            Ok(_) => return Ok(()),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::DatabaseBusy && attempts < 100 =>
            {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

impl Cache {
    /// Create a new cache instance with automatic migrations.
    ///
    /// A file-backed cache runs in WAL mode and gets a background writer
    /// thread, so large writes never block the caller (see [`Cache::write`]).
    pub fn new(path: &Path) -> Result<Self> {
        info!("Opening cache at {:?}", path);
        let conn = Connection::open(path)?;
        let in_memory = path == Path::new(":memory:");
        if !in_memory {
            // Another c9s process may hold the lock (e.g. while migrating):
            // wait for it rather than fail. Set before anything that locks.
            conn.busy_timeout(std::time::Duration::from_secs(10))?;
            // WAL lets this connection read while the writer thread writes.
            enable_wal(&conn)?;
        }

        let mut cache = Self { conn, writer: None };
        cache.run_migrations()?;
        if !in_memory {
            cache.writer = Some(writer::Writer::spawn(path)?);
        }

        Ok(cache)
    }

    /// Replace a cached scope. File-backed caches do this on the background
    /// writer thread and return immediately; in-memory caches write now.
    pub fn write(&self, job: WriteJob) {
        match &self.writer {
            Some(writer) => writer.send(job),
            None => {
                if let Err(e) = job.apply(&self.repository()) {
                    tracing::warn!("Cache write failed: {}", e);
                }
            }
        }
    }

    /// Wait until all queued background writes are done.
    pub fn flush(&self) {
        if let Some(writer) = &self.writer {
            writer.flush();
        }
    }

    /// Run database migrations
    fn run_migrations(&self) -> Result<()> {
        migrations::run_migrations(&self.conn)?;
        Ok(())
    }

    /// Get a repository for data access
    pub fn repository(&self) -> CacheRepository<'_> {
        CacheRepository::new(&self.conn)
    }

    /// Get a cache warmer for pre-loading data
    pub fn warmer(&self) -> CacheWarmer<'_> {
        CacheWarmer::new(self)
    }

    /// Clear all cached data
    pub fn clear_all(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            DELETE FROM resources;
            DELETE FROM resource_groups;
            DELETE FROM subscriptions;
            DELETE FROM kv;
            "#,
        )?;
        info!("Cleared all cached data");
        Ok(())
    }

    /// Clear expired cache entries based on TTL
    pub fn clear_expired(&self, ttl_seconds: i64) -> Result<usize> {
        let cutoff = chrono::Utc::now().timestamp() - ttl_seconds;

        let mut deleted = 0;
        deleted += self
            .conn
            .execute("DELETE FROM subscriptions WHERE cached_at < ?1", [cutoff])?;
        deleted += self
            .conn
            .execute("DELETE FROM resource_groups WHERE cached_at < ?1", [cutoff])?;
        deleted += self
            .conn
            .execute("DELETE FROM resources WHERE cached_at < ?1", [cutoff])?;

        if deleted > 0 {
            info!("Cleared {} expired cache entries", deleted);
        }

        Ok(deleted)
    }

    /// Get cache statistics
    pub fn stats(&self) -> Result<CacheStats> {
        let subscription_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM subscriptions", [], |row| row.get(0))?;

        let resource_group_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM resource_groups", [], |row| row.get(0))?;

        let resource_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM resources", [], |row| row.get(0))?;

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
    use crate::models::{Resource, ResourceGroup, Subscription};
    use std::collections::HashMap;
    use tempfile::NamedTempFile;

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

        // First create the subscription (required by foreign key)
        let sub = Subscription {
            id: "sub-1".to_string(),
            name: "Test Sub".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "tenant-1".to_string(),
        };
        repo.cache_subscription(&sub).unwrap();

        // Now create the resource
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

    #[test]
    fn test_warmer() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = Cache::new(temp_file.path()).unwrap();

        // Add test data
        let repo = cache.repository();
        let sub = Subscription {
            id: "sub-1".to_string(),
            name: "Test".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "tenant-1".to_string(),
        };
        repo.cache_subscription(&sub).unwrap();

        // Test warmer
        let warmer = cache.warmer();
        assert!(!warmer.is_cache_empty());
        assert!(warmer.has_subscription_data("sub-1"));

        let stats = warmer.warm(WarmingStrategy::Minimal).unwrap();
        assert_eq!(stats.subscriptions_loaded, 1);
    }
}

#[cfg(test)]
mod writer_tests {
    use super::*;
    use crate::models::*;
    use std::time::{Duration, Instant};

    fn temp_db(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("c9s-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("cache.db")
    }

    fn rows(n: usize) -> Vec<Resource> {
        (0..n)
            .map(|i| Resource {
                id: format!("/subscriptions/s/resourceGroups/rg/providers/x/r{i}"),
                name: format!("r{i}"),
                resource_type: "x".into(),
                location: "westeurope".into(),
                resource_group: "rg".into(),
                subscription_id: "s".into(),
                tags: Default::default(),
            })
            .collect()
    }

    #[test]
    fn file_cache_writes_in_background_and_flush_waits_for_them() {
        let path = temp_db("writer");
        let cache = Cache::new(&path).unwrap();
        cache.write(WriteJob::Subscriptions(vec![Subscription {
            id: "s".into(),
            name: "s".into(),
            state: "Enabled".into(),
            tenant_id: "t".into(),
        }]));

        let started = Instant::now();
        cache.write(WriteJob::Resources {
            subscription_ids: vec!["s".into()],
            resource_group: None,
            items: rows(20_000),
        });
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "write must not block the caller ({:?})",
            started.elapsed()
        );

        cache.flush();
        let got = cache
            .repository()
            .cached_resources(&["s".into()], None)
            .unwrap();
        assert_eq!(got.items.len(), 20_000);
        drop(cache);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn dropping_the_cache_finishes_queued_writes() {
        let path = temp_db("drop");
        {
            let cache = Cache::new(&path).unwrap();
            cache.write(WriteJob::Subscriptions(vec![Subscription {
                id: "s".into(),
                name: "s".into(),
                state: "Enabled".into(),
                tenant_id: "t".into(),
            }]));
        }
        let cache = Cache::new(&path).unwrap();
        assert_eq!(cache.repository().get_subscriptions().unwrap().len(), 1);
        drop(cache);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// Two processes starting at once on the same cache file (issue #9).
    #[test]
    fn caches_opened_concurrently_on_one_file_all_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c9s.db");
        for _ in 0..10 {
            let _ = std::fs::remove_file(&path);
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let path = path.clone();
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        barrier.wait();
                        Cache::new(&path).map(|_| ())
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap().expect("cache must open");
            }
        }
    }
}
