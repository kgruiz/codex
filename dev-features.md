# Dev Feature Inventory

## Queue editing and queued message UI
- Commits: 2bf5c4733, 120e36e45, 9d6d308e8, 761caf450, 02b0163e1, e6b69d90c, 5fa379366, 632a3ce2b, 1bd2e7af4
- Code anchors:
  - codex-rs/tui/src/bottom_pane/queued_user_messages.rs:14
  - codex-rs/tui/src/bottom_pane/queue_popup.rs:31
  - codex-rs/tui/src/bottom_pane/mod.rs:105

## Reverse history search (Ctrl+R)
- Commit: 58158ef3d
- Code anchors:
  - codex-rs/tui/src/bottom_pane/chat_composer.rs:140
  - codex-rs/tui/src/bottom_pane/chat_composer.rs:762

## Chat export (/export) with defaults and path prompt
- Commits: 29a60f56a, 2d494d3c3, be057fd3e
- Code anchors:
  - codex-rs/tui/src/chatwidget.rs:3421
  - codex-rs/tui/src/chatwidget.rs:3724
  - codex-rs/tui/src/export_markdown.rs:15

## Session manager and chat renaming
- Commits: 720cb8287, 6c05b8b94, 54791d145, b94fd60fd
- Code anchors:
  - codex-rs/tui/src/sessions_picker.rs:118
  - codex-rs/tui/src/app.rs:701

## Backtrack edit and resend flow (Esc, Shift+Esc, etc.)
- Commits: dca034e09, cb6493efb, c75e09adc, 0c1f89e54, 16d8d36e3, 9f7f8dfba
- Code anchors:
  - codex-rs/tui/src/app_backtrack.rs:69
  - codex-rs/tui/src/app_backtrack.rs:219
  - codex-rs/tui/src/chatwidget.rs:3969

## Diff view and pretty diff rendering
- Commits: 985dbe747, 515b8c8e5, 18b770ee2, 356ef95e6, f7e6da496, 59fa25682, 0759101ba
- Code anchors:
  - codex-rs/tui/src/diff_render.rs:110
  - codex-rs/tui/src/chatwidget.rs:6736
  - codex-rs/core/src/config/mod.rs:196

## Status line stats and model info
- Commits: 8ce97dfab, 3a53f2668
- Code anchors:
  - codex-rs/core/src/config/mod.rs:190
  - codex-rs/tui/src/bottom_pane/footer.rs:377
  - codex-rs/tui/src/bottom_pane/chat_composer.rs:195

## Configurable keybindings
- Commits: 8eddbc636, 7a6886313
- Code anchors:
  - codex-rs/core/src/config/mod.rs:207
  - codex-rs/tui/src/keybindings.rs:33
  - codex-rs/tui/src/bottom_pane/chat_composer.rs:2928

## Notification focus filtering
- Commits: eb808106a, 7cc6afd5d, 6f8d51022
- Code anchors:
  - codex-rs/core/src/config/mod.rs:177
  - codex-rs/tui/src/chatwidget.rs:6279

## External editor handoff
- Commit: c8a9cf37e
- Code anchors:
  - codex-rs/tui/src/app.rs:1350

## Live exec output in TUI
- Commit: 29b75f9fc
- Likely anchors:
  - codex-rs/tui/src/exec_cell/
  - codex-rs/tui/src/history_cell.rs
