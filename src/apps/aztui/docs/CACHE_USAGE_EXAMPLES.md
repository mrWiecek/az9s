# Cache Usage Examples

## Overview

This document provides practical examples of using the aztui cache layer with the new repository pattern.

## Basic Setup

```rust
use aztui_core::cache::{Cache, CacheRepository, ResourceFilter};
use aztui_core::models::{Subscription, ResourceGroup, Resource};
use std::path::Path;

// Initialize cache
let cache = Cache::new(Path::new("~/.local/share/aztui/aztui.db"))?;

// Get repository for data access
let repo = cache.repository();
```

## Working with Subscriptions

### Cache Subscriptions from Azure

```rust
use aztui_azure::AzureClient;

// Fetch from Azure
let azure_client = AzureClient::new().await?;
let subscriptions = azure_client.list_subscriptions().await?;

// Cache locally
repo.cache_subscriptions(&subscriptions)?;
```

### Read Subscriptions from Cache

```rust
// Get all subscriptions (fast, from cache)
let subscriptions = repo.get_subscriptions()?;

for sub in subscriptions {
    println!("{}: {}", sub.name, sub.state);
}
```

### Get Single Subscription

```rust
// Lookup by ID
if let Some(sub) = repo.get_subscription("subscription-id-123")? {
    println!("Found: {}", sub.name);
}
```

## Working with Resource Groups

### Cache Resource Groups

```rust
// Fetch from Azure
let resource_groups = azure_client
    .list_resource_groups("subscription-id")
    .await?;

// Cache locally
repo.cache_resource_groups(&resource_groups)?;
```

### Read Resource Groups

```rust
// Get all resource groups for a subscription
let rgs = repo.get_resource_groups("subscription-id")?;

for rg in rgs {
    println!("{} ({})", rg.name, rg.location);
}
```

### Get Single Resource Group

```rust
// Lookup by subscription and name
if let Some(rg) = repo.get_resource_group("sub-id", "my-rg")? {
    println!("Location: {}", rg.location);
    println!("Tags: {:?}", rg.tags);
}
```

## Working with Resources

### Cache Resources

```rust
// Fetch from Azure
let resources = azure_client
    .list_resources("subscription-id")
    .await?;

// Cache locally (bulk operation)
repo.cache_resources(&resources)?;
```

### Query with Filters

```rust
use aztui_core::cache::ResourceFilter;

// Build filter with fluent API
let filter = ResourceFilter::new()
    .subscription("subscription-id")
    .resource_type("Microsoft.Compute/virtualMachines")
    .location("eastus")
    .name_contains("prod")
    .limit(100);

// Execute query
let resources = repo.get_resources(&filter)?;

for resource in resources {
    println!("{}: {} ({})", 
        resource.name, 
        resource.resource_type,
        resource.location
    );
}
```

### Filter by Resource Group

```rust
let filter = ResourceFilter::new()
    .subscription("sub-123")
    .resource_group("my-resource-group");

let resources = repo.get_resources(&filter)?;
```

### Search by Name

```rust
let filter = ResourceFilter::new()
    .subscription("sub-123")
    .name_contains("database");

let databases = repo.get_resources(&filter)?;
```

### Get Single Resource

```rust
let resource_id = "/subscriptions/sub-123/resourceGroups/rg-1/providers/Microsoft.Compute/virtualMachines/vm-1";

if let Some(resource) = repo.get_resource(resource_id)? {
    println!("Name: {}", resource.name);
    println!("Type: {}", resource.resource_type);
    println!("Tags: {:?}", resource.tags);
}
```

## Aggregations and Analytics

### Resource Type Counts

```rust
// Get count of each resource type in a subscription
let counts = repo.get_resource_type_counts("subscription-id")?;

for (resource_type, count) in counts {
    println!("{}: {} resources", resource_type, count);
}

// Example output:
// Microsoft.Compute/virtualMachines: 15 resources
// Microsoft.Storage/storageAccounts: 8 resources
// Microsoft.Network/virtualNetworks: 5 resources
```

### Unique Locations

```rust
// Get all locations used in a subscription
let locations = repo.get_locations("subscription-id")?;

println!("Resources deployed in: {}", locations.join(", "));
// Example: eastus, westus2, centralus
```

## Resource Details

### Cache Extended Properties

```rust
use aztui_core::models::ResourceDetail;

// Fetch detailed properties from Azure
let detail = azure_client
    .get_resource_detail(resource_id)
    .await?;

// Cache with extended properties
repo.cache_resource_detail(&detail)?;
```

### Read Resource Details

```rust
if let Some(detail) = repo.get_resource_detail(resource_id)? {
    println!("Resource: {}", detail.resource.name);
    println!("SKU: {:?}", detail.sku);
    println!("Kind: {:?}", detail.kind);
    println!("Properties: {}", serde_json::to_string_pretty(&detail.properties)?);
}
```

## Cache Management

### Get Statistics

```rust
let stats = cache.stats()?;

println!("Cache Statistics:");
println!("  Subscriptions: {}", stats.subscription_count);
println!("  Resource Groups: {}", stats.resource_group_count);
println!("  Resources: {}", stats.resource_count);
println!("  Database Size: {} KB", stats.db_size_bytes / 1024);
```

### Clear Expired Entries

```rust
// Clear entries older than 1 hour (3600 seconds)
let deleted = cache.clear_expired(3600)?;
println!("Cleared {} expired entries", deleted);
```

### Clear All Data

```rust
// Clear entire cache (useful for logout or reset)
cache.clear_all()?;
println!("Cache cleared");
```

### Vacuum Database

```rust
// Reclaim disk space after deletions
cache.vacuum()?;
println!("Database optimized");
```

## Integration with App

### In App Initialization

```rust
impl App {
    pub async fn new() -> anyhow::Result<Self> {
        let cache_path = Self::cache_dir()?.join("aztui.db");
        let cache = Cache::new(&cache_path)?;
        
        // Load cached subscriptions on startup
        let subscriptions = cache.repository().get_subscriptions()?;
        
        let mut state = AppState::new();
        state.subscriptions = subscriptions;
        
        Ok(Self { state, cache })
    }
}
```

### Refresh Data from Azure

```rust
impl App {
    pub async fn refresh_subscriptions(&mut self) -> anyhow::Result<()> {
        // Fetch from Azure
        let azure_client = AzureClient::new().await?;
        let subscriptions = azure_client.list_subscriptions().await?;
        
        // Update cache
        self.cache.repository().cache_subscriptions(&subscriptions)?;
        
        // Update state
        self.state.subscriptions = subscriptions;
        
        Ok(())
    }
}
```

### Load Resources for View

```rust
impl App {
    pub async fn load_resources(&mut self, subscription_id: &str) -> anyhow::Result<()> {
        // Try cache first
        let filter = ResourceFilter::new().subscription(subscription_id);
        let mut resources = self.cache.repository().get_resources(&filter)?;
        
        // If cache is empty or expired, fetch from Azure
        if resources.is_empty() {
            let azure_client = AzureClient::new().await?;
            resources = azure_client.list_resources(subscription_id).await?;
            
            // Update cache
            self.cache.repository().cache_resources(&resources)?;
        }
        
        // Update state
        self.state.resources = resources;
        
        Ok(())
    }
}
```

## Background Cache Cleanup

### Tokio Task for Periodic Cleanup

```rust
use tokio::time::{interval, Duration};

pub fn spawn_cache_cleanup_task(cache: Cache) {
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(300)); // 5 minutes
        
        loop {
            interval.tick().await;
            
            // Clear entries older than 1 hour
            match cache.clear_expired(3600) {
                Ok(deleted) if deleted > 0 => {
                    tracing::info!("Cleared {} expired cache entries", deleted);
                }
                Err(e) => {
                    tracing::error!("Failed to clear expired cache: {}", e);
                }
                _ => {}
            }
        }
    });
}
```

### Usage in Main

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = App::new().await?;
    
    // Spawn background cleanup task
    spawn_cache_cleanup_task(app.cache().clone()); // Note: Cache needs to be Clone
    
    // Run TUI
    run_tui(app).await?;
    
    Ok(())
}
```

## Error Handling

### Graceful Degradation

```rust
pub async fn get_resources_with_fallback(
    cache: &Cache,
    azure_client: &AzureClient,
    subscription_id: &str,
) -> anyhow::Result<Vec<Resource>> {
    // Try cache first
    let filter = ResourceFilter::new().subscription(subscription_id);
    
    match cache.repository().get_resources(&filter) {
        Ok(resources) if !resources.is_empty() => {
            tracing::debug!("Loaded {} resources from cache", resources.len());
            Ok(resources)
        }
        _ => {
            // Fallback to Azure API
            tracing::info!("Cache miss, fetching from Azure");
            let resources = azure_client.list_resources(subscription_id).await?;
            
            // Update cache for next time
            if let Err(e) = cache.repository().cache_resources(&resources) {
                tracing::warn!("Failed to cache resources: {}", e);
            }
            
            Ok(resources)
        }
    }
}
```

### Retry on Database Locked

```rust
use std::time::Duration;
use tokio::time::sleep;

pub async fn cache_with_retry<T, F>(
    operation: F,
    max_retries: usize,
) -> anyhow::Result<T>
where
    F: Fn() -> Result<T>,
{
    let mut retries = 0;
    
    loop {
        match operation() {
            Ok(result) => return Ok(result),
            Err(e) if retries < max_retries => {
                tracing::warn!("Cache operation failed, retrying: {}", e);
                retries += 1;
                sleep(Duration::from_millis(100 * retries as u64)).await;
            }
            Err(e) => return Err(e.into()),
        }
    }
}
```

## Performance Tips

### Batch Operations

```rust
// Good: Batch insert with transaction
repo.cache_resources(&resources)?;

// Bad: Individual inserts (slow)
for resource in resources {
    repo.cache_resource(&resource)?; // Don't do this!
}
```

### Use Filters Effectively

```rust
// Good: Filter in database
let filter = ResourceFilter::new()
    .subscription("sub-123")
    .resource_type("Microsoft.Compute/virtualMachines")
    .limit(100);
let vms = repo.get_resources(&filter)?;

// Bad: Filter in application (slow)
let all_resources = repo.get_resources(&ResourceFilter::new())?;
let vms: Vec<_> = all_resources.into_iter()
    .filter(|r| r.resource_type == "Microsoft.Compute/virtualMachines")
    .take(100)
    .collect();
```

### Pagination for Large Lists

```rust
// Load first page
let filter = ResourceFilter::new()
    .subscription("sub-123")
    .limit(100);

let first_page = repo.get_resources(&filter)?;

// For next page, use offset (future enhancement)
// Currently, limit is supported but offset is not
```

## Testing

### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    
    #[test]
    fn test_resource_caching() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = Cache::new(temp_file.path()).unwrap();
        let repo = cache.repository();
        
        // Create test resource
        let resource = Resource {
            id: "test-id".to_string(),
            name: "test-vm".to_string(),
            resource_type: "Microsoft.Compute/virtualMachines".to_string(),
            location: "eastus".to_string(),
            resource_group: "test-rg".to_string(),
            subscription_id: "test-sub".to_string(),
            tags: HashMap::new(),
        };
        
        // Cache it
        repo.cache_resource(&resource).unwrap();
        
        // Retrieve it
        let retrieved = repo.get_resource("test-id").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test-vm");
    }
}
```

## Conclusion

The cache layer provides:
- ✅ Fast local access to Azure data
- ✅ Offline browsing capability
- ✅ Flexible filtering and querying
- ✅ Automatic cache management
- ✅ Clean repository pattern API

Use these examples as a reference for integrating the cache into your application.
