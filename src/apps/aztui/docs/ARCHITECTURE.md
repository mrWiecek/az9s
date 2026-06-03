# aztui Architecture

## Overview

`aztui` is a terminal-based Azure resource browser inspired by k9s, built with Rust, Ratatui, and the Azure SDK. The application provides keyboard-first navigation through Azure subscriptions, resource groups, and resources with local caching for performance.

## Design Principles

1. **Separation of Concerns**: Clear boundaries between UI, business logic, and Azure API integration
2. **Async-First**: Built on Tokio for responsive UI and concurrent API calls
3. **Offline-Capable**: SQLite cache enables browsing previously fetched resources
4. **Keyboard-Driven**: Vim-inspired navigation with context-aware keybindings
5. **Modular**: Workspace structure allows independent development and testing

## Workspace Structure

```
aztui/
├── crates/
│   ├── aztui/           # Main binary and application entry point
│   ├── aztui-core/      # Core domain models, state, and business logic
│   ├── aztui-azure/     # Azure SDK integration and authentication
│   └── aztui-tui/       # Ratatui UI components and event handling
├── Cargo.toml           # Workspace configuration
└── README.md
```

## Crate Responsibilities

### aztui (Binary Crate)

**Purpose**: Application entry point and initialization

**Key Files**:
- `main.rs`: Initializes logging, creates core app, launches TUI

**Dependencies**: 
- `aztui-core`: Core application logic
- `aztui-azure`: Azure integration
- `aztui-tui`: Terminal UI
- `tokio`: Async runtime
- `tracing`: Logging

### aztui-core (Library Crate)

**Purpose**: Domain models, application state, and business logic

**Key Modules**:
- `app.rs`: Main application coordinator, manages state and cache
- `state.rs`: Application state management (view mode, navigation, selections)
- `models.rs`: Domain models (Subscription, ResourceGroup, Resource)
- `cache.rs`: SQLite-backed cache for offline access

**Responsibilities**:
- Maintain application state (current view, selections, navigation history)
- Coordinate between Azure API and cache
- Provide business logic for resource operations
- Manage local SQLite database

**Dependencies**:
- `aztui-azure`: For Azure types and errors
- `rusqlite`: SQLite database
- `serde`: Serialization
- `tokio`: Async support

### aztui-azure (Library Crate)

**Purpose**: Azure SDK integration and authentication

**Key Modules**:
- `auth.rs`: Authentication providers (Azure CLI, default credential chain)
- `client.rs`: Azure API client wrapper
- `models.rs`: Azure API response models

**Responsibilities**:
- Handle Azure authentication (CLI, device code, managed identity)
- Provide high-level API for listing subscriptions, resource groups, resources
- Convert Azure SDK types to domain models
- Handle API errors and retries

**Dependencies**:
- `azure_core`, `azure_identity`: Azure SDK authentication
- `azure_mgmt_*`: Azure management SDKs
- `serde`: Serialization

### aztui-tui (Library Crate)

**Purpose**: Terminal UI rendering and event handling

**Key Modules**:
- `app.rs`: TUI application wrapper, event routing
- `event.rs`: Event handler for keyboard and terminal events
- `ui/mod.rs`: Main UI rendering logic
- `ui/widgets.rs`: Reusable UI components

**Responsibilities**:
- Manage terminal lifecycle (raw mode, alternate screen)
- Handle keyboard input and route to appropriate handlers
- Render UI using Ratatui
- Provide view-specific rendering (subscription list, resource list, etc.)

**Dependencies**:
- `aztui-core`: For application state
- `ratatui`: TUI framework
- `crossterm`: Terminal manipulation
- `tokio`: Async event handling

## Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│                         User Input                          │
│                      (Keyboard Events)                      │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                      aztui-tui                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ EventHandler → TuiApp → View-specific handlers       │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                      aztui-core                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ App → AppState → Cache                               │  │
│  │  ↓                                                    │  │
│  │ Business Logic                                       │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────┬───────────────────────┬────────────────────┘
                 │                       │
                 ▼                       ▼
┌────────────────────────────┐  ┌──────────────────────────┐
│      aztui-azure           │  │    SQLite Cache          │
│  ┌──────────────────────┐  │  │  ┌────────────────────┐ │
│  │ AzureAuth            │  │  │  │ subscriptions      │ │
│  │ AzureClient          │  │  │  │ resource_groups    │ │
│  │   ↓                  │  │  │  │ resources          │ │
│  │ Azure SDK            │  │  │  └────────────────────┘ │
│  └──────────────────────┘  │  └──────────────────────────┘
└────────────────────────────┘
                 │
                 ▼
┌────────────────────────────────────────────────────────────┐
│                    Azure REST APIs                         │
└────────────────────────────────────────────────────────────┘
```

## State Management

### AppState Structure

```rust
pub struct AppState {
    pub view: ViewMode,                          // Current view
    pub selected_subscription: Option<Subscription>,
    pub subscriptions: Vec<Subscription>,
    pub resources: Vec<Resource>,
    pub navigation_stack: Vec<NavigationContext>,
    pub command_palette_open: bool,
    pub filter: String,
}
```

### View Modes

1. **SubscriptionList**: Browse available Azure subscriptions
2. **ResourceGroupList**: Browse resource groups in selected subscription
3. **ResourceList**: Browse resources in selected scope
4. **ResourceDetail**: View detailed information about a resource
5. **Logs**: View logs for a resource (future)

### Navigation Flow

```
SubscriptionList
    ↓ (Enter on subscription)
ResourceGroupList
    ↓ (Enter on resource group)
ResourceList
    ↓ (Enter on resource)
ResourceDetail
    ↓ (Esc to go back)
```

## Key Bindings

### Global
- `q` or `Ctrl+C`: Quit application
- `Ctrl+P`: Toggle command palette
- `?`: Show help
- `/`: Start filter mode
- `Esc`: Go back / close palette

### View-Specific
- `Enter`: Select / drill down
- `r`: Refresh current view
- `j/k` or `↓/↑`: Navigate list (future)
- `g/G`: Go to top/bottom (future)

## Cache Strategy

### Purpose
- Enable offline browsing of previously fetched resources
- Reduce API calls and improve responsiveness
- Persist user's exploration history

### Schema
- **subscriptions**: Cached subscription list
- **resource_groups**: Cached resource groups per subscription
- **resources**: Cached resources with full metadata

### Cache Invalidation
- Manual refresh (`r` key)
- Time-based expiration (future)
- On-demand fetch for missing data

## Authentication Flow

1. **Auto-detection**: Try Azure CLI first, fall back to default credential chain
2. **Azure CLI**: Use `az login` credentials (most common for operators)
3. **Default Chain**: Managed identity, environment variables, interactive browser
4. **Device Code**: Manual fallback for restricted environments (future)

## Error Handling

### Error Types
- `CoreError`: Business logic and cache errors
- `AzureError`: Azure API and authentication errors
- `anyhow::Error`: General application errors

### Strategy
- Graceful degradation: Show cached data if API fails
- User-friendly error messages in UI
- Detailed logging for debugging

## Performance Considerations

1. **Async Operations**: All Azure API calls are async, UI remains responsive
2. **Lazy Loading**: Fetch resources only when needed
3. **Caching**: Reduce redundant API calls
4. **Pagination**: Handle large resource lists (future)
5. **Incremental Rendering**: Update UI as data arrives (future)

## Testing Strategy

### Unit Tests
- Core business logic in `aztui-core`
- Azure client mocking in `aztui-azure`
- Widget rendering in `aztui-tui`

### Integration Tests
- End-to-end navigation flows
- Cache persistence
- Authentication scenarios

### Manual Testing
- Real Azure subscriptions
- Various terminal sizes
- Different authentication methods

## Future Enhancements

### Phase 1 (Current)
- ✅ Workspace structure
- ✅ Basic TUI scaffolding
- ✅ Authentication abstraction
- ✅ SQLite cache
- ⏳ Subscription listing
- ⏳ Resource browsing

### Phase 2
- Resource group navigation
- Detailed resource views
- Search and filtering
- Improved list navigation (vim-style)

### Phase 3
- Resource logs integration
- Metrics and monitoring
- Resource actions (start/stop VMs, etc.)
- Configuration management

### Phase 4
- Multi-pane views
- Custom dashboards
- Export and reporting
- Plugin system

## Development Workflow

### Build and Run
```bash
cargo build --release
cargo run
```

### Testing
```bash
cargo test --workspace
cargo clippy --workspace
```

### Debugging
```bash
RUST_LOG=debug cargo run
```

### Code Organization Guidelines
1. Keep UI logic in `aztui-tui`
2. Keep business logic in `aztui-core`
3. Keep Azure API calls in `aztui-azure`
4. Use `anyhow` for application errors, `thiserror` for library errors
5. Add tracing for debugging, not for user messages

## Dependencies Rationale

- **Ratatui**: Modern, actively maintained TUI framework
- **Crossterm**: Cross-platform terminal manipulation
- **Tokio**: Industry-standard async runtime
- **Azure SDK**: Official Azure Rust SDK
- **SQLite**: Embedded database, no external dependencies
- **Serde**: De facto serialization standard
- **Tracing**: Structured logging with filtering

## Security Considerations

1. **Credentials**: Never log or cache credentials
2. **Cache**: Store only non-sensitive resource metadata
3. **Permissions**: Respect Azure RBAC, fail gracefully on access denied
4. **Updates**: Keep Azure SDK updated for security patches
