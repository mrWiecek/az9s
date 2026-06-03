# Contributing to aztui

Thank you for your interest in contributing to aztui! This document provides guidelines and instructions for contributing to the project.

## Code of Conduct

This project adheres to a code of conduct that all contributors are expected to follow:

- Be respectful and inclusive
- Welcome newcomers and help them get started
- Focus on constructive feedback
- Assume good intentions

## Getting Started

### Prerequisites

- Rust toolchain (1.75 or later)
- Azure CLI (optional, for testing Azure integration)
- Git

### Development Setup

1. **Clone the repository**
   ```bash
   git clone https://github.com/aztui/aztui.git
   cd aztui
   ```

2. **Build the project**
   ```bash
   cargo build
   ```

3. **Run tests**
   ```bash
   cargo test
   ```

4. **Run the application**
   ```bash
   cargo run
   ```

5. **Enable debug logging**
   ```bash
   RUST_LOG=debug cargo run
   ```

## Project Structure

```
aztui/
├── crates/
│   ├── aztui/           # Main binary
│   ├── aztui-core/      # Core business logic
│   ├── aztui-azure/     # Azure SDK integration
│   └── aztui-tui/       # Terminal UI
├── docs/                # Documentation
├── Cargo.toml           # Workspace configuration
└── README.md
```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
```

Use descriptive branch names:
- `feature/` for new features
- `fix/` for bug fixes
- `docs/` for documentation changes
- `refactor/` for code refactoring

### 2. Make Changes

- Write clean, idiomatic Rust code
- Follow the existing code style
- Add tests for new functionality
- Update documentation as needed

### 3. Test Your Changes

```bash
# Run all tests
cargo test

# Run clippy for linting
cargo clippy -- -D warnings

# Check formatting
cargo fmt -- --check

# Build release version
cargo build --release
```

### 4. Commit Your Changes

Write clear, descriptive commit messages:

```
feat: add resource filtering by location

- Implement location-based filtering in cache repository
- Add UI controls for location filter
- Update tests to cover new functionality

Closes #123
```

Commit message format:
- `feat:` for new features
- `fix:` for bug fixes
- `docs:` for documentation changes
- `test:` for test additions/changes
- `refactor:` for code refactoring
- `perf:` for performance improvements

### 5. Push and Create Pull Request

```bash
git push origin feature/your-feature-name
```

Then create a pull request on GitHub with:
- Clear description of changes
- Reference to related issues
- Screenshots for UI changes
- Test results

## Coding Standards

### Rust Style

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` for formatting
- Use `cargo clippy` for linting
- Avoid `unsafe` code unless absolutely necessary
- Document public APIs with doc comments

### Error Handling

- Use `Result<T>` for fallible operations
- Use `thiserror` for custom error types
- Provide helpful error messages
- Log errors with appropriate levels

### Testing

- Write unit tests for all new functionality
- Add integration tests for complex features
- Test edge cases and error paths
- Use `#[ignore]` for tests requiring external services
- Aim for high test coverage

### Documentation

- Document all public APIs
- Include examples in doc comments
- Update README.md for user-facing changes
- Add architecture docs for significant changes
- Keep CHANGELOG.md up to date

## Crate-Specific Guidelines

### aztui (Binary)

- Keep `main.rs` minimal
- Focus on initialization and orchestration
- Delegate logic to other crates

### aztui-core

- Pure business logic, no UI code
- Comprehensive error handling
- Well-tested with unit tests
- Clear separation from Azure SDK

### aztui-azure

- Thin wrapper around Azure SDK
- Convert Azure types to domain models
- Handle authentication gracefully
- Retry transient failures

### aztui-tui

- Responsive UI (< 16ms frame time)
- Keyboard-first design
- Clear visual hierarchy
- Accessible to all users

## Testing Guidelines

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature() {
        // Arrange
        let input = setup_test_data();
        
        // Act
        let result = function_under_test(input);
        
        // Assert
        assert_eq!(result, expected_value);
    }
}
```

### Integration Tests

```rust
#[tokio::test]
#[ignore] // For tests requiring external services
async fn test_azure_integration() {
    // Test Azure integration
}
```

### Running Tests

```bash
# All tests
cargo test

# Specific crate
cargo test -p aztui-core

# Include ignored tests (requires Azure credentials)
cargo test -- --ignored

# With output
cargo test -- --nocapture
```

## Documentation Guidelines

### Code Documentation

```rust
/// Brief description of the function
///
/// More detailed explanation if needed.
///
/// # Arguments
///
/// * `param` - Description of parameter
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// Description of possible errors
///
/// # Examples
///
/// ```
/// let result = function(param);
/// assert_eq!(result, expected);
/// ```
pub fn function(param: Type) -> Result<ReturnType> {
    // Implementation
}
```

### Architecture Documentation

- Add new docs to `docs/` directory
- Use Markdown format
- Include diagrams where helpful
- Keep docs up to date with code

## Pull Request Process

1. **Ensure all tests pass**
   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo fmt -- --check
   ```

2. **Update documentation**
   - Update README.md if needed
   - Add/update doc comments
   - Update CHANGELOG.md

3. **Create pull request**
   - Clear title and description
   - Reference related issues
   - Include test results
   - Add screenshots for UI changes

4. **Code review**
   - Address reviewer feedback
   - Keep discussion constructive
   - Update PR as needed

5. **Merge**
   - Squash commits if requested
   - Ensure CI passes
   - Maintainer will merge

## Release Process

(For maintainers)

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Create git tag: `git tag -a v0.x.0 -m "Release v0.x.0"`
4. Push tag: `git push origin v0.x.0`
5. Create GitHub release
6. Publish to crates.io (if applicable)

## Getting Help

- **Documentation**: Check `docs/` directory
- **Issues**: Search existing issues on GitHub
- **Discussions**: Start a discussion on GitHub
- **Questions**: Open an issue with `question` label

## Areas for Contribution

### Good First Issues

- Documentation improvements
- Test coverage
- UI polish
- Error message improvements

### Feature Requests

- Command palette
- Resource actions (start/stop VMs)
- Multi-pane support
- Log viewer
- Configuration file support

### Performance

- Cache optimization
- UI rendering performance
- Memory usage reduction
- Startup time improvement

### Accessibility

- Keyboard navigation improvements
- Screen reader support
- High contrast themes
- Configurable keybindings

## License

By contributing to aztui, you agree that your contributions will be licensed under both the MIT License and Apache License 2.0.

## Questions?

If you have questions about contributing, please open an issue with the `question` label or start a discussion on GitHub.

Thank you for contributing to aztui! 🎉
