# aztui Iteration Plan

## Overview

This document outlines the detailed iteration plan for building aztui from its current scaffolded state to a fully functional Azure resource browser.

## Current State (Iteration 0)

### ✅ Completed
- Workspace structure with 4 crates
- Basic Cargo.toml with dependencies
- Core domain models (Subscription, ResourceGroup, Resource)
- SQLite cache implementation with schema
- Authentication abstraction (Azure CLI, default credential)
- TUI scaffolding with event handling
- Basic UI rendering with header/footer
- View mode state management
- Navigation stack for back navigation
- Keyboard event routing

### ⚠️ Incomplete
- Azure API client implementation (placeholder only)
- Actual subscription/resource fetching
- List navigation (selection, scrolling)
- Resource group views
- Resource detail views
- Error display in UI
- Loading states
- Command palette
- Help system
- Filtering

### 🐛 Known Issues
- No actual Azure API calls (returns empty lists)
- No list selection/highlighting
- No scrolling for long lists
- No loading indicators
- No error handling in UI

## Iteration Breakdown

### Iteration 1: Azure API Integration (Subscriptions)
**Goal**: Fetch and display real Azure subscriptions

**Tasks**:
1. Implement `AzureClient::list_subscriptions()` using Azure SDK
2. Add conversion from Azure SDK types to domain models
3. Wire up subscription fetching in `App::new()`
4. Add loading state to `AppState`
5. Display loading indicator in UI
6. Handle authentication errors gracefully
7. Cache fetched subscriptions
8. Add refresh functionality

**Files to Modify**:
- `crates/aztui-azure/src/client.rs`
- `crates/aztui-core/src/app.rs`
- `crates/aztui-core/src/state.rs`
- `crates/aztui-tui/src/ui/mod.rs`

**Validation**:
- `cargo check --workspace`
- `cargo test --workspace`
- Manual test with real Azure subscription

**Estimated Complexity**: Medium

---

### Iteration 2: List Navigation and Selection
**Goal**: Enable keyboard navigation through subscription list

**Tasks**:
1. Add `selected_index` to `AppState`
2. Implement up/down navigation (j/k, arrow keys)
3. Add visual highlighting for selected item
4. Implement Enter to select subscription
5. Add g/G for top/bottom navigation
6. Handle empty lists gracefully
7. Add list scrolling for long lists

**Files to Modify**:
- `crates/aztui-core/src/state.rs`
- `crates/aztui-tui/src/app.rs`
- `crates/aztui-tui/src/ui/mod.rs`
- `crates/aztui-tui/src/ui/widgets.rs` (new list widget)

**Validation**:
- Navigate through subscription list
- Select subscription with Enter
- Scroll through long lists

**Estimated Complexity**: Medium

---

### Iteration 3: Resource Group View
**Goal**: Display resource groups for selected subscription

**Tasks**:
1. Implement `AzureClient::list_resource_groups()`
2. Add resource group fetching to `App`
3. Add `resource_groups` field to `AppState`
4. Implement `ResourceGroupList` view rendering
5. Add navigation from subscription to resource groups
6. Cache resource groups
7. Add back navigation (Esc)

**Files to Modify**:
- `crates/aztui-azure/src/client.rs`
- `crates/aztui-core/src/app.rs`
- `crates/aztui-core/src/state.rs`
- `crates/aztui-core/src/cache.rs`
- `crates/aztui-tui/src/app.rs`
- `crates/aztui-tui/src/ui/mod.rs`

**Validation**:
- Select subscription and view resource groups
- Navigate back to subscription list
- Verify caching works

**Estimated Complexity**: Medium

---

### Iteration 4: Resource List View
**Goal**: Display resources in selected scope

**Tasks**:
1. Implement `AzureClient::list_resources()`
2. Add resource fetching to `App`
3. Implement `ResourceList` view rendering
4. Add resource type icons/colors
5. Add location and status display
6. Support filtering by resource group
7. Add sorting options

**Files to Modify**:
- `crates/aztui-azure/src/client.rs`
- `crates/aztui-core/src/app.rs`
- `crates/aztui-tui/src/app.rs`
- `crates/aztui-tui/src/ui/mod.rs`

**Validation**:
- View resources in subscription
- View resources in resource group
- Verify resource metadata display

**Estimated Complexity**: High

---

### Iteration 5: Error Handling and Display
**Goal**: Gracefully handle and display errors

**Tasks**:
1. Add error state to `AppState`
2. Create error display widget
3. Handle authentication errors
4. Handle API errors (rate limiting, permissions)
5. Handle network errors
6. Add retry logic
7. Show cached data when API fails

**Files to Modify**:
- `crates/aztui-core/src/state.rs`
- `crates/aztui-azure/src/client.rs`
- `crates/aztui-tui/src/ui/mod.rs`
- `crates/aztui-tui/src/ui/widgets.rs`

**Validation**:
- Test with invalid credentials
- Test with network disconnected
- Test with insufficient permissions

**Estimated Complexity**: Medium

---

### Iteration 6: Resource Detail View
**Goal**: Show detailed information for selected resource

**Tasks**:
1. Add `ResourceDetail` view implementation
2. Fetch extended resource properties
3. Display resource properties in formatted view
4. Add JSON view toggle
5. Show resource tags
6. Show resource SKU and kind
7. Add copy-to-clipboard for IDs

**Files to Modify**:
- `crates/aztui-azure/src/client.rs`
- `crates/aztui-core/src/models.rs`
- `crates/aztui-tui/src/app.rs`
- `crates/aztui-tui/src/ui/mod.rs`

**Validation**:
- View details for various resource types
- Toggle between formatted and JSON views
- Verify all metadata is displayed

**Estimated Complexity**: Medium

---

### Iteration 7: Search and Filtering
**Goal**: Enable searching and filtering resources

**Tasks**:
1. Add filter input mode
2. Implement fuzzy search
3. Filter by resource type
4. Filter by location
5. Filter by tags
6. Show filter status in UI
7. Clear filter with Esc

**Files to Modify**:
- `crates/aztui-core/src/state.rs`
- `crates/aztui-tui/src/app.rs`
- `crates/aztui-tui/src/ui/mod.rs`

**Validation**:
- Search for resources by name
- Filter by type and location
- Clear filters

**Estimated Complexity**: Medium

---

### Iteration 8: Command Palette
**Goal**: Implement command palette for quick actions

**Tasks**:
1. Create command palette widget
2. Add command registry
3. Implement fuzzy command search
4. Add common commands (refresh, navigate, quit)
5. Add keyboard shortcuts display
6. Add command history

**Files to Modify**:
- `crates/aztui-core/src/state.rs`
- `crates/aztui-tui/src/app.rs`
- `crates/aztui-tui/src/ui/mod.rs`
- `crates/aztui-tui/src/ui/widgets.rs`

**Validation**:
- Open palette with Ctrl+P
- Search and execute commands
- Verify shortcuts work

**Estimated Complexity**: High

---

### Iteration 9: Help System
**Goal**: Provide contextual help and keybinding reference

**Tasks**:
1. Create help view
2. Add context-aware keybinding display
3. Add view-specific help
4. Add getting started guide
5. Add troubleshooting tips

**Files to Modify**:
- `crates/aztui-core/src/state.rs`
- `crates/aztui-tui/src/app.rs`
- `crates/aztui-tui/src/ui/mod.rs`

**Validation**:
- Open help with ?
- Verify keybindings are accurate
- Check help for each view

**Estimated Complexity**: Low

---

### Iteration 10: Performance Optimization
**Goal**: Optimize for large resource sets

**Tasks**:
1. Implement virtual scrolling for large lists
2. Add pagination for API calls
3. Optimize cache queries
4. Add background refresh
5. Implement incremental loading
6. Add progress indicators

**Files to Modify**:
- `crates/aztui-core/src/cache.rs`
- `crates/aztui-azure/src/client.rs`
- `crates/aztui-tui/src/ui/widgets.rs`

**Validation**:
- Test with 1000+ resources
- Verify smooth scrolling
- Check memory usage

**Estimated Complexity**: High

---

### Iteration 11: Multi-Pane Layout
**Goal**: Support split-pane views

**Tasks**:
1. Implement pane management
2. Add horizontal/vertical splits
3. Add focus switching between panes
4. Add pane resize
5. Save layout preferences

**Files to Modify**:
- `crates/aztui-core/src/state.rs`
- `crates/aztui-tui/src/app.rs`
- `crates/aztui-tui/src/ui/mod.rs`

**Validation**:
- Split view horizontally/vertically
- Switch focus between panes
- Resize panes

**Estimated Complexity**: High

---

### Iteration 12: Resource Logs (Basic)
**Goal**: Display logs for supported resources

**Tasks**:
1. Implement log fetching for VMs
2. Add log view rendering
3. Add log filtering
4. Add log export
5. Add auto-refresh

**Files to Modify**:
- `crates/aztui-azure/src/client.rs`
- `crates/aztui-core/src/models.rs`
- `crates/aztui-tui/src/ui/mod.rs`

**Validation**:
- View logs for VM
- Filter logs by level
- Export logs to file

**Estimated Complexity**: High

---

### Iteration 13: Configuration Management
**Goal**: Support user configuration

**Tasks**:
1. Add config file support (TOML)
2. Add theme customization
3. Add keybinding customization
4. Add default view preferences
5. Add cache settings

**Files to Create**:
- `crates/aztui-core/src/config.rs`

**Files to Modify**:
- `crates/aztui-core/src/app.rs`
- `crates/aztui-tui/src/ui/mod.rs`

**Validation**:
- Create custom config
- Verify settings are applied
- Test config reload

**Estimated Complexity**: Medium

---

### Iteration 14: Resource Actions (Basic)
**Goal**: Enable basic resource operations

**Tasks**:
1. Add action menu for resources
2. Implement VM start/stop
3. Implement resource deletion (with confirmation)
4. Add action status display
5. Add action history

**Files to Modify**:
- `crates/aztui-azure/src/client.rs`
- `crates/aztui-core/src/app.rs`
- `crates/aztui-tui/src/app.rs`
- `crates/aztui-tui/src/ui/mod.rs`

**Validation**:
- Start/stop VM
- Delete resource with confirmation
- Verify action status

**Estimated Complexity**: High

---

### Iteration 15: Metrics and Monitoring
**Goal**: Display resource metrics

**Tasks**:
1. Fetch Azure Monitor metrics
2. Add metrics view
3. Display CPU, memory, network graphs
4. Add time range selection
5. Add metric export

**Files to Modify**:
- `crates/aztui-azure/src/client.rs`
- `crates/aztui-core/src/models.rs`
- `crates/aztui-tui/src/ui/mod.rs`
- `crates/aztui-tui/src/ui/widgets.rs` (chart widget)

**Validation**:
- View metrics for VM
- Change time range
- Export metrics data

**Estimated Complexity**: Very High

---

### Iteration 16: Export and Reporting
**Goal**: Export data in various formats

**Tasks**:
1. Add JSON export
2. Add CSV export
3. Add Markdown export
4. Add clipboard integration
5. Add export templates

**Files to Create**:
- `crates/aztui-core/src/export.rs`

**Files to Modify**:
- `crates/aztui-tui/src/app.rs`

**Validation**:
- Export resource list to JSON
- Export to CSV
- Copy to clipboard

**Estimated Complexity**: Medium

---

### Iteration 17: Polish and UX Improvements
**Goal**: Refine user experience

**Tasks**:
1. Add animations and transitions
2. Improve color scheme
3. Add status bar with context
4. Add breadcrumb navigation
5. Improve error messages
6. Add tooltips
7. Add keyboard shortcut hints

**Files to Modify**:
- `crates/aztui-tui/src/ui/mod.rs`
- `crates/aztui-tui/src/ui/widgets.rs`

**Validation**:
- User testing
- Accessibility review
- Performance testing

**Estimated Complexity**: Medium

---

### Iteration 18: Documentation and Release
**Goal**: Prepare for release

**Tasks**:
1. Write comprehensive README
2. Add usage examples
3. Create user guide
4. Add architecture documentation
5. Add contributing guidelines
6. Set up CI/CD
7. Create release binaries
8. Publish to crates.io

**Files to Create**:
- `docs/USER_GUIDE.md`
- `docs/CONTRIBUTING.md`
- `.github/workflows/ci.yml`

**Validation**:
- Documentation review
- Binary testing on multiple platforms
- Release checklist

**Estimated Complexity**: Medium

---

## Iteration Priorities

### Must Have (MVP)
- Iterations 1-6: Core functionality
- Iteration 9: Help system

### Should Have
- Iterations 7-8: Search and command palette
- Iteration 10: Performance optimization
- Iteration 13: Configuration

### Nice to Have
- Iterations 11-12: Multi-pane and logs
- Iterations 14-16: Actions, metrics, export

### Future
- Iteration 17: Polish
- Iteration 18: Release preparation

## Success Criteria

### Per Iteration
- All tests pass
- `cargo clippy` has no warnings
- Manual testing completed
- Documentation updated

### Overall
- Can browse Azure resources efficiently
- Faster than Azure Portal for common tasks
- Stable and reliable
- Good user experience
- Well documented

## Risk Mitigation

### Azure SDK Changes
- Pin SDK versions
- Monitor SDK releases
- Test with multiple SDK versions

### Performance Issues
- Profile early and often
- Set performance benchmarks
- Test with large datasets

### User Experience
- Get early feedback
- Iterate on UX
- Provide good defaults

## Next Steps

1. Begin Iteration 1: Azure API Integration
2. Set up development environment
3. Test with real Azure subscription
4. Gather initial feedback
