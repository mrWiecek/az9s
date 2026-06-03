# Azure Authentication Guide for aztui

## Overview

aztui uses the Azure SDK for Rust (version 0.19) to authenticate and interact with Azure resources. This document explains the authentication flows, credential management, and best practices for local development and production use.

## Authentication Methods

### 1. Azure CLI Authentication (Recommended for Local Development)

The **Azure CLI** method is the simplest and most common approach for operators working locally.

#### Prerequisites
```bash
# Install Azure CLI
# macOS: brew install azure-cli
# Linux: curl -sL https://aka.ms/InstallAzureCLIDeb | sudo bash
# Windows: Download from https://aka.ms/installazurecliwindows

# Login to Azure
az login

# Verify authentication
az account show
```

#### How it Works
- aztui uses `AzureCliCredential` from the `azure_identity` crate
- Reads cached credentials from `~/.azure/` directory
- Automatically refreshes tokens when needed
- No additional configuration required

#### Code Example
```rust
use aztui_azure::AzureAuth;

// Create authentication using Azure CLI
let auth = AzureAuth::new_cli()?;
```

### 2. Default Credential Chain (Production)

The **Default Credential Chain** tries multiple authentication methods in order:

1. **Environment variables** (`AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`)
2. **Managed Identity** (when running on Azure VMs, App Service, etc.)
3. **Azure CLI** (fallback for local development)

#### Code Example
```rust
use aztui_azure::AzureAuth;

// Create authentication using default chain
let auth = AzureAuth::new_default()?;
```

### 3. Auto-Detection (Recommended for aztui)

The **auto-detection** method tries Azure CLI first, then falls back to the default chain:

```rust
use aztui_azure::AzureAuth;

// Automatically detect best authentication method
let auth = AzureAuth::new_auto().await?;
```

This is the method used by aztui's main application.

## Subscription Management

### Current Implementation Status

⚠️ **Note**: The Azure SDK 0.19 for Rust does not include a subscription management client in the `azure_mgmt_resources` crate. Subscription listing requires a separate crate.

#### Workaround Options

1. **Use Azure CLI directly** (current approach):
   ```bash
   az account list --output json
   ```

2. **Add `azure_mgmt_subscription` crate** (future enhancement):
   ```toml
   [dependencies]
   azure_mgmt_subscription = "0.19"
   ```

3. **Use REST API directly**:
   ```rust
   // Call https://management.azure.com/subscriptions?api-version=2020-01-01
   ```

### Listing Subscriptions

```rust
use aztui_azure::AzureClient;

let client = AzureClient::new(auth);

// Currently returns empty list - requires implementation
let subscriptions = client.list_subscriptions().await?;
```

## Resource Group and Resource Listing

### List Resource Groups

```rust
use aztui_azure::AzureClient;

let client = AzureClient::new(auth);
let subscription_id = "your-subscription-id";

// Fetch all resource groups in a subscription
let resource_groups = client.list_resource_groups(subscription_id).await?;

for rg in resource_groups {
    println!("Resource Group: {} in {}", rg.name, rg.location);
}
```

### List Resources

```rust
use aztui_azure::AzureClient;

let client = AzureClient::new(auth);
let subscription_id = "your-subscription-id";

// Fetch all resources in a subscription
let resources = client.list_resources(subscription_id).await?;

for resource in resources {
    println!("Resource: {} ({})", resource.name, resource.resource_type);
}
```

## Error Handling

### Authentication Errors

```rust
use aztui_azure::{AzureAuth, AzureError};

match AzureAuth::new_cli() {
    Ok(auth) => {
        // Authentication successful
    }
    Err(AzureError::AuthenticationFailed(msg)) => {
        eprintln!("Authentication failed: {}", msg);
        eprintln!("Try running: az login");
    }
    Err(e) => {
        eprintln!("Unexpected error: {}", e);
    }
}
```

### API Errors

```rust
use aztui_azure::{AzureClient, AzureError};

match client.list_resource_groups(subscription_id).await {
    Ok(groups) => {
        // Process resource groups
    }
    Err(AzureError::ApiError(msg)) => {
        eprintln!("API call failed: {}", msg);
    }
    Err(AzureError::NotFound(msg)) => {
        eprintln!("Resource not found: {}", msg);
    }
    Err(e) => {
        eprintln!("Unexpected error: {}", e);
    }
}
```

## Credential Caching and Refresh

### Token Lifecycle

- Azure CLI tokens are cached in `~/.azure/`
- Tokens typically expire after 1 hour
- The Azure SDK automatically refreshes tokens
- No manual token management required

### Offline Mode

aztui includes SQLite caching to support offline browsing:

```rust
use aztui_core::Cache;

let cache = Cache::new("~/.aztui/cache.db")?;

// Cache subscriptions
cache.cache_subscriptions(&subscriptions)?;

// Retrieve from cache when offline
let cached_subs = cache.get_subscriptions()?;
```

## Multi-Tenant Scenarios

### Switching Tenants

```bash
# List available tenants
az account tenant list

# Login to specific tenant
az login --tenant <tenant-id>

# Verify current tenant
az account show --query tenantId
```

### Handling Multiple Subscriptions

```bash
# List all subscriptions
az account list --output table

# Set default subscription
az account set --subscription <subscription-id>

# Show current subscription
az account show
```

## Security Best Practices

### Local Development

1. **Use Azure CLI authentication** - simplest and most secure
2. **Never commit credentials** - add `.azure/` to `.gitignore`
3. **Use least-privilege accounts** - only grant necessary permissions
4. **Regularly rotate credentials** - run `az login` periodically

### Production Deployment

1. **Use Managed Identity** when running on Azure
2. **Use Service Principal** with certificate authentication
3. **Store secrets in Azure Key Vault**
4. **Enable audit logging** for all API calls
5. **Implement credential rotation** policies

### Environment Variables (Production)

```bash
# Service Principal authentication
export AZURE_CLIENT_ID="your-client-id"
export AZURE_CLIENT_SECRET="your-client-secret"
export AZURE_TENANT_ID="your-tenant-id"

# Run aztui
./aztui
```

## Troubleshooting

### "Authentication failed" Error

```bash
# Check if logged in
az account show

# If not logged in
az login

# Clear cached credentials
rm -rf ~/.azure/
az login
```

### "Subscription not found" Error

```bash
# List available subscriptions
az account list --output table

# Set correct subscription
az account set --subscription <subscription-id>
```

### "Insufficient permissions" Error

```bash
# Check current role assignments
az role assignment list --assignee $(az account show --query user.name -o tsv)

# Required roles:
# - Reader (minimum for listing resources)
# - Contributor (for resource management)
```

### Network/Proxy Issues

```bash
# Configure proxy for Azure CLI
export HTTP_PROXY=http://proxy.example.com:8080
export HTTPS_PROXY=http://proxy.example.com:8080

# Test connectivity
az account show
```

## API Rate Limits

Azure Resource Manager has rate limits:

- **12,000 reads per hour** per subscription
- **1,200 writes per hour** per subscription

aztui mitigates this by:
1. **Caching responses** in SQLite
2. **Batching requests** where possible
3. **Implementing exponential backoff** on rate limit errors

## Future Enhancements

### Planned Features

1. **Device Code Flow** - for headless/SSH scenarios
2. **Certificate-based authentication** - for production
3. **Subscription filtering** - by tenant, name, or tags
4. **Credential refresh UI** - visual feedback for token expiry
5. **Multi-account support** - switch between Azure accounts

### Implementation Roadmap

- **Iteration 1**: Basic Azure CLI authentication ✅
- **Iteration 2**: Resource group and resource listing ✅
- **Iteration 3**: Subscription management (requires additional crate)
- **Iteration 4**: Device code flow for SSH scenarios
- **Iteration 5**: Certificate-based authentication

## References

- [Azure SDK for Rust](https://github.com/Azure/azure-sdk-for-rust)
- [Azure Identity Documentation](https://docs.microsoft.com/en-us/azure/developer/rust/authentication)
- [Azure CLI Documentation](https://docs.microsoft.com/en-us/cli/azure/)
- [Azure Resource Manager API](https://docs.microsoft.com/en-us/rest/api/resources/)

## Support

For issues or questions:
- Check the [troubleshooting section](#troubleshooting)
- Review Azure SDK logs: `RUST_LOG=azure_core=debug cargo run`
- File an issue on the aztui GitHub repository
