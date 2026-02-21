set working-directory := "codex-rs"
set positional-arguments

# Display help
help:
    just -l

# `codex`
alias c := codex
codex *args:
    cargo run --bin codex -- "$@"

# `codex` with app-server debug logs enabled by default.
codex-debug *args:
    RUST_LOG="${RUST_LOG:-codex_core=info,codex_tui=info,codex_rmcp_client=info,codex_app_server=debug}" cargo run --bin codex -- "$@"

# `codex exec`
exec *args:
    cargo run --bin codex -- exec "$@"

# Run the CLI version of the file-search crate.
file-search *args:
    cargo run --bin codex-file-search -- "$@"

# Build the CLI and run the app-server test client
app-server-test-client *args:
    cargo build -p codex-cli
    cargo run -p codex-app-server-test-client -- --codex-bin ./target/debug/codex "$@"

# format code
fmt:
    cargo fmt -- --config imports_granularity=Item 2>/dev/null

fix *args:
    cargo clippy --fix --all-features --tests --allow-dirty "$@"

clippy:
    cargo clippy --all-features --tests "$@"

install:
    rustup show active-toolchain
    cargo fetch

clean:
    cargo clean

# Install dev `codex` into ~/.cargo/bin (overwrites prior dev install) and
# clean the Rust workspace afterwards to keep `target/` from growing.
install-dev:
    cargo install --path cli --bin codex --locked --force
    just clean

alias id := install-dev

# Install dev `codex` into ~/.cargo/bin without cleaning.
install-dev-no-clean:
    cargo install --path cli --bin codex --locked --force

# Run `cargo nextest` since it's faster than `cargo test`, though including
# --no-fail-fast is important to ensure all tests are run.
#
# Run `cargo install cargo-nextest` if you don't have it installed.
test:
    cargo nextest run --no-fail-fast

# Build and run Codex from source using Bazel.
# Note we have to use the combination of `[no-cd]` and `--run_under="cd $PWD &&"`
# to ensure that Bazel runs the command in the current working directory.
[no-cd]
bazel-codex *args:
    bazel run //codex-rs/cli:codex --run_under="cd $PWD &&" -- "$@"

bazel-test:
    bazel test //... --keep_going

bazel-remote-test:
    bazel test //... --config=remote --platforms=//:rbe --keep_going

build-for-release:
    bazel build //codex-rs/cli:release_binaries --config=remote

# Run the MCP server
mcp-server-run *args:
    cargo run -p codex-mcp-server -- "$@"

[no-cd]
codex-menubar-build:
    if ! command -v swift >/dev/null 2>&1; then echo "swift is required"; exit 1; fi
    cd CodexMenuBar && swift build

[no-cd]
codex-menubar-run:
    if ! command -v swift >/dev/null 2>&1; then echo "swift is required"; exit 1; fi
    cd CodexMenuBar && swift run CodexMenuBar

[no-cd]
codex-menubar-install:
    just codex-menubar-build
    APP_DIR="$HOME/Applications/CodexMenuBar.app"
    BIN_SRC="$PWD/CodexMenuBar/.build/debug/CodexMenuBar"
    BIN_DST="$APP_DIR/Contents/MacOS/CodexMenuBar"
    PLIST="$APP_DIR/Contents/Info.plist"
    mkdir -p "$APP_DIR/Contents/MacOS"
    cp "$BIN_SRC" "$BIN_DST"
    chmod +x "$BIN_DST"
    printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>' \
        '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
        '<plist version="1.0">' \
        '<dict>' \
        '  <key>CFBundleIdentifier</key><string>com.openai.codex.menubar</string>' \
        '  <key>CFBundleName</key><string>CodexMenuBar</string>' \
        '  <key>CFBundleExecutable</key><string>CodexMenuBar</string>' \
        '  <key>CFBundlePackageType</key><string>APPL</string>' \
        '  <key>LSUIElement</key><true/>' \
        '</dict>' \
        '</plist>' > "$PLIST"
    open "$APP_DIR"

# Install and bootstrap the local codexd launch agent used by CodexMenuBar.
codexd-install-launch-agent:
    cargo run --bin codex -- app-server codexd install-launch-agent

# Show local codexd launch agent status.
codexd-status:
    cargo run --bin codex -- app-server codexd status

# Regenerate the json schema for config.toml from the current config types.
write-config-schema:
    cargo run -p codex-core --bin codex-write-config-schema

# Regenerate vendored app-server protocol schema artifacts.
write-app-server-schema *args:
    cargo run -p codex-app-server-protocol --bin write_schema_fixtures -- "$@"

# Sync local and origin main to upstream/main.
sync-upstream-main:
    git fetch upstream
    git switch main
    git branch --set-upstream-to=upstream/main
    git reset --hard upstream/main
    git push --force-with-lease origin main

alias sync := sync-upstream-main

# Tail logs from the state SQLite database
log *args:
    if [ "${1:-}" = "--" ]; then shift; fi; cargo run -p codex-state --bin logs_client -- "$@"
