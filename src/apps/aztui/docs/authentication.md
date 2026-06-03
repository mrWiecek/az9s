# Azure Authentication Guide

## Overview

`aztui` uses Azure SDK for Rust to authenticate with Azure services. The authentication system is designed to be flexible, supporting multiple credential types with automatic fallback.

## Authentication Flow

### Startup Authentication

When `aztui` starts, it automatically attempts to authenticate using the following credential chain:

1. **Azure CLI Credentials** (Primary)
   - Uses `az login` session
   - Most common for operators and developers
   - No additional configuration required if already logged in

2. **Default Azure Credential** (Fallback)
   - Managed Identity (for Azure VMs/containers)
   - Environment variables
   - Azure CLI (redundant check)
   - Interactive browser login (if available)

### Subscription Loading

After successful authentication, `aztui` immediately loads subscriptions:

1. Fetches all subscriptions from Azure API
2. Caches subscriptions locally in SQLite
3. Populates the UI with subscription list
4. If API call fails, attempts to load from cache

This ensures the UI shows subscriptions immediately on launch rather than an empty screen.

## Authentication Methods

### Azure CLI (Recommended for Local Development)

The simplest method for local operators:

```bash
# Login to Azure
az login

# Verify login
az account show

# Start aztui
aztui
```

**Advantages:**
- No configuration needed
- Works with MFA
- Supports multiple tenants
- Familiar to Azure operators

**Requirements:**
- Azure CLI installed (`az` command available)
- Active login session (`az login` completed)

### Environment Variables

For automation and CI/CD scenarios:

```bash
# Service Principal authentication
export AZURE_CLIENT_ID="<client-id>"
export AZURE_CLIENT_SECRET="<client-secret>"
export AZURE_TENANT_ID="<tenant-id>"

# Start aztui
aztui
```

**Advantages:**
- Non-interactive
- Suitable for automation
- Works in containers

**Requirements:**
- Service Principal with appropriate permissions
- Environment variables set before launch

### Managed Identity

For Azure VMs and container instances:

```bash
# No configuration needed - automatically detected
aztui
```

**Advantages:**
- No credentials to manage
- Automatic rotation
- Secure by default

**Requirements:**
- Running on Azure VM or container with managed identity enabled
- Identity has appropriate RBAC permissions

## Credential Fallback Strategy

The authentication system implements a graceful fallback strategy:

```
Azure CLI → Default Credential Chain → Error
     ↓              ↓                    ↓
  Success        Success            Show Error
     ↓              ↓                    ↓
Load Subs      Load Subs          Load from Cache
     ↓              ↓                    ↓
  Cache          Cache            Show Warning
```

### Offline Mode

If authentication fails but cached data exists:

1. `aztui` loads subscriptions from local cache
2. Displays a warning banner: "Using cached data (Azure API unavailable)"
3. Allows browsing cached resources
4. Refresh operations will fail with error messages

## Required Permissions

### Minimum RBAC Roles

For read-only browsing (recommended):
- **Reader** role at subscription level

For full functionality:
- **Reader** role at subscription level
- **Log Analytics Reader** (for log viewing)

### Service Principal Setup

Create a service principal with appropriate permissions:

```bash
# Create service principal
az ad sp create-for-rbac \
  --name "aztui-reader" \
  --role "Reader" \
  --scopes /subscriptions/<subscription-id>

# Output will include:
# - appId (AZURE_CLIENT_ID)
# - password (AZURE_CLIENT_SECRET)
# - tenant (AZURE_TENANT_ID)
```

## Troubleshooting

### "Authentication failed" on startup

**Symptoms:**
- Error message on launch
- Empty subscription list
- No cached data available

**Solutions:**

1. **Check Azure CLI login:**
   ```bash
   az account show
   # If not logged in:
   az login
   ```

2. **Verify environment variables:**
   ```bash
   echo $AZURE_CLIENT_ID
   echo $AZURE_TENANT_ID
   # Should not be empty if using service principal
   ```

3. **Check network connectivity:**
   ```bash
   curl -I https://management.azure.com
   # Should return 401 (authentication required) not timeout
   ```

4. **Enable debug logging:**
   ```bash
   RUST_LOG=debug aztui
   # Check logs for detailed error messages
   ```

### "Using cached data" warning

**Symptoms:**
- Warning banner at top of screen
- Subscriptions visible but may be stale
- Refresh operations fail

**Causes:**
- Network connectivity issues
- Azure API outage
- Expired credentials
- Insufficient permissions

**Solutions:**

1. **Refresh credentials:**
   ```bash
   az login --force
   ```

2. **Check Azure service health:**
   - Visit https://status.azure.com

3. **Verify permissions:**
   ```bash
   az role assignment list --assignee <your-user-or-sp>
   ```

### Subscription list is empty

**Symptoms:**
- No subscriptions shown after successful login
- No error message

**Possible causes:**

1. **No subscriptions accessible:**
   ```bash
   az account list
   # Should show at least one subscription
   ```

2. **Subscription filter:**
   - Check if Azure CLI has subscription filters set
   ```bash
   az account list --all
   ```

3. **Permission issues:**
   - User/SP may not have access to any subscriptions
   - Check RBAC assignments in Azure Portal

### Multi-tenant scenarios

**Issue:** Need to access resources across multiple tenants

**Solution:**

1. **Login to specific tenant:**
   ```bash
   az login --tenant <tenant-id>
   ```

2. **Switch between tenants:**
   ```bash
   az account set --subscription <subscription-in-other-tenant>
   ```

3. **List all accessible tenants:**
   ```bash
   az account tenant list
   ```

## Security Best Practices

### Local Development

1. **Use Azure CLI authentication**
   - Leverages existing login session
   - Supports MFA
   - No credentials stored in environment

2. **Regular credential rotation**
   ```bash
   # Re-login periodically
   az login
   ```

3. **Use least-privilege access**
   - Request only Reader role
   - Avoid Owner/Contributor for browsing

### Production/Automation

1. **Use Managed Identity when possible**
   - No credential management
   - Automatic rotation
   - Audit trail in Azure AD

2. **Service Principal best practices**
   - Rotate secrets regularly (90 days)
   - Use certificate-based auth when possible
   - Store secrets in Azure Key Vault
   - Never commit credentials to source control

3. **Environment variable security**
   ```bash
   # Use secret management
   export AZURE_CLIENT_SECRET=$(az keyvault secret show \
     --vault-name <vault> \
     --name aztui-sp-secret \
     --query value -o tsv)
   ```

### Cache Security

The local SQLite cache contains:
- Subscription names and IDs
- Resource metadata
- Resource group information
- Tags (may contain sensitive data)

**Cache location:**
- Linux: `~/.local/share/aztui/aztui.db`
- macOS: `~/Library/Application Support/aztui/aztui.db`
- Windows: `%LOCALAPPDATA%\aztui\aztui.db`

**Recommendations:**
1. Ensure cache directory has appropriate permissions (0700)
2. Clear cache when offboarding: `rm -rf ~/.local/share/aztui`
3. Exclude cache from backups if it contains sensitive tags

## Advanced Configuration

### Custom Credential Provider

For advanced scenarios, you can extend the authentication system:

```rust
use aztui_azure::{AuthProvider, AzureClient};
use azure_core::auth::TokenCredential;
use std::sync::Arc;

struct CustomAuth {
    credential: Arc<dyn TokenCredential>,
}

impl AuthProvider for CustomAuth {
    fn credential(&self) -> Arc<dyn TokenCredential> {
        self.credential.clone()
    }
}

// Use custom auth
let auth = CustomAuth { /* ... */ };
let client = AzureClient::new(Arc::new(auth));
```

### Credential Caching

Azure SDK automatically caches tokens in memory. Token refresh is handled transparently.

**Token lifetime:**
- Default: 1 hour
- Automatic refresh: 5 minutes before expiry
- No manual intervention required

## API Reference

### AzureAuth

```rust
// Create with Azure CLI credentials
let auth = AzureAuth::new_cli()?;

// Create with default credential chain
let auth = AzureAuth::new_default()?;

// Automatic fallback (recommended)
let auth = AzureAuth::new_auto().await?;
```

### AzureClient

```rust
let client = AzureClient::new(Arc::new(auth));

// List subscriptions
let subscriptions = client.list_subscriptions().await?;

// List resource groups
let rgs = client.list_resource_groups(subscription_id).await?;

// List resources
let resources = client.list_resources(subscription_id).await?;
```

## Related Documentation

- [Azure SDK for Rust](https://github.com/Azure/azure-sdk-for-rust)
- [Azure CLI Authentication](https://docs.microsoft.com/en-us/cli/azure/authenticate-azure-cli)
- [Azure Managed Identities](https://docs.microsoft.com/en-us/azure/active-directory/managed-identities-azure-resources/)
- [Service Principal Best Practices](https://docs.microsoft.com/en-us/azure/active-directory/develop/howto-create-service-principal-portal)
