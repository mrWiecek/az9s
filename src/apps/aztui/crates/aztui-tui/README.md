# aztui-tui

Terminal User Interface (TUI) implementation for aztui using Ratatui and Crossterm.

## Overview

This crate provides the terminal interface layer for aztui, handling:

- Terminal lifecycle management (setup, cleanup, panic recovery)
- Event loop with async support
- Keyboard and mouse input handling
- View rendering with Ratatui
- Error display and user feedback

## Architecture

### Core Components

#### `lib.rs` - TUI Runner
- Terminal setup and teardown
- Panic hook for graceful terminal restoration
- Main event loop coordination
- Error handling and recovery

#### `event.rs` - Event Handling
- Background task for polling terminal events
- Channel-based event distribution
- Support for keyboard, mouse, resize, and tick events
- Configurable tick rate for periodic updates

#### `app.rs` - Application Logic
- TUI state wrapper around core `App`
- Keyboard navigation (vim-style and arrow keys)
- Mouse scroll support
- View-specific event handlers
- Refresh and data loading coordination

#### `ui/mod.rs` - Rendering
- Main draw function with header/content/footer layout
- View-specific rendering functions
- Error overlay modal
- Color-coded status indicators
- Responsive list rendering with scrolling

#### `ui/widgets.rs` - Custom Widgets
- `SelectableList` - Scrollable list with keyboard navigation
- Automatic viewport management
- Highlight styling for selected items

## Key Features

### Robust Terminal Lifecycle

```rust
// Automatic cleanup on panic
panic::set_hook(Box::new(move |panic_info| {
    let _ = restore_terminal();
    original_hook(panic_info);
}));
```

### Async Event Handling

Events are processed in a background task and sent through a channel:

```rust
pub async fn next(&mut self) -> Option<Event> {
    self.rx.recv().await
}
```

### Keyboard Navigation

- **Vim-style**: `j/k` for up/down, `g/G` for first/last
- **Arrow keys**: Standard navigation
- **Page navigation**: `Ctrl+d/u` for page down/up
- **Selection**: `Enter` to select, `Esc` to go back
- **Actions**: `r` to refresh, `q` to quit, `?` for help

### Mouse Support

- Scroll wheel navigation in lists
- Click support (future enhancement)

### Error Handling

- Errors displayed as centered modal overlays
- Non-blocking error display
- Press `Esc` to dismiss errors
- Automatic error recovery in event loop

### View Rendering

Each view has dedicated rendering logic:

- **Subscription List**: Color-coded subscription states
- **Resource List**: Type-specific coloring, truncated names
- **Resource Groups**: Placeholder for future implementation
- **Resource Detail**: Placeholder for future implementation
- **Logs**: Placeholder for future implementation

## Usage

```rust
use aztui_core::App;
use aztui_tui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize core application
    let app = App::new().await?;
    
    // Run TUI
    aztui_tui::run(app).await?;
    
    Ok(())
}
```

## Event Flow

```
Terminal Events → EventHandler → Event Channel → TuiApp → Core App
                                                    ↓
                                                UI Render
```

1. **EventHandler** polls terminal for events in background task
2. Events sent through unbounded channel
3. **TuiApp** receives events and handles them
4. State updates trigger UI re-render
5. **UI** draws current state to terminal

## Error Recovery

The TUI is designed to be resilient:

- **Terminal errors**: Logged but don't crash the app
- **Render errors**: Displayed in error overlay
- **Event errors**: Captured and shown to user
- **Panic recovery**: Terminal always restored

## Testing

Tests cover:

- Event handler creation and basic operation
- Terminal lifecycle (setup/restore)
- UI module compilation
- App structure (integration tests require Azure credentials)

Run tests:

```bash
cargo test -p aztui-tui
```

## Performance Considerations

- **Tick rate**: 250ms default, configurable
- **Event polling**: Non-blocking with tokio::select!
- **Rendering**: Only on events, not continuous
- **List scrolling**: Viewport-based, efficient for large lists

## Future Enhancements

- [ ] Command palette implementation
- [ ] Filter/search mode
- [ ] Help view with keybinding reference
- [ ] Resource detail view with tabs
- [ ] Log streaming view
- [ ] Multi-pane layout
- [ ] Theme customization
- [ ] Mouse click support for selection
- [ ] Copy-to-clipboard functionality

## Dependencies

- **ratatui**: Terminal UI framework
- **crossterm**: Cross-platform terminal manipulation
- **tokio**: Async runtime for event handling
- **anyhow**: Error handling
- **tracing**: Logging and diagnostics

## Contributing

When adding new views or features:

1. Add event handling in `app.rs`
2. Add rendering logic in `ui/mod.rs`
3. Update keybindings in footer
4. Add tests for new functionality
5. Update this README

## License

MIT OR Apache-2.0
