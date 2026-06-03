# Data Layer Architecture

## Overview

The aztui data layer provides a robust SQLite-backed caching system for Azure resource metadata. It implements a repository pattern for clean data access, automatic schema migrations, and cache invalidation strategies optimized for terminal rendering performance.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Application Layer                      │
│                    (aztui-core::App)                        │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                      Cache Layer                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ Cache                                                │  │
│  │  ├─ new() - Initialize with migrations              │  │
│  │  ├─ repository() - Get data access layer            │  │
│  │  ├─ clear_expired() - TTL-based invalidation        │  │
│  │  ├─ stats() - Cache statistics                      │  │
│  │  └─ vacuum() - Database optimization                │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                   Repository Layer                          │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ CacheRepository                                      │  │
│  │  ├─ Subscriptions (CRUD)                            │  │
│  │  ├─ Resource Groups (CRUD)                          │  │
│  │  ├─ Resources (CRUD + Filtering)                    │  │
│  │  ├─ Resource Details (Extended properties)          │  │
│  │  └─ Aggregations (counts, locations)                │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                    Migration System                         │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ migrations::run_migrations()                         │  │
│  │  ├─ Version tracking                                 │  │
│  │  ├─ Idempotent migrations                           │  │
│  │  └─ Forward-only schema evolution                   │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                      SQLite Database                        │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ Tables:                                              │  │
│  │  ├─ schema_migrations                               │  │
│  │  ├─ subscriptions                                   │  │
│  │  ├─ resource_groups                                 │  │
│  │  ├─ resources                                       │  │
│  │  └─ resource_details                                │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Database Schema

### Schema Version 1 (Current)

#### schema_migrations
Tracks applied migrations for version control.

```sql
CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at INTEGER NOT NULL
);
```

#### subscriptions
Stores Azure subscription metadata.

```sql
CREATE TABLE subscriptions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    state TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    cached_at INTEGER NOT NULL
);

CREATE INDEX idx_subscriptions_cached_at ON subscriptions(cached_at);
```

#### resource_groups
Stores resource group metadata with foreign key to subscriptions.

```sql
CREATE TABLE resource_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    location TEXT NOT NULL,
    subscription_id TEXT NOT NULL,
    tags TEXT,  -- JSON serialized HashMap
    cached_at INTEGER NOT NULL,
    FOREIGN KEY (subscription_id) REFERENCES subscriptions(id) ON DELETE CASCADE
);

CREATE INDEX idx_resource_groups_subscription ON resource_groups(subscription_id);
CREATE INDEX idx_resource_groups_cached_at ON resource_groups(cached_at);
CREATE INDEX idx_resource_groups_name ON resource_groups(name);
```

#### resources
Stores resource metadata with multiple indexes for fast queries.

```sql
CREATE TABLE resources (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    location TEXT NOT NULL,
    resource_group TEXT NOT NULL,
    subscription_id TEXT NOT NULL,
    tags TEXT,  -- JSON serialized HashMap
    cached_at INTEGER NOT NULL,
    FOREIGN KEY (subscription_id) REFERENCES subscriptions(id) ON DELETE CASCADE
);

CREATE INDEX idx_resources_subscription ON resources(subscription_id);
CREATE INDEX idx_resources_rg ON resources(resource_group);
CREATE INDEX idx_resources_type ON resources(resource_type);
CREATE INDEX idx_resources_location ON resources(location);
CREATE INDEX idx_resources_cached_at ON resources(cached_at);
CREATE INDEX idx_resources_name ON resources(name);
```

#### resource_details
Stores extended resource properties for detail views.

```sql
CREATE TABLE resource_details (
    resource_id TEXT PRIMARY KEY,
    properties TEXT NOT NULL,  -- JSON serialized properties
    sku TEXT,
    kind TEXT,
    cached_at INTEGER NOT NULL,
    FOREIGN KEY (resource_id) REFERENCES resources(id) ON DELETE CASCADE
);

CREATE INDEX idx_resource_details_cached_at ON resource_details(cached_at);
```

## Cache API

### Initialization

```rust
use aztui_core::cache::Cache;
use std::path::Path;

// Create cache with automatic migrations
let cache = Cache::new(Path::new("~/.local/share/aztui/aztui.db"))?;
```

### Repository Pattern

```rust
// Get repository for data access
let repo = cache.repository();

// Cache subscriptions
repo.cache_subscriptions(&subscriptions)?;

// Retrieve subscriptions
let subs = repo.get_subscriptions()?;

// Cache resource groups
repo.cache_resource_groups(&resource_groups)?;

// Get resource groups for a subscription
let rgs = repo.get_resource_groups("subscription-id")?;
```

### Filtering Resources

```rust
use aztui_core::cache::ResourceFilter;

// Build a filter with fluent API
let filter = ResourceFilter::new()
    .subscription("sub-123")
    .resource_type("Microsoft.Compute/virtualMachines")
    .location("eastus")
    .name_contains("prod")
    .limit(100);

// Query with filter
let resources = repo.get_resources(&filter)?;
```

### Cache Management

```rust
// Get cache statistics
let stats = cache.stats()?;
println!("Cached {} resources", stats.resource_count);
println!("Database size: {} bytes", stats.db_size_bytes);

// Clear expired entries (older than 1 hour)
let deleted = cache.clear_expired(3600)?;
println!("Cleared {} expired entries", deleted);

// Clear all cached data
cache.clear_all()?;

// Vacuum database to reclaim space
cache.vacuum()?;
```

## Cache Invalidation Strategy

### Time-Based Expiration (TTL)

Each cached entry has a `cached_at` timestamp. The application can:

1. **Manual refresh**: User presses 'r' to force refresh
2. **Automatic expiration**: Background task clears entries older than TTL
3. **Lazy invalidation**: Check timestamp on read, fetch if expired

### Recommended TTL Values

- **Subscriptions**: 1 hour (rarely change)
- **Resource Groups**: 30 minutes (moderate change rate)
- **Resources**: 5 minutes (frequent changes)
- **Resource Details**: 2 minutes (most volatile)

### Implementation Example

```rust
use std::time::Duration;

// In background task
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
    
    loop {
        interval.tick().await;
        
        // Clear expired entries
        if let Err(e) = cache.clear_expired(300) {
            tracing::error!("Failed to clear expired cache: {}", e);
        }
    }
});
```

## Fast Read Paths for Terminal Rendering

### Optimized Queries

The schema includes strategic indexes for common UI queries:

1. **List subscriptions** (sorted by name)
   ```sql
   SELECT id, name, state, tenant_id FROM subscriptions ORDER BY name
   ```

2. **List resource groups** (filtered by subscription, sorted by name)
   ```sql
   SELECT * FROM resource_groups 
   WHERE subscription_id = ? 
   ORDER BY name
   ```

3. **List resources** (multiple filters, sorted by name)
   ```sql
   SELECT * FROM resources 
   WHERE subscription_id = ? 
     AND resource_type = ? 
     AND location = ?
   ORDER BY name
   LIMIT 100
   ```

4. **Resource type counts** (for summary views)
   ```sql
   SELECT resource_type, COUNT(*) 
   FROM resources 
   WHERE subscription_id = ? 
   GROUP BY resource_type
   ```

### Performance Characteristics

- **Subscription list**: O(1) - typically < 10 items
- **Resource group list**: O(log n) - indexed by subscription_id
- **Resource list**: O(log n) - multiple indexes for filtering
- **Resource detail**: O(1) - primary key lookup

### Pagination Support

For large result sets, use the `limit` parameter:

```rust
let filter = ResourceFilter::new()
    .subscription("sub-123")
    .limit(100);  // First 100 results

let resources = repo.get_resources(&filter)?;
```

## Migration System

### Adding a New Migration

1. Increment `CURRENT_VERSION` in `migrations.rs`
2. Add migration function:

```rust
fn migration_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        ALTER TABLE resources ADD COLUMN provisioning_state TEXT;
        CREATE INDEX idx_resources_state ON resources(provisioning_state);
        "#,
    )?;
    Ok(())
}
```

3. Add case to `apply_migration()`:

```rust
match version {
    1 => migration_v1(conn)?,
    2 => migration_v2(conn)?,
    _ => panic!("Unknown migration version: {}", version),
}
```

### Migration Best Practices

- **Idempotent**: Use `IF NOT EXISTS` for CREATE statements
- **Forward-only**: Never modify existing migrations
- **Transactional**: Each migration runs in a transaction
- **Tested**: Add tests for each migration
- **Documented**: Comment complex schema changes

## Serialization Strategy

### JSON for Complex Types

Tags and properties are serialized as JSON:

```rust
// Serialize
let tags_json = serde_json::to_string(&resource.tags)?;

// Deserialize with fallback
let tags: HashMap<String, String> = serde_json::from_str(&tags_json)
    .unwrap_or_default();
```

### Benefits

- **Flexible schema**: No need to add columns for each tag
- **Efficient storage**: JSON is compact
- **Easy querying**: SQLite has JSON functions (future enhancement)

### Future: JSON Queries

SQLite supports JSON queries for advanced filtering:

```sql
SELECT * FROM resources 
WHERE json_extract(tags, '$.environment') = 'production'
```

## Error Handling

### Error Types

```rust
pub enum CoreError {
    Cache(rusqlite::Error),
    Serialization(serde_json::Error),
    // ...
}
```

### Error Recovery

1. **Database locked**: Retry with exponential backoff
2. **Corrupted data**: Log error, return empty result
3. **Migration failure**: Panic (requires manual intervention)
4. **Disk full**: Clear expired entries, vacuum database

## Testing Strategy

### Unit Tests

Each repository method has unit tests:

```rust
#[test]
fn test_subscription_cache() {
    let conn = setup_test_db();
    let repo = CacheRepository::new(&conn);
    
    let sub = Subscription { /* ... */ };
    repo.cache_subscription(&sub).unwrap();
    
    let subs = repo.get_subscriptions().unwrap();
    assert_eq!(subs.len(), 1);
}
```

### Integration Tests

Test with real Azure data:

```rust
#[tokio::test]
async fn test_cache_azure_resources() {
    let cache = Cache::new(temp_path()).unwrap();
    let client = AzureClient::new().await.unwrap();
    
    // Fetch from Azure
    let resources = client.list_resources("sub-123").await.unwrap();
    
    // Cache
    cache.repository().cache_resources(&resources).unwrap();
    
    // Verify
    let cached = cache.repository().get_resources(&filter).unwrap();
    assert_eq!(cached.len(), resources.len());
}
```

### Performance Tests

Benchmark common queries:

```rust
#[bench]
fn bench_list_resources(b: &mut Bencher) {
    let cache = setup_cache_with_1000_resources();
    let filter = ResourceFilter::new().subscription("sub-123");
    
    b.iter(|| {
        cache.repository().get_resources(&filter).unwrap()
    });
}
```

## Monitoring and Observability

### Logging

All cache operations are logged:

```rust
use tracing::{debug, info, error};

debug!("Cached {} subscriptions", subscriptions.len());
info!("Cleared {} expired cache entries", deleted);
error!("Failed to cache resource: {}", e);
```

### Metrics

Track cache performance:

```rust
let stats = cache.stats()?;
tracing::info!(
    subscription_count = stats.subscription_count,
    resource_count = stats.resource_count,
    db_size_bytes = stats.db_size_bytes,
    "Cache statistics"
);
```

## Future Enhancements

### Phase 1 (Current)
- ✅ Basic schema with migrations
- ✅ Repository pattern
- ✅ TTL-based invalidation
- ✅ Resource filtering

### Phase 2
- ⏳ JSON query support for tags
- ⏳ Full-text search on resource names
- ⏳ Pagination with cursors
- ⏳ Cache warming on startup

### Phase 3
- ⏳ Incremental updates (delta sync)
- ⏳ Compression for large properties
- ⏳ Multi-tenant support
- ⏳ Cache replication

## Troubleshooting

### Database Locked

**Symptom**: `rusqlite::Error::SqliteFailure(5, "database is locked")`

**Solution**: 
- Ensure only one Cache instance per process
- Use transactions for bulk operations
- Increase busy timeout: `conn.busy_timeout(Duration::from_secs(5))`

### Slow Queries

**Symptom**: List operations take > 100ms

**Solution**:
- Run `ANALYZE` to update query planner statistics
- Vacuum database to defragment
- Add indexes for common filters
- Limit result sets with pagination

### Corrupted Database

**Symptom**: `rusqlite::Error::DatabaseCorrupt`

**Solution**:
- Delete database file and re-fetch from Azure
- Implement automatic backup/restore
- Use SQLite integrity check: `PRAGMA integrity_check`

### High Memory Usage

**Symptom**: Process memory grows over time

**Solution**:
- Vacuum database regularly
- Clear expired entries more frequently
- Limit cache size with LRU eviction
- Use streaming queries for large result sets

## References

- [SQLite Documentation](https://www.sqlite.org/docs.html)
- [rusqlite Crate](https://docs.rs/rusqlite/)
- [Repository Pattern](https://martinfowler.com/eaaCatalog/repository.html)
- [Database Migrations](https://en.wikipedia.org/wiki/Schema_migration)
