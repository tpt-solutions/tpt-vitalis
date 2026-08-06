# Vitalis — developer recipes.
# `just` is the task runner (https://github.com/casey/just). If you don't have
# it, each recipe is just the cargo invocation shown in its body.

set shell := ["pwsh", "-NoProfile", "-Command"]

# List available recipes.
default:
    @just --list

# Format the whole workspace.
fmt:
    cargo fmt --all

# Verify formatting (used by CI).
fmt-check:
    cargo fmt --all -- --check

# Lint the whole workspace, denying warnings.
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Run the test suite.
test:
    cargo test --workspace

# Run cargo-deny license + advisory checks.
deny-check:
    cargo deny check

# Build every crate in release mode.
build-all:
    cargo build --workspace --release

# Run everything CI runs locally.
ci: fmt-check clippy test deny-check
    @echo "CI checks passed."
