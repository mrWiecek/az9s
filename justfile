set shell := ["bash", "-cu"]

default:
    @just help

help:
    @echo "Available recipes:"
    @echo "  just build                       # build app (debug)"
    @echo "  just build-release               # build app (release)"
    @echo "  just app-run                     # run app in release mode (recommended)"
    @echo "  just app-run-debug               # run app in debug mode"
    @echo "  just test"
    @echo "  just fmt"
    @echo "  just clippy"
    @echo "  just clean"

build:
    @cd src/apps/aztui && cargo build

build-release:
    @cd src/apps/aztui && cargo build --release

test:
    @cd src/apps/aztui && cargo test

fmt:
    @cd src/apps/aztui && cargo fmt --all

clippy:
    @cd src/apps/aztui && cargo clippy --workspace --all-targets -- -D warnings

# Run the TUI app with release profile and logging disabled for stable rendering.
app-run:
    @cd src/apps/aztui && RUST_LOG=off cargo run --release --quiet

# Run the TUI app in debug mode with logging disabled.
app-run-debug:
    @cd src/apps/aztui && RUST_LOG=off cargo run --quiet

# Remove local build/test artifacts.
clean:
        @cd src/apps/aztui && cargo clean