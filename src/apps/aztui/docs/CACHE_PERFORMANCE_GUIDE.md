# Cache Performance Optimization Guide

## Overview

This guide provides strategies for optimizing cache performance in aztui, focusing on fast read paths for terminal rendering, efficient cache warming, and minimizing latency.

## Performance Goals

- **Startup time**: < 100ms to first render with cached data
- **List rendering**: < 10ms for typical subscription/resource lists
- **Cache warming**: < 500ms for full cache load
- **Memory footprint**: < 50MB for typical Azure environments

## Cache Warming Strategies

### Startup Warming

The cache warmer provides three strategies optimized for different use cases:

#### Minimal Strategy (Fastest)
```rust
use aztui_core::cache::{Cache, WarmingStrategy};

let cache = Cache::new(cache_path)?;
let warmer = cache.warmer();

// Load only subscriptions (~10ms)
let stats = warmer.warm(WarmingStrategy::Minimal)?;
println!("Loaded {} subscriptions in {}ms", 
    stats.subscriptions_loaded, 
    stats.duration_ms
);
```

**Use when:**
- Fast startup is critical
- User will navigate to specific subscriptions
- Network is available for on-demand loading

**Performance:**
- Typical: 5-15ms
- Subscriptions: 1-50 items

#### Standard Strategy (Balanced)
```rust
// Load subscriptions + resource groups (~50ms)
let stats = warmer.warm(WarmingStrategy::Standard)?;
println!("Loaded {} subs, {} RGs in {}ms",
    stats.subscriptions_loaded,
    stats.resource_groups_loaded,
    stats.duration_ms
);
```

**Use when:**
- Balanced startup time and data availability
- Resource group navigation is common
- Moderate cache size

**Performance:**
- Typical: 30-100ms
- Resource groups: 10-500 items

#### Full Strategy (Complete)
```rust
// Load everything (~200ms)
let stats = warmer.warm(WarmingStrategy::Full)?;
println!("Loaded {} subs, {} RGs, {} resources in {}ms",
    stats.subscriptions_loaded,
    stats.resource_groups_loaded,
    stats.resources_loaded,
    stats.duration_ms
);
```

**Use when:**
- Offline mode is required
- Full data browsing without network
- Large cache is acceptable

**Performance:**
- Typical: 100-500ms
- Resources: 100-10,000 items

### Adaptive Warming

Choose strategy based on cache size:

```rust
fn choose_warming_strategy(cache: &Cache) -> WarmingStrategy {
    let stats = cache.stats().unwrap();
    
    if stats.resource_count > 5000 {
        // Large cache - load incrementally
        WarmingStrategy::Minimal
    } else if stats.resource_count > 500 {
        // Medium cache - load subscriptions and RGs
        WarmingStrategy::Standard
    } else {
        // Small cache - load everything
        WarmingStrategy::Full
    }
}

let strategy = choose_warming_strategy(&cache);
let warmer = cache.warmer();
warmer.warm(strategy)?;
```

## Query Optimization

### Use Indexes Effectively

The schema includes strategic indexes for common queries:

```sql
-- Subscription lookup (O(1))
SELECT * FROM subscriptions WHERE id = ?

-- Resource groups by subscription (O(log n))
SELECT * FROM resource_groups WHERE subscription_id = ?

-- Resources by type (O(log n))
SELECT * FROM resources WHERE resource_type = ?

-- Resources by location (O(log n))
SELECT * FROM resources WHERE location = ?
```

### Filter in Database, Not Application

**Good:**
```rust
// Filter in database (fast)
let filter = ResourceFilter::new()
    .subscription("sub-123")
    .resource_type("Microsoft.Compute/virtualMachines")
    .location("eastus")
    .limit(100);

let vms = repo.get_resources(&filter)?;
```

**Bad:**
```rust
// Filter in application (slow)
let all = repo.get_resources(&ResourceFilter::new())?;
let vms: Vec<_> = all.into_iter()
    .filter(|r| r.subscription_id == "sub-123")
    .filter(|r| r.resource_type == "Microsoft.Compute/virtualMachines")
    .filter(|r| r.location == "eastus")
    .take(100)
    .collect();
```

### Batch Operations

**Good:**
```rust
// Single transaction for all resources
repo.cache_resources(&resources)?;
```

**Bad:**
```rust
// Individual transactions (100x slower)
for resource in resources {
    repo.cache_resource(&resource)?;
}
```

### Limit Result Sets

Always use limits for large queries:

```rust
let filter = ResourceFilter::new()
    .subscription("sub-123")
    .limit(1000);  // Prevent loading 10,000+ items

let resources = repo.get_resources(&filter)?;
```

## Memory Management

### Cache Size Monitoring

Monitor cache size and clean up when needed:

```rust
use std::time::Duration;
use tokio::time::interval;

async fn monitor_cache_size(cache: Cache) {
    let mut interval = interval(Duration::from_secs(60));
    
    loop {
        interval.tick().await;
        
        let stats = cache.stats().unwrap();
        let size_mb = stats.db_size_bytes / (1024 * 1024);
        
        if size_mb > 100 {
            tracing::warn!("Cache size: {}MB, consider cleanup", size_mb);
            
            // Clear old entries
            cache.clear_expired(3600).unwrap();
            cache.vacuum().unwrap();
        }
    }
}
```

### Vacuum Strategy

Vacuum periodically to reclaim space:

```rust
async fn periodic_vacuum(cache: Cache) {
    let mut interval = interval(Duration::from_secs(3600)); // 1 hour
    
    loop {
        interval.tick().await;
        
        if let Err(e) = cache.vacuum() {
            tracing::error!("Vacuum failed: {}", e);
        }
    }
}
```

## TTL-Based Invalidation

### Recommended TTL Values

Different data types have different change frequencies:

```rust
const SUBSCRIPTION_TTL: i64 = 3600;      // 1 hour (rarely change)
const RESOURCE_GROUP_TTL: i64 = 1800;    // 30 minutes
const RESOURCE_TTL: i64 = 300;           // 5 minutes
const RESOURCE_DETAIL_TTL: i64 = 120;    // 2 minutes (most volatile)
```

### Automatic Cleanup

Run background cleanup task:

```rust
use tokio::time::{interval, Duration};

pub fn spawn_cache_cleanup(cache: Cache) {
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(300)); // 5 minutes
        
        loop {
            interval.tick().await;
            
            match cache.clear_expired(300) {
                Ok(deleted) if deleted > 0 => {
                    tracing::info!("Cleared {} expired entries", deleted);
                }
                Err(e) => {
                    tracing::error!("Cleanup failed: {}", e);
                }
                _ => {}
            }
        }
    });
}
```

### Lazy Invalidation

Check age on read and refresh if needed:

```rust
async fn get_resources_with_ttl(
    cache: &Cache,
    azure_client: &AzureClient,
    subscription_id: &str,
    ttl_seconds: i64,
) -> Result<Vec<Resource>> {
    let filter = ResourceFilter::new().subscription(subscription_id);
    let cached = cache.repository().get_resources(&filter)?;
    
    // Check if cache is fresh (simplified - would need timestamp query)
    if cached.is_empty() {
        // Cache miss - fetch from Azure
        let resources = azure_client.list_resources(subscription_id).await?;
        cache.repository().cache_resources(&resources)?;
        Ok(resources)
    } else {
        Ok(cached)
    }
}
```

## Rendering Optimization

### Pagination for Large Lists

Render only visible items:

```rust
struct ListView {
    offset: usize,
    page_size: usize,
}

impl ListView {
    fn render(&self, resources: &[Resource]) {
        let end = (self.offset + self.page_size).min(resources.len());
        let visible = &resources[self.offset..end];
        
        for resource in visible {
            // Render only visible items
            println!("{}", resource.name);
        }
    }
}
```

### Incremental Loading

Load data as user scrolls:

```rust
async fn load_page(
    cache: &Cache,
    subscription_id: &str,
    offset: usize,
    limit: usize,
) -> Result<Vec<Resource>> {
    // Note: Current implementation doesn't support offset
    // This is a future enhancement
    let filter = ResourceFilter::new()
        .subscription(subscription_id)
        .limit(limit);
    
    cache.repository().get_resources(&filter)
}
```

### Caching Aggregations

Pre-compute expensive aggregations:

```rust
// Cache resource type counts
let counts = repo.get_resource_type_counts(subscription_id)?;

// Display summary without re-querying
for (resource_type, count) in counts {
    println!("{}: {}", resource_type, count);
}
```

## Benchmarking

### Measure Query Performance

```rust
use std::time::Instant;

fn benchmark_query(cache: &Cache, subscription_id: &str) {
    let start = Instant::now();
    
    let filter = ResourceFilter::new().subscription(subscription_id);
    let resources = cache.repository().get_resources(&filter).unwrap();
    
    let duration = start.elapsed();
    println!("Loaded {} resources in {:?}", resources.len(), duration);
}
```

### Expected Performance

| Operation | Items | Expected Time |
|-----------|-------|---------------|
| List subscriptions | 10 | < 1ms |
| List resource groups | 100 | < 5ms |
| List resources (filtered) | 1,000 | < 20ms |
| List resources (all) | 10,000 | < 100ms |
| Get resource by ID | 1 | < 1ms |
| Resource type counts | 50 types | < 10ms |

### Performance Testing

```rust
#[cfg(test)]
mod perf_tests {
    use super::*;
    use std::time::Instant;
    
    #[test]
    fn test_list_performance() {
        let cache = setup_cache_with_1000_resources();
        let filter = ResourceFilter::new().subscription("sub-123");
        
        let start = Instant::now();
        let resources = cache.repository().get_resources(&filter).unwrap();
        let duration = start.elapsed();
        
        assert_eq!(resources.len(), 1000);
        assert!(duration.as_millis() < 50, "Query too slow: {:?}", duration);
    }
}
```

## Troubleshooting Performance Issues

### Slow Queries

**Symptom:** List operations take > 100ms

**Diagnosis:**
```rust
// Enable SQLite query logging
cache.conn.trace(Some(|sql| {
    println!("SQL: {}", sql);
}));
```

**Solutions:**
1. Run ANALYZE to update statistics:
   ```sql
   ANALYZE;
   ```

2. Check index usage:
   ```sql
   EXPLAIN QUERY PLAN SELECT * FROM resources WHERE subscription_id = ?;
   ```

3. Add missing indexes if needed

### High Memory Usage

**Symptom:** Process memory grows over time

**Diagnosis:**
```rust
let stats = cache.stats()?;
println!("Cache size: {} MB", stats.db_size_bytes / (1024 * 1024));
```

**Solutions:**
1. Vacuum database:
   ```rust
   cache.vacuum()?;
   ```

2. Clear expired entries:
   ```rust
   cache.clear_expired(3600)?;
   ```

3. Implement LRU eviction for large caches

### Database Locked

**Symptom:** `rusqlite::Error::SqliteFailure(5, "database is locked")`

**Solutions:**
1. Increase busy timeout:
   ```rust
   conn.busy_timeout(Duration::from_secs(5))?;
   ```

2. Use WAL mode for better concurrency:
   ```sql
   PRAGMA journal_mode=WAL;
   ```

3. Ensure single Cache instance per process

## Best Practices Summary

1. **Choose appropriate warming strategy** based on use case
2. **Filter in database** using ResourceFilter
3. **Batch operations** for bulk inserts/updates
4. **Limit result sets** to prevent loading too much data
5. **Monitor cache size** and vacuum periodically
6. **Use TTL-based invalidation** for automatic cleanup
7. **Paginate large lists** in UI rendering
8. **Benchmark critical paths** to ensure performance goals
9. **Enable WAL mode** for better concurrency
10. **Cache aggregations** to avoid repeated queries

## Future Optimizations

### Phase 1 (Current)
- ✅ Strategic indexes
- ✅ Batch operations
- ✅ TTL-based invalidation
- ✅ Cache warming strategies

### Phase 2
- ⏳ Offset-based pagination
- ⏳ Full-text search on names
- ⏳ JSON query support for tags
- ⏳ Query result caching

### Phase 3
- ⏳ Incremental delta sync
- ⏳ Compression for large properties
- ⏳ LRU eviction policy
- ⏳ Read-through cache pattern

## References

- [SQLite Performance Tuning](https://www.sqlite.org/optoverview.html)
- [rusqlite Documentation](https://docs.rs/rusqlite/)
- [Database Indexing Best Practices](https://use-the-index-luke.com/)
