set shell := ["bash", "-cu"]

default:
    @just --list

# Build (debug)
build:
    cargo build

# Build (release)
build-release:
    cargo build --release

# Run the app (release profile, logging off for clean rendering)
run:
    RUST_LOG=off cargo run --release --quiet

# Run the app with debug logging to az9s.log
run-debug:
    RUST_LOG=debug cargo run 2> az9s.log

# Run all tests
test:
    cargo test --workspace

# Format all code
fmt:
    cargo fmt --all

# Lint (warnings are errors)
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Everything CI checks
ci:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

# Install the az9s binary from source
install:
    cargo install --path crates/az9s --locked

# Remove build artifacts
clean:
    cargo clean
