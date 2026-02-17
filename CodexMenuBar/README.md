# CodexMenuBar

`CodexMenuBar` is a standalone macOS menu bar companion app.

It does not modify or depend on `Codex.app` internals. It discovers embedded app-server websocket endpoints published by interactive `codex` CLI sessions and renders active turn progress in the menu bar dropdown.

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

When running, the app scans `~/.codex/runtime/menubar/endpoints/*.json` for websocket endpoints and listens for:

- `turn/started`
- `turn/completed`
- `turn/progressTrace`

It also uses `item/started` and `item/completed` as a fallback to synthesize trace categories if needed.

Endpoint files are lease-based. Active `codex` sessions refresh `lastHeartbeatAt` periodically, and the menu bar prunes stale endpoint files (dead PID or expired lease) to prevent unbounded growth.
