set working-directory := "codex-rs"
set positional-arguments

# Display help
help:
    just -l

codex *args:
    cargo run --bin codex -- "$@"

alias c := codex

exec *args:
    cargo run --bin codex -- exec "$@"

# `codex tui`
tui *args:
    cargo run --bin codex -- tui "$@"

# Run the CLI version of the file-search crate.
file-search *args:
    cargo run --bin codex-file-search -- "$@"

# Build the CLI and run the app-server test client
app-server-test-client *args:
    cargo build -p codex-cli
    cargo run -p codex-app-server-test-client -- --codex-bin ./target/debug/codex "$@"

# format code
fmt:
    cargo fmt -- --config imports_granularity=Item

# Fix lint issues with clippy (writes changes).
fix *args:
    cargo clippy --fix --all-features --tests --allow-dirty "$@"

# Run clippy (all features + tests).
clippy:
    cargo clippy --all-features --tests "$@"

# Clean Rust workspace in codex-rs/
clean:
    cargo clean

alias cl := clean

# Fetch deps and show active toolchain.
install:
    rustup show active-toolchain
    cargo fetch

# Install dev `codex` into ~/.cargo/bin (overwrites prior dev install) and
# clean the Rust workspace afterwards to keep `target/` from growing.
install-dev:
    cargo install --path cli --bin codex --locked --force
    cargo clean

alias id := install-dev

# Install dev `codex` into ~/.cargo/bin without cleaning.
install-dev-no-clean:
    cargo install --path cli --bin codex --locked --force

alias idnc := install-dev-no-clean

# Run `cargo nextest` since it's faster than `cargo test`, though including
# --no-fail-fast is important to ensure all tests are run.
#
# Run `cargo install cargo-nextest` if you don't have it installed.
test:
    cargo nextest run --no-fail-fast

# Run the MCP server
mcp-server-run *args:
    cargo run -p codex-mcp-server -- "$@"
