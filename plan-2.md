# Notification Enhancements Plan

Goal: extend the notification hook to fire on approval-needed events (configurable) and add an easy, built‑in cross‑platform notifier for approvals and/or turn completion.

## Current Behavior (as implemented)

- [x] **TUI notifications** (OSC 9 / Windows toast) are emitted when the terminal is unfocused.
  - Events: `agent-turn-complete`, `approval-requested`.
  - Controlled by `tui.notifications` (bool or list).
  - Code: `codex-rs/tui/src/chatwidget.rs`, `codex-rs/tui/src/tui.rs`.
- [x] **External notify hook** (core) fires only after a turn completes.
  - Config: top‑level `notify = ["cmd", ...]`.
  - Payload: `type = "agent-turn-complete"` (plus thread/turn/cwd/messages).
  - Code: `codex-rs/core/src/user_notification.rs`, emitted in `codex-rs/core/src/codex.rs`.

## Proposed Config Structure (new)

### Names (locked in)

- [x] `approval_command` → command to run when approval is needed
- [x] `completion_command` → command to run when a turn completes
- [x] `approval_notify` → enable built‑in approval notification
- [x] `completion_notify` → enable built‑in completion notification

### Example config (proposed)

```toml
approval_command = ["python3", "/Users/me/.codex/notify-approval.py"]
completion_command = ["python3", "/Users/me/.codex/notify-complete.py"]
approval_notify = true
completion_notify = true
```

## Behavior Changes

1) [x] **External command hooks**
   - `approval_command` runs on approval-needed events.
   - `completion_command` runs on turn completion.
   - Both receive a JSON payload as the final argv entry.

2) [x] **Built‑in notifier**
   - `approval_notify` and `completion_notify` control whether the built‑in notifier fires.
   - Cross‑platform, no user script required.

3) [x] **No legacy support**
   - Remove the old `notify` config path entirely.
   - Only the new names are supported going forward.

## Payloads

### Completion (unchanged)

- `type = "agent-turn-complete"`
- `thread-id`, `turn-id`, `cwd`, `input-messages`, `last-assistant-message`

### Approval (new)

- `type = "approval-requested"`
- `approval-type`: `exec`, `apply-patch`, or `mcp-elicitation`
- Exec: `command`, `cwd`, optional `reason`
- Patch: `cwd`, `files`, optional `reason`, optional `grant-root`
- Elicitation: `server-name`, `message`

## Built‑In Notifier Implementation

- [x] **macOS**: `osascript -e 'display notification ...'`
- [x] **Linux**: `notify-send` if available; otherwise warn once + skip
- [x] **Windows / WSL**: PowerShell toast (reuse `tui/src/notifications/windows_toast.rs`)

Message text mirrors the TUI’s short previews.

## Wiring Points

- [x] Exec approvals: `request_command_approval` in `codex-rs/core/src/codex.rs`.
- [x] Patch approvals: `request_patch_approval` in `codex-rs/core/src/codex.rs`.
- [x] MCP elicitation: `McpConnectionManager::make_sender` in `codex-rs/core/src/mcp_connection_manager.rs`.
- [x] Pass a notifier into `McpConnectionManager::initialize` to emit approval notifications.

## Docs & Tests

- [x] Update `docs/config.md`, `docs/example-config.md`, `codex-rs/README.md`.
- [x] Extend `core/tests/suite/user_notification.rs` for approval payloads.
- [x] Add config parsing tests for the new top‑level fields.

## Version + Validation

- [x] Bump workspace minor in `codex-rs/Cargo.toml` (0.22.2 → 0.23.0).
- [x] Run `just fmt`, `just fix -p codex-core`, `cargo test -p codex-core`.
- [ ] If core/common/protocol changed, run `cargo test --all-features` (blocked: `cargo test -p codex-core` failed).
- [x] Update snapshots if needed (none needed).

## Open Questions

- For Linux, missing `notify-send` will warn once then skip.
