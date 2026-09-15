# Show available recipes
[private]
default:
    @just --list

# Static checks CI runs: fmt, clippy for the server and for the wasm extension
lint:
    cargo fmt --all --check
    cargo clippy -p janet-zed-server --all-targets -- -D warnings
    cargo clippy -p janet-zed --target wasm32-wasip2 -- -D warnings

# Format everything
fmt:
    cargo fmt --all

# Run the server test suite
test:
    cargo test -p janet-zed-server

# Build the Zed extension (Zed builds it too on `zed: install dev extension`)
build:
    cargo build -p janet-zed --target wasm32-wasip2 --release

# Install janet-zed-server into ~/.cargo/bin, where the extension finds it
install:
    cargo install --path server --locked

# Everything CI runs
check: lint test
