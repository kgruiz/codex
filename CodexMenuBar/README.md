# CodexMenuBar

`CodexMenuBar` is a standalone macOS menu bar companion app.

It does not modify or depend on `Codex.app` internals. It connects to a single local `codexd` socket and renders authoritative active turn state in the menu bar dropdown.

## Features

- Menu bar icon state for connected/running/error.
- One row per active turn.
- Terminal-style progress semantics:
  - working status
  - elapsed timer
  - trace legend categories
  - indeterminate progress bar while running

## Build

```shell
cd CodexMenuBar
swift build
```

## Run

```shell
cd CodexMenuBar
swift run CodexMenuBar
```

When running, the app connects to `~/.codex/runtime/codexd/codexd.sock` and subscribes to:

- `turn/started`
- `turn/completed`
- `turn/progressTrace`

It also uses `item/started` and `item/completed` as a fallback to synthesize trace categories if needed.

`codexd` receives runtime updates from Codex runtimes and provides:

- `codexd/snapshot` for current state.
- `codexd/event` notifications for live changes.

If the menu bar disconnects, it reconnects and re-fetches snapshot state before resubscribing.
