# aztui Quick Start Guide

## Installation

```bash
# Clone the repository
git clone https://github.com/aztui/aztui
cd aztui

# Build the application
cargo build --release

# Install to system
cargo install --path crates/aztui
```

## First Run

### Prerequisites

1. **Azure CLI** (recommended for local use)
   ```bash
   # Install Azure CLI
   curl -sL https://aka.ms/InstallAzureCLIDeb | sudo bash
   
   # Login to Azure
   az login
   ```

2. **Verify access**
   ```bash
   # Check subscriptions
   az account list
   ```

### Launch aztui

```bash
# Start the application
aztui

# With debug logging
RUST_LOG=debug aztui
```

## What Happens on Startup

1. **Authentication**: aztui automatically authenticates using Azure CLI credentials
2. **Subscription Loading**: All accessible subscriptions are loaded immediately
3. **Caching**: Subscriptions are cached locally for offline access
4. **UI Display**: The subscription list appears within 1-2 seconds

## Keyboard Navigation

### Global Keys

| Key | Action |
|-----|--------|
| `q` or `Ctrl+C` | Quit application |
| `Ctrl+P` | Toggle command palette |
| `r` | Refresh current view |
| `Ctrl+R` | Force refresh (bypass cache) |
| `?` | Show help |
| `Esc` | Go back / Close dialogs |

### List Navigation

| Key | Action |
|-----|--------|
| `↓` or `j` | Move down |
| `↑` or `k` | Move up |
| `g` | Go to first item |
| `G` | Go to last item |
| `PageDown` | Move down 10 items |
| `PageUp` | Move up 10 items |
| `Enter` | Select item / Drill down |

### View-Specific Keys

| Key | Action | View |
|-----|--------|------|
| `/` | Start filter | All lists |
| `d` | Show details | Resource list |
| `l` | View logs | Resource detail |
| `t` | Edit tags | Resource detail |

## Basic Workflow

### 1. Browse Subscriptions

On startup, you'll see a list of all your Azure subscriptions:

```
┌─ Subscriptions ────────────────────────────────────────┐
│ > Production Subscription (Enabled)                    │
│   Development Subscription (Enabled)                   │
│   Test Subscription (Enabled)                          │
└────────────────────────────────────────────────────────┘
```

**Actions:**
- Use `↓`/`↑` or `j`/`k` to navigate
- Press `Enter` to select a subscription
- Press `r` to refresh the list

### 2. View Resources

After selecting a subscription, resources are loaded automatically:

```
┌─ Resources: Production Subscription ───────────────────┐
│ > web-app-01 (Microsoft.Web/sites)                     │
│   sql-db-prod (Microsoft.Sql/servers)                  │
│   storage-prod (Microsoft.Storage/storageAccounts)     │
│   vm-web-01 (Microsoft.Compute/virtualMachines)        │
└────────────────────────────────────────────────────────┘
```

**Actions:**
- Navigate with `↓`/`↑` or `j`/`k`
- Press `Enter` to view resource details
- Press `Esc` to go back to subscriptions
- Press `r` to refresh resources

### 3. Filter Resources

Press `/` to start filtering:

```
┌─ Resources: Production Subscription ───────────────────┐
│ Filter: web                                            │
│ > web-app-01 (Microsoft.Web/sites)                     │
│   vm-web-01 (Microsoft.Compute/virtualMachines)        │
└────────────────────────────────────────────────────────┘
```

**Actions:**
- Type to filter by name
- Press `Esc` to clear filter
- Press `Enter` to select filtered item

## Common Scenarios

### Scenario 1: Quick Resource Lookup

```bash
# 1. Start aztui
aztui

# 2. Navigate to subscription (↓/↑)
# 3. Press Enter to load resources
# 4. Press / to filter
# 5. Type resource name
# 6. Press Enter to view details
```

### Scenario 2: Multi-Subscription Browse

```bash
# 1. Start aztui
aztui

# 2. Browse first subscription
#    - Press Enter on subscription
#    - View resources
#    - Press Esc to go back

# 3. Browse second subscription
#    - Navigate to next subscription
#    - Press Enter
#    - View resources
```

### Scenario 3: Offline Mode

```bash
# 1. Run aztui while online (builds cache)
aztui

# 2. Disconnect from network

# 3. Run aztui again (uses cache)
aztui
# You'll see: "Using cached data (Azure API unavailable)"

# 4. Browse cached subscriptions and resources
```

## Troubleshooting

### "Authentication failed" Error

**Problem**: Can't connect to Azure

**Solutions**:
1. Check Azure CLI login:
   ```bash
   az account show
   ```
2. Re-login if needed:
   ```bash
   az login
   ```
3. Verify network connectivity:
   ```bash
   ping management.azure.com
   ```

### Empty Subscription List

**Problem**: No subscriptions shown after login

**Solutions**:
1. Verify subscriptions exist:
   ```bash
   az account list
   ```
2. Check permissions (need at least Reader role)
3. Try refreshing with `r` key

### Slow Startup

**Problem**: Takes more than 5 seconds to show subscriptions

**Possible causes**:
- Many subscriptions (>100)
- Slow network connection
- Azure API throttling

**Solutions**:
1. Wait for initial load (subsequent starts use cache)
2. Check network speed
3. Use offline mode after first successful load

### "Using cached data" Warning

**Problem**: Warning banner at top of screen

**Meaning**: Azure API is unavailable, showing cached data

**Solutions**:
1. Check network connectivity
2. Verify Azure CLI login: `az login`
3. Press `r` to retry refresh
4. Continue using cached data (read-only)

## Configuration

### Environment Variables

```bash
# Enable debug logging
export RUST_LOG=debug

# Set custom cache location
export AZTUI_CACHE_DIR=~/.cache/aztui

# Use service principal authentication
export AZURE_CLIENT_ID="<client-id>"
export AZURE_CLIENT_SECRET="<client-secret>"
export AZURE_TENANT_ID="<tenant-id>"
```

### Cache Location

Default cache locations:
- **Linux**: `~/.local/share/aztui/aztui.db`
- **macOS**: `~/Library/Application Support/aztui/aztui.db`
- **Windows**: `%LOCALAPPDATA%\aztui\aztui.db`

### Clear Cache

```bash
# Remove cache directory
rm -rf ~/.local/share/aztui

# Next run will rebuild cache
aztui
```

## Performance Tips

1. **First run**: Initial load may take 2-5 seconds depending on subscription count
2. **Subsequent runs**: Cache provides instant startup (<1 second)
3. **Refresh**: Press `r` to update data (respects Azure API rate limits)
4. **Offline mode**: Full functionality with cached data

## Advanced Usage

### Service Principal Authentication

For automation or CI/CD:

```bash
# Set environment variables
export AZURE_CLIENT_ID="<client-id>"
export AZURE_CLIENT_SECRET="<client-secret>"
export AZURE_TENANT_ID="<tenant-id>"

# Run aztui
aztui
```

### Managed Identity

On Azure VMs with managed identity:

```bash
# No configuration needed
aztui
# Automatically uses managed identity
```

### Multiple Tenants

```bash
# Login to specific tenant
az login --tenant <tenant-id>

# Run aztui
aztui
```

## Next Steps

- Read [Authentication Guide](./authentication.md) for detailed auth setup
- See [Azure Integration](./azure-integration.md) for technical details
- Check [Keyboard Reference](./keyboard-reference.md) for all shortcuts
- Review [Architecture](./architecture.md) for system design

## Getting Help

- Press `?` in the application for context-sensitive help
- Check logs with `RUST_LOG=debug aztui`
- Report issues at https://github.com/aztui/aztui/issues

## Tips & Tricks

1. **Fast navigation**: Use `g` and `G` to jump to first/last item
2. **Quick refresh**: `Ctrl+R` forces a fresh load from Azure
3. **Filter efficiently**: Press `/` and type partial names
4. **Keyboard-first**: All actions available via keyboard (no mouse needed)
5. **Offline work**: Cache enables full browsing without network

## Comparison with Azure CLI

| Task | Azure CLI | aztui |
|------|-----------|-------|
| List subscriptions | `az account list` | Launch app (instant) |
| List resources | `az resource list` | Navigate + Enter |
| Filter resources | `az resource list \| grep` | Press `/` + type |
| View details | `az resource show --ids` | Navigate + Enter |
| Multiple subscriptions | Multiple commands | Navigate between subs |
| Offline access | Not available | Full cache support |

## Comparison with Azure Portal

| Feature | Azure Portal | aztui |
|---------|--------------|-------|
| Speed | 5-10 seconds load | 1-2 seconds |
| Keyboard navigation | Limited | Full support |
| Offline mode | Not available | Full cache |
| Resource browsing | Click-heavy | Keyboard-first |
| Multi-subscription | Tab switching | Instant navigation |
| Terminal-friendly | No | Yes |

---

**Ready to start?** Run `aztui` and explore your Azure resources!
