# Azure SDK Integration Technical Guide

## Overview

This document provides technical details about how aztui integrates with the Azure SDK for Rust (version 0.19), including API patterns, data structures, and implementation notes.

## Azure SDK Architecture

### Crate Structure

```
aztui-azure/
├── src/
│   ├── lib.rs          # Public API and error types
│   ├── auth.rs         # Authentication providers
│   ├── client.rs       # Azure API client
│   └── models.rs       # Azure API response models
└── Cargo.toml
```

### Dependencies

```toml
[dependencies]
# Azure SDK core
azure_core = "0.19"           # Core types and traits
azure_identity = "0.19"       # Authentication
azure_mgmt_resources = "0.19" # Resource management
azure_mgmt_compute = "0.19"   # Compute resources (future)
azure_mgmt_network = "0.19"   # Network resources (future)

# Async runtime
tokio = { version = "1.35", features = ["full"] }
futures = "0.3"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

## Authentication Implementation

### AuthProvider Trait

```rust
pub trait AuthProvider: Send + Sync {
    fn credential(&self) -> Arc<dyn TokenCredential>;
}
```

This trait abstracts credential management, allowing different authentication strategies.

### AzureAuth Implementation

```rust
pub struct AzureAuth {
    credential: Arc<dyn TokenCredential>,
}

impl AzureAuth {
    // Azure CLI authentication
    pub fn new_cli() -> Result<Self> {
        let credential = Arc::new(AzureCliCredential::new());
        Ok(Self { credential })
    }

    // Default credential chain
    pub fn new_default() -> Result<Self> {
        let credential = Arc::new(DefaultAzureCredential::default());
        Ok(Self { credential })
    }

    // Auto-detection with fallback
    pub async fn new_auto() -> Result<Self> {
        match Self::new_cli() {
            Ok(auth) => Ok(auth),
            Err(_) => Self::new_default(),
        }
    }
}
```

### Credential Types

| Type | Use Case | Pros | Cons |
|------|----------|------|------|
| `AzureCliCredential` | Local development | Simple, no config | Requires Azure CLI |
| `DefaultAzureCredential` | Production | Flexible, multiple sources | Complex fallback chain |
| `EnvironmentCredential` | CI/CD | Explicit, secure | Requires env vars |
| `ManagedIdentityCredential` | Azure VMs | No secrets | Azure-only |

## Azure Client Implementation

### Client Structure

```rust
pub struct AzureClient {
    auth: Arc<dyn AuthProvider>,
}

impl AzureClient {
    pub fn new(auth: Arc<dyn AuthProvider>) -> Self {
        Self { auth }
    }
}
```

### Resource Groups API

```rust
pub async fn list_resource_groups(
    &self,
    subscription_id: &str,
) -> Result<Vec<AzureResourceGroup>> {
    let credential = self.auth.credential();
    
    // Create resources client
    let client = ResourcesClient::builder(credential.clone())
        .build()
        .map_err(|e| AzureError::ApiError(format!("Failed to create client: {}", e)))?;
    
    // List resource groups with pagination
    let mut resource_groups = Vec::new();
    let mut stream = client
        .resource_groups_client()
        .list(subscription_id)
        .into_stream();
    
    use futures::StreamExt;
    while let Some(result) = stream.next().await {
        match result {
            Ok(page) => {
                for rg in page.value {
                    // Convert to domain model
                    let azure_rg = AzureResourceGroup {
                        id: rg.id.clone().unwrap_or_default(),
                        name: rg.name.clone().unwrap_or_else(|| "Unnamed".to_string()),
                        location: rg.location.clone(),
                        subscription_id: subscription_id.to_string(),
                        tags: parse_tags(&rg.tags),
                    };
                    resource_groups.push(azure_rg);
                }
            }
            Err(e) => {
                return Err(AzureError::ApiError(format!("Failed to list: {}", e)));
            }
        }
    }
    
    Ok(resource_groups)
}
```

### Resources API

```rust
pub async fn list_resources(
    &self,
    subscription_id: &str,
) -> Result<Vec<AzureResource>> {
    let credential = self.auth.credential();
    
    let client = ResourcesClient::builder(credential.clone())
        .build()
        .map_err(|e| AzureError::ApiError(format!("Failed to create client: {}", e)))?;
    
    let mut resources = Vec::new();
    let mut stream = client
        .resources_client()
        .list(subscription_id)
        .into_stream();
    
    use futures::StreamExt;
    while let Some(result) = stream.next().await {
        match result {
            Ok(page) => {
                for resource in page.value {
                    // Azure SDK 0.19 uses nested structure
                    let res = &resource.generic_resource.resource;
                    
                    let azure_resource = AzureResource {
                        id: res.id.clone().unwrap_or_default(),
                        name: res.name.clone().unwrap_or_else(|| "Unnamed".to_string()),
                        resource_type: res.type_.clone().unwrap_or_else(|| "Unknown".to_string()),
                        location: res.location.clone().unwrap_or_else(|| "Unknown".to_string()),
                        resource_group: extract_resource_group(&res.id),
                        subscription_id: subscription_id.to_string(),
                        tags: parse_tags(&res.tags),
                    };
                    resources.push(azure_resource);
                }
            }
            Err(e) => {
                return Err(AzureError::ApiError(format!("Failed to list: {}", e)));
            }
        }
    }
    
    Ok(resources)
}
```

## Data Model Mapping

### Azure SDK Types → Domain Models

| Azure SDK Type | Domain Model | Notes |
|----------------|--------------|-------|
| `Subscription` | `AzureSubscription` | Requires separate crate |
| `ResourceGroup` | `AzureResourceGroup` | Direct mapping |
| `GenericResourceExpanded` | `AzureResource` | Nested structure |
| `Value` (tags) | `HashMap<String, String>` | JSON parsing required |

### Tag Parsing

Azure SDK returns tags as `serde_json::Value`. We convert to `HashMap`:

```rust
fn parse_tags(tags_value: &Option<Value>) -> HashMap<String, String> {
    if let Some(tags_value) = tags_value {
        if let Some(obj) = tags_value.as_object() {
            return obj.iter()
                .filter_map(|(k, v)| {
                    v.as_str().map(|s| (k.clone(), s.to_string()))
                })
                .collect();
        }
    }
    HashMap::new()
}
```

### Resource ID Parsing

Azure resource IDs follow a standard format:

```
/subscriptions/{subscription-id}/resourceGroups/{rg-name}/providers/{provider}/{type}/{name}
```

We extract the resource group name:

```rust
fn extract_resource_group(resource_id: &str) -> Option<String> {
    resource_id
        .split('/')
        .enumerate()
        .find(|(i, part)| *i > 0 && part.eq_ignore_ascii_case("resourceGroups"))
        .and_then(|(i, _)| resource_id.split('/').nth(i + 1))
        .map(|s| s.to_string())
}
```

## Pagination Handling

Azure APIs return paginated results. The SDK provides streaming:

```rust
use futures::StreamExt;

let mut stream = client
    .resource_groups_client()
    .list(subscription_id)
    .into_stream();

while let Some(result) = stream.next().await {
    match result {
        Ok(page) => {
            // Process page.value (Vec<ResourceGroup>)
        }
        Err(e) => {
            // Handle error
        }
    }
}
```

### Pagination Best Practices

1. **Stream processing** - don't load all pages into memory
2. **Error handling** - handle per-page errors gracefully
3. **Progress feedback** - show progress for large result sets
4. **Cancellation** - support user cancellation of long operations

## Error Handling

### Error Types

```rust
#[derive(Error, Debug)]
pub enum AzureError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    
    #[error("API request failed: {0}")]
    ApiError(String),
    
    #[error("Resource not found: {0}")]
    NotFound(String),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}
```

### Error Conversion

```rust
// Convert Azure SDK errors to our error type
.map_err(|e| AzureError::ApiError(format!("Failed to create client: {}", e)))?
```

### Retry Strategy

For transient errors, implement exponential backoff:

```rust
use tokio::time::{sleep, Duration};

async fn retry_with_backoff<F, T>(mut f: F, max_retries: u32) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let mut retries = 0;
    loop {
        match f() {
            Ok(result) => return Ok(result),
            Err(e) if retries < max_retries => {
                let delay = Duration::from_secs(2u64.pow(retries));
                sleep(delay).await;
                retries += 1;
            }
            Err(e) => return Err(e),
        }
    }
}
```

## Performance Considerations

### Caching Strategy

1. **Cache subscriptions** - rarely change, cache for 1 hour
2. **Cache resource groups** - moderate change rate, cache for 15 minutes
3. **Cache resources** - frequent changes, cache for 5 minutes
4. **Invalidate on error** - clear cache on 404 errors

### Parallel Requests

For multiple subscriptions, fetch in parallel:

```rust
use futures::future::join_all;

let futures: Vec<_> = subscription_ids
    .iter()
    .map(|id| client.list_resource_groups(id))
    .collect();

let results = join_all(futures).await;
```

### Rate Limiting

Implement client-side rate limiting:

```rust
use tokio::sync::Semaphore;

let semaphore = Arc::new(Semaphore::new(10)); // Max 10 concurrent requests

let permit = semaphore.acquire().await?;
let result = client.list_resources(subscription_id).await?;
drop(permit);
```

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_resource_group() {
        let id = "/subscriptions/sub-id/resourceGroups/my-rg/providers/Microsoft.Compute/virtualMachines/my-vm";
        assert_eq!(extract_resource_group(id), Some("my-rg".to_string()));
    }
}
```

### Integration Tests

```rust
#[tokio::test]
#[ignore] // Requires Azure credentials
async fn test_list_resource_groups() {
    let auth = AzureAuth::new_cli().unwrap();
    let client = AzureClient::new(Arc::new(auth));
    
    let subscription_id = std::env::var("AZURE_SUBSCRIPTION_ID").unwrap();
    let result = client.list_resource_groups(&subscription_id).await;
    
    assert!(result.is_ok());
}
```

### Mocking

For unit tests without Azure credentials:

```rust
struct MockAuthProvider {
    credential: Arc<dyn TokenCredential>,
}

impl AuthProvider for MockAuthProvider {
    fn credential(&self) -> Arc<dyn TokenCredential> {
        self.credential.clone()
    }
}
```

## Known Limitations

### Azure SDK 0.19 Limitations

1. **No subscription client** - requires separate crate or REST API
2. **Nested resource structure** - `GenericResourceExpanded` has nested fields
3. **Tag parsing** - tags are `serde_json::Value`, not `HashMap`
4. **Limited documentation** - SDK is still evolving

### Workarounds

1. **Subscription listing**: Use Azure CLI or REST API directly
2. **Nested fields**: Access via `resource.generic_resource.resource`
3. **Tag parsing**: Implement custom parser for `Value` → `HashMap`

## Future Enhancements

### Planned Features

1. **Subscription management** - add `azure_mgmt_subscription` dependency
2. **Resource filtering** - filter by type, location, tags
3. **Resource details** - fetch detailed properties for specific resources
4. **Resource actions** - start/stop VMs, scale resources
5. **Metrics and logs** - integrate with Azure Monitor

### API Additions

```rust
// Future API methods
impl AzureClient {
    pub async fn get_resource_details(&self, resource_id: &str) -> Result<ResourceDetail>;
    pub async fn list_virtual_machines(&self, subscription_id: &str) -> Result<Vec<VirtualMachine>>;
    pub async fn get_resource_metrics(&self, resource_id: &str) -> Result<Vec<Metric>>;
    pub async fn get_resource_logs(&self, resource_id: &str) -> Result<Vec<LogEntry>>;
}
```

## References

- [Azure SDK for Rust GitHub](https://github.com/Azure/azure-sdk-for-rust)
- [Azure SDK for Rust Documentation](https://docs.rs/azure_mgmt_resources/)
- [Azure Resource Manager REST API](https://docs.microsoft.com/en-us/rest/api/resources/)
- [Azure SDK Design Guidelines](https://azure.github.io/azure-sdk/rust_introduction.html)

## Contributing

When adding new Azure API integrations:

1. Follow the existing pattern in `client.rs`
2. Add proper error handling and logging
3. Implement pagination for list operations
4. Add unit tests for parsing logic
5. Document any SDK quirks or workarounds
6. Update this guide with new patterns
