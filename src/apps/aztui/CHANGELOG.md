# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Planned
- Command palette (Ctrl+P)
- Resource actions (start/stop VMs, delete resources)
- Multi-pane support
- Log viewer with Azure Monitor integration
- Configuration file support
- Custom keybindings
- Theme customization

## [0.1.0] - 2024

### Added

#### Core Features
- **Azure Authentication**: Support for Azure CLI and default credential chain
- **Subscription Browser**: List and navigate Azure subscriptions
- **Resource Group Browser**: View resource groups within subscriptions
- **Resource Browser**: Browse all resources with filtering
- **Resource Detail View**: Detailed information for individual resources
- **SQLite Cache**: Local caching for offline access and performance

#### User Interface
- **Keyboard Navigation**: Vim-style navigation (j/k, g/G) and arrow keys
- **Filter Mode**: Real-time filtering with `/` key
- **Help System**: Comprehensive help screen with `?` key
- **Status Bar**: Shows data source (Azure/Cache) and connection status
- **Breadcrumb Navigation**: Clear view hierarchy display
- **Error Handling**: User-friendly error messages with dismissal
- **Loading States**: Visual feedback during data loading

#### Technical
- **Workspace Structure**: Clean separation into 4 crates
  - `aztui`: Main binary
  - `aztui-core`: Business logic and state management
  - `aztui-azure`: Azure SDK integration
  - `aztui-tui`: Terminal UI components
- **Async Architecture**: Built on Tokio for responsive UI
- **Comprehensive Testing**: 55 unit and integration tests
- **Documentation**: 25+ documentation files covering architecture, usage, and development

#### Keyboard Shortcuts
- `q` / `Ctrl+C`: Quit application
- `?`: Toggle help screen
- `/`: Enter filter mode
- `r`: Refresh current view
- `j/k` or `↑/↓`: Navigate up/down
- `g/G`: Jump to first/last item
- `Ctrl+D/U`: Page down/up
- `Enter`: Select item
- `Esc`: Go back / dismiss error / clear filter
- `Backspace`: Go back

#### Cache Features
- **Automatic Caching**: Resources cached for offline access
- **TTL Support**: Configurable cache expiration
- **Cache Warming**: Pre-load strategies (minimal, standard, full)
- **Cache Statistics**: Monitor cache size and contents
- **Vacuum Support**: Database optimization

### Technical Details

#### Dependencies
- Rust 1.75+
- Ratatui 0.26 (Terminal UI)
- Crossterm 0.27 (Terminal control)
- Azure SDK 0.19 (Azure integration)
- Tokio 1.35 (Async runtime)
- SQLite 0.31 (Local cache)

#### Performance
- Startup time: < 5s (network dependent)
- UI refresh: 60 FPS capable
- Memory usage: < 50MB typical
- Cache queries: < 5ms

#### Security
- No credential storage
- Azure SDK official authentication
- SQLite cache in user data directory
- No network communication except Azure API

### Documentation

#### User Documentation
- README.md: Quick start and overview
- docs/quickstart.md: Getting started guide
- docs/KEYBOARD_SHORTCUTS.md: Complete keyboard reference
- docs/AZURE_AUTHENTICATION.md: Authentication setup

#### Developer Documentation
- docs/ARCHITECTURE.md: System architecture
- docs/BUILDING.md: Build instructions
- docs/TUI_ARCHITECTURE.md: UI component design
- docs/DATA_LAYER.md: Cache implementation
- docs/NAVIGATION_IMPLEMENTATION_GUIDE.md: Navigation system

#### Contributing
- CONTRIBUTING.md: Contribution guidelines
- LICENSE-MIT: MIT license
- LICENSE-APACHE: Apache 2.0 license

### Known Limitations

- No resource modification (read-only)
- No log viewing (planned for future)
- No multi-pane support (planned for future)
- No command palette (planned for future)
- No custom keybindings (planned for future)
- No theme customization (planned for future)

### Platform Support

- ✅ Linux (tested on Ubuntu, Fedora)
- ✅ macOS (tested on macOS 12+)
- ✅ Windows (tested on Windows 10/11)

### Terminal Compatibility

- ✅ GNOME Terminal
- ✅ Konsole
- ✅ Alacritty
- ✅ iTerm2
- ✅ Windows Terminal
- ⚠️ CMD (limited key support)
- ⚠️ PowerShell (limited key support)

## Development History

### Iteration 1-2: Foundation
- Project setup and workspace structure
- Core architecture design
- Dependency selection

### Iteration 3-4: Azure Integration
- Azure authentication implementation
- Subscription loading
- Resource group and resource listing

### Iteration 5-6: Cache Layer
- SQLite cache implementation
- Migration system
- Repository pattern
- Cache warming strategies

### Iteration 7-8: UI Implementation
- Ratatui integration
- List views for subscriptions, resource groups, resources
- Keyboard navigation
- Status bar and breadcrumb

### Iteration 9: Navigation & UX
- Filter mode implementation
- Help system
- Error handling improvements
- Navigation stack

### Iteration 10: Review & Polish
- Comprehensive testing
- Documentation completion
- Code quality improvements
- License and contribution guidelines

## Upgrade Guide

### From Development to 0.1.0

This is the initial release. No upgrade needed.

## Contributors

- aztui contributors

## Acknowledgments

- Inspired by [k9s](https://k9scli.io/) - Kubernetes CLI
- Built with [Ratatui](https://ratatui.rs/) - Terminal UI framework
- Powered by [Azure SDK for Rust](https://github.com/Azure/azure-sdk-for-rust)

---

[Unreleased]: https://github.com/aztui/aztui/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/aztui/aztui/releases/tag/v0.1.0
