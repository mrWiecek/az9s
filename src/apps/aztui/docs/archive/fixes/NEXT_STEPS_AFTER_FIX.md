# Next Steps After Module Ambiguity Fix

**Prerequisites**: `crates/aztui-core/src/cache.rs` has been deleted and `cargo check` succeeds.

---

## Immediate Validation (5 minutes)

### 1. Run Full Test Suite
```bash
cargo test --all
```

**Expected**: All 28 tests pass
- aztui-core: 20 tests
- aztui-tui: 8 tests

### 2. Run Linter
```bash
cargo clippy --all-targets --all-features
```

**Expected**: No errors, possibly some warnings to address

### 3. Build Release Binary
```bash
cargo build --release
```

**Expected**: Successful build, binary at `target/release/aztui`

---

## Priority 1: Complete Azure Integration (1-2 hours)

### Add Subscription Management

**Issue**: `list_subscriptions()` currently returns empty list (see warning in `client.rs:36`)

**Solution**:

1. Add dependency to `Cargo.toml`:
```toml
[workspace.dependencies]
azure_mgmt_subscription = "0.19"
```

2. Update `crates/aztui-azure/Cargo.toml`:
```toml
[dependencies]
azure_mgmt_subscription = { workspace = true }
```

3. Implement in `crates/aztui-azure/src/client.rs`:
```rust
use azure_mgmt_subscription::Client as SubscriptionClient;

pub async fn list_subscriptions(&self) -> Result<Vec<AzureSubscription>> {
    info!("Fetching subscriptions from Azure API");
    
    let credential = self.auth.credential();
    let client = SubscriptionClient::builder(credential.clone())
        .build()
        .map_err(|e| AzureError::ApiError(format!("Failed to create subscription client: {}", e)))?;
    
    let mut subscriptions = Vec::new();
    let mut stream = client.subscriptions_client().list().into_stream();
    
    use futures::StreamExt;
    while let Some(result) = stream.next().await {
        match result {
            Ok(page) => {
                for sub in page.value {
                    subscriptions.push(AzureSubscription {
                        id: sub.subscription_id.unwrap_or_default(),
                        name: sub.display_name.unwrap_or_else(|| "Unnamed".to_string()),
                        state: sub.state.map(|s| format!("{:?}", s)).unwrap_or_else(|| "Unknown".to_string()),
                        tenant_id: sub.tenant_id.unwrap_or_default(),
                    });
                }
            }
            Err(e) => {
                return Err(AzureError::ApiError(format!("Failed to list subscriptions: {}", e)));
            }
        }
    }
    
    info!("Successfully fetched {} subscriptions", subscriptions.len());
    Ok(subscriptions)
}
```

4. Add integration test:
```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::auth::AzureAuth;
    
    #[tokio::test]
    #[ignore] // Only run with --ignored flag when Azure credentials are available
    async fn test_list_subscriptions() {
        let auth = AzureAuth::new_auto().await.expect("Failed to authenticate");
        let client = AzureClient::new(Arc::new(auth));
        
        let subscriptions = client.list_subscriptions().await.expect("Failed to list subscriptions");
        assert!(!subscriptions.is_empty(), "Should have at least one subscription");
    }
}
```

---

## Priority 2: Wire Up Subscription Loading in TUI (30 minutes)

### Update App Initialization

**File**: `crates/aztui-core/src/app.rs`

Add Azure client and load subscriptions:

```rust
use aztui_azure::{AzureAuth, AzureClient};
use std::sync::Arc;

pub struct App {
    state: AppState,
    cache: Cache,
    azure_client: AzureClient,
}

impl App {
    pub async fn new() -> anyhow::Result<Self> {
        info!("Initializing aztui application");

        // Initialize cache
        let cache_dir = Self::cache_dir()?;
        std::fs::create_dir_all(&cache_dir)?;
        let cache_path = cache_dir.join("aztui.db");
        let cache = Cache::new(&cache_path)?;

        // Initialize Azure authentication
        let auth = AzureAuth::new_auto().await
            .context("Failed to authenticate with Azure")?;
        let azure_client = AzureClient::new(Arc::new(auth));

        // Initialize state
        let mut state = AppState::new();
        
        // Load subscriptions
        info!("Loading subscriptions...");
        let subscriptions = azure_client.list_subscriptions().await
            .context("Failed to load subscriptions")?;
        
        // Convert to core models
        state.subscriptions = subscriptions.into_iter()
            .map(|s| crate::models::Subscription {
                id: s.id,
                name: s.name,
                state: s.state,
                tenant_id: s.tenant_id,
            })
            .collect();
        
        info!("Loaded {} subscriptions", state.subscriptions.len());

        Ok(Self { state, cache, azure_client })
    }
    
    pub fn azure_client(&self) -> &AzureClient {
        &self.azure_client
    }
}
```

---

## Priority 3: Implement Resource Group Navigation (1 hour)

### Update TUI App to Load Resource Groups

**File**: `crates/aztui-tui/src/app.rs`

Update `handle_subscription_select`:

```rust
async fn handle_subscription_select(&mut self) -> Result<bool> {
    if let Some(subscription) = self.state().selected_subscription().cloned() {
        info!("Selected subscription: {}", subscription.name);
        
        // Set loading state
        self.state_mut().set_loading(true);
        
        // Load resource groups
        match self.core().azure_client().list_resource_groups(&subscription.id).await {
            Ok(azure_rgs) => {
                // Convert to core models
                let resource_groups: Vec<_> = azure_rgs.into_iter()
                    .map(|rg| aztui_core::models::ResourceGroup {
                        id: rg.id,
                        name: rg.name,
                        location: rg.location,
                        subscription_id: rg.subscription_id,
                        tags: rg.tags,
                    })
                    .collect();
                
                info!("Loaded {} resource groups", resource_groups.len());
                
                // Update state
                let state = self.state_mut();
                state.selected_subscription = Some(subscription.clone());
                state.view = ViewMode::ResourceGroupList;
                state.list_state.reset();
                state.set_loading(false);
                
                // Store resource groups (need to add field to AppState)
                // state.resource_groups = resource_groups;
            }
            Err(e) => {
                self.state_mut().set_error(Some(format!("Failed to load resource groups: {}", e)));
                self.state_mut().set_loading(false);
            }
        }
    }
    Ok(true)
}
```

### Add Resource Groups to AppState

**File**: `crates/aztui-core/src/state.rs`

```rust
pub struct AppState {
    // ... existing fields ...
    
    /// Current resource groups
    pub resource_groups: Vec<ResourceGroup>,
}
```

---

## Priority 4: Implement Cache Integration (1 hour)

### Cache Subscriptions

**File**: `crates/aztui-core/src/app.rs`

```rust
impl App {
    pub async fn load_subscriptions(&mut self, force_refresh: bool) -> anyhow::Result<()> {
        // Try cache first
        if !force_refresh {
            let cached = self.cache.repository()
                .list_subscriptions(None)?;
            
            if !cached.is_empty() {
                info!("Loaded {} subscriptions from cache", cached.len());
                self.state.subscriptions = cached;
                return Ok(());
            }
        }
        
        // Fetch from Azure
        info!("Fetching subscriptions from Azure...");
        let azure_subs = self.azure_client.list_subscriptions().await?;
        
        // Convert and cache
        let subscriptions: Vec<_> = azure_subs.into_iter()
            .map(|s| Subscription {
                id: s.id,
                name: s.name,
                state: s.state,
                tenant_id: s.tenant_id,
            })
            .collect();
        
        // Save to cache
        for sub in &subscriptions {
            self.cache.repository().upsert_subscription(sub)?;
        }
        
        self.state.subscriptions = subscriptions;
        Ok(())
    }
}
```

---

## Priority 5: Add UI for Resource Groups (30 minutes)

### Update UI Rendering

**File**: `crates/aztui-tui/src/ui/mod.rs`

Add resource group list rendering:

```rust
fn draw_content(f: &mut Frame, area: Rect, app: &TuiApp) -> Result<()> {
    match app.state().view {
        ViewMode::SubscriptionList => draw_subscription_list(f, area, app),
        ViewMode::ResourceGroupList => draw_resource_group_list(f, area, app),
        // ... other views ...
    }
}

fn draw_resource_group_list(f: &mut Frame, area: Rect, app: &TuiApp) -> Result<()> {
    let items: Vec<_> = app.state().resource_groups.iter()
        .map(|rg| {
            list_item_spans(&[
                (&rg.name, Color::White),
                (" | ", Color::DarkGray),
                (&rg.location, Color::Cyan),
            ])
        })
        .collect();
    
    let mut list_state = SelectableListState::new(app.state().list_state.selected());
    
    let list = SelectableList::new(items)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("Resource Groups"));
    
    f.render_stateful_widget(list, area, &mut list_state);
    Ok(())
}
```

---

## Priority 6: Add Tests (1 hour)

### Integration Tests

**File**: `crates/aztui-core/tests/integration_test.rs`

```rust
use aztui_core::App;
use tempfile::tempdir;

#[tokio::test]
#[ignore] // Requires Azure credentials
async fn test_app_initialization() {
    let app = App::new().await.expect("Failed to create app");
    assert!(!app.state().subscriptions.is_empty());
}

#[tokio::test]
async fn test_cache_persistence() {
    let temp_dir = tempdir().unwrap();
    let cache_path = temp_dir.path().join("test.db");
    
    // Create cache and add subscription
    {
        let cache = aztui_core::cache::Cache::new(&cache_path).unwrap();
        let sub = aztui_core::models::Subscription {
            id: "test-id".to_string(),
            name: "Test Sub".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "test-tenant".to_string(),
        };
        cache.repository().upsert_subscription(&sub).unwrap();
    }
    
    // Reopen cache and verify
    {
        let cache = aztui_core::cache::Cache::new(&cache_path).unwrap();
        let subs = cache.repository().list_subscriptions(None).unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].name, "Test Sub");
    }
}
```

---

## Priority 7: Polish and Documentation (30 minutes)

### Update README

Add usage instructions:

```markdown
## Usage

### Prerequisites

- Azure CLI installed and authenticated (`az login`)
- Or Azure credentials configured via environment variables

### Running

```bash
cargo run --release
```

### Keyboard Shortcuts

- `j/k` or `↓/↑` - Navigate lists
- `Enter` - Select item
- `Esc` - Go back
- `g/G` - Jump to first/last item
- `r` - Refresh current view
- `/` - Filter (coming soon)
- `Ctrl+p` - Command palette (coming soon)
- `?` - Help
- `q` or `Ctrl+c` - Quit
```

### Add CHANGELOG

Track changes and versions.

---

## Testing Checklist

After implementing each priority:

- [ ] Code compiles without errors
- [ ] All tests pass (`cargo test --all`)
- [ ] Clippy shows no errors (`cargo clippy`)
- [ ] Manual testing in terminal works
- [ ] Error handling is graceful
- [ ] Loading states are visible
- [ ] Navigation works as expected

---

## Estimated Timeline

| Priority | Task | Time | Cumulative |
|----------|------|------|------------|
| 1 | Azure subscription integration | 2h | 2h |
| 2 | Wire up TUI loading | 30m | 2.5h |
| 3 | Resource group navigation | 1h | 3.5h |
| 4 | Cache integration | 1h | 4.5h |
| 5 | UI for resource groups | 30m | 5h |
| 6 | Add tests | 1h | 6h |
| 7 | Polish and docs | 30m | 6.5h |

**Total**: ~6.5 hours of focused development

---

## Success Criteria

The aztui application is feature-complete for MVP when:

1. ✅ Compiles and runs without errors
2. ✅ Authenticates with Azure automatically
3. ✅ Lists subscriptions from Azure API
4. ✅ Caches subscriptions in SQLite
5. ✅ Allows navigation to resource groups
6. ✅ Lists resource groups for selected subscription
7. ✅ Displays resources in selected group
8. ✅ Keyboard navigation works smoothly
9. ✅ Error handling is user-friendly
10. ✅ Loading states are clear

---

**Generated**: Iteration 11  
**Status**: Ready to execute after blocker is resolved  
**Confidence**: High - Clear implementation path
