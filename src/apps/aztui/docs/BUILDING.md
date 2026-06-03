# aztui Quick Start Guide

Get up and running with aztui in under 5 minutes.

## Prerequisites

- Rust 1.75 or later ([install](https://rustup.rs/))
- Azure CLI ([install](https://docs.microsoft.com/en-us/cli/azure/install-azure-cli)) - optional but recommended
- An Azure subscription

## Step 1: Fix Module Conflict ⚠️

**This step is required before building!**

The workspace has a module ambiguity that must be resolved first.

### Linux/macOS

```bash
cd /home/matx/projects/aztui/workspaces/aztui
chmod +x cleanup.sh
./cleanup.sh
```

### Windows

```cmd
cd C:\path\to\aztui\workspaces\aztui
cleanup.bat
```

You should see:
```
✅ Cleanup complete!
✅ All checks passed! The workspace is now ready for development.
```

## Step 2: Authenticate with Azure

### Option A: Azure CLI (Recommended)

```bash
az login
az account set --subscription "Your Subscription Name"
```

### Option B: Default Credential Chain

Set environment variables:

```bash
# Service Principal
export AZURE_CLIENT_ID="your-client-id"
export AZURE_CLIENT_SECRET="your-client-secret"
export AZURE_TENANT_ID="your-tenant-id"

# Or Managed Identity (when running in Azure)
# No environment variables needed
```

## Step 3: Build aztui

```bash
cargo build --release
```

This will:
- Download and compile dependencies (~5 minutes first time)
- Build all 4 crates (aztui, aztui-core, aztui-azure, aztui-tui)
- Create the binary at `target/release/aztui`

## Step 4: Run aztui

```bash
# Development mode (with debug logging)
RUST_LOG=debug cargo run

# Or run the release binary
./target/release/aztui
```

## Step 5: Navigate the UI

Once aztui starts, you'll see the subscription list:

```
┌─ aztui ─────────────────────────────────────────────────────────┐
│ Subscriptions (3)                                               │
│                                                                 │
│ > My Production Subscription                                    │
│   My Development Subscription                                   │
│   My Test Subscription                                          │
│                                                                 │
│                                                                 │
│ j/k: navigate | Enter: select | q: quit                        │
└─────────────────────────────────────────────────────────────────┘
```

### Basic Navigation

- `j` or `↓` - Move down
- `k` or `↑` - Move up
- `g` - Jump to top
- `G` - Jump to bottom
- `Enter` - Select subscription (view resource groups)
- `Esc` - Go back
- `q` - Quit

## Troubleshooting

### "Module ambiguity error"

You skipped Step 1. Run the cleanup script:

```bash
./cleanup.sh  # Linux/macOS
cleanup.bat   # Windows
```

### "Authentication failed"

Make sure you're logged in with Azure CLI:

```bash
az login
az account show  # Verify you're logged in
```

### "No subscriptions found"

Check your Azure CLI authentication:

```bash
az account list --output table
```

If you see subscriptions in the CLI but not in aztui, try:

```bash
# Clear the cache
rm -rf ~/.aztui/cache.db

# Run with debug logging
RUST_LOG=debug cargo run
```

### "Compilation errors"

Make sure you're using Rust 1.75 or later:

```bash
rustc --version
rustup update
```

## Next Steps

- Read [KEYBOARD_SHORTCUTS.md](docs/KEYBOARD_SHORTCUTS.md) for all keybindings
- Check [ARCHITECTURE.md](ARCHITECTURE.md) to understand the codebase
- See [PROJECT_STATUS.md](PROJECT_STATUS.md) for current features and roadmap
- Review [docs/AZURE_AUTHENTICATION.md](docs/AZURE_AUTHENTICATION.md) for authentication details

## Development

### Run Tests

```bash
cargo test --workspace
```

### Run Clippy

```bash
cargo clippy --workspace
```

### Check All Crates

```bash
cargo check --workspace
```

### Enable Debug Logging

```bash
RUST_LOG=debug cargo run
RUST_LOG=trace cargo run  # Very verbose
```

### Cache Location

aztui stores its cache at:
- Linux/macOS: `~/.aztui/cache.db`
- Windows: `%USERPROFILE%\.aztui\cache.db`

To clear the cache:

```bash
rm -rf ~/.aztui/cache.db  # Linux/macOS
del %USERPROFILE%\.aztui\cache.db  # Windows
```

## Getting Help

- Check [URGENT_ACTION_REQUIRED.md](URGENT_ACTION_REQUIRED.md) for known issues
- Review [docs/](docs/) for detailed documentation
- Open an issue on GitHub (when available)

## What's Working (Iteration 3)

✅ Azure authentication (CLI + default credential)  
✅ Subscription listing (placeholder data)  
✅ List navigation with vim-style keys  
✅ Visual selection highlighting  
✅ Error overlay display  
✅ Loading indicators  
✅ SQLite cache infrastructure  

## What's Coming (Iteration 4)

🚧 Real Azure data fetching  
🚧 Resource group navigation  
🚧 Resource listing  
🚧 Breadcrumb navigation  
🚧 View transitions  

See [ITERATION_PLAN.md](ITERATION_PLAN.md) for the full roadmap.
