# Show available recipes
[private]
default:
    @just --list

# Static checks CI runs: fmt, clippy for the native crates and for the wasm extension
lint:
    cargo fmt --all --check
    cargo clippy -p janet-check -p janet-lsp-plus --all-targets -- -D warnings
    cargo clippy -p janet-zed --target wasm32-wasip2 -- -D warnings

# Format everything
fmt:
    cargo fmt --all

# Run the test suites
test:
    cargo test --no-fail-fast -p janet-check -p janet-lsp-plus

# The speed tests in the build users run: the debug budgets are loose, the release ones are the plan's
speed:
    cargo test --release -p janet-check -- inferred_in_milliseconds costs_no_more_than_its_lines checked_in_seconds

# Build the Zed extension (Zed builds it too on `zed: install dev extension`)
build:
    cargo build -p janet-zed --target wasm32-wasip2 --release

# Install both binaries into ~/.cargo/bin, where the extension finds the server
install:
    cargo install --path janet-check --locked
    cargo install --path janet-lsp-plus --locked

# Everything CI runs
check: lint test speed
