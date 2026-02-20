# CodexMenuBar

`CodexMenuBar` is a standalone macOS menu bar companion app.

It does not modify or depend on `Codex.app` internals. It discovers embedded app-server Unix domain socket endpoints published by interactive `codex` CLI sessions and renders active turn progress in the menu bar dropdown.

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

When running, the app scans `~/.codex/runtime/menubar/endpoints/*.json` for socket endpoints and listens for:

- `turn/started`
- `turn/completed`
- `turn/progressTrace`

It also uses `item/started` and `item/completed` as a fallback to synthesize trace categories if needed.

Sockets live under `~/.codex/runtime/menubar/sockets/*.sock`. If the backing process exits, the socket disconnects immediately and the menu bar removes stale endpoint records.
