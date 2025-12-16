# Edit Queue Messages - Plan (No Code Changes Yet)

## Goal

Enable editing queued user messages **in place** (same queue position), without popping them out and re-adding to the back. Add a richer UI to browse/select/reorder/delete queued items.

## Current Baseline (for reference)

- Queued user messages exist as `VecDeque<UserMessage>` during an active turn.
- UI shows a preview list above the composer and a hint `Alt+↑ edit`.
- `Alt+Up` currently pops the newest queued item (`pop_back`) and restores **text only** into the composer, removing it from the queue.
- Turn completion auto-sends exactly one queued item from the **front** (`pop_front`).

## Proposed Model

### Data Model

- Promote queued items to stable entries with identity and full content:
  - `QueuedItem { id, text, image_paths, created_at?, model_override?, thinking_override? }`
- Queue remains `VecDeque<QueuedItem>` to preserve FIFO semantics.
- Add an “editing state” that references an existing queue item by `id`:
  - `editing: Option<{ id, original_snapshot, draft_text, draft_image_paths, cursor_state? }>`

#### Per-item Model / Thinking Overrides

- Each queued item may optionally carry:
  - `model_override`: a chosen model (or model preset) to use when this item is eventually sent.
  - `thinking_override`: a chosen “thinking level” to use when this item is eventually sent (e.g., a discrete level or a reasoning-effort style setting).
- When no override is present, the item uses the session/default model and thinking settings.

### UI Surfaces

#### 1) Inline Queue Edit Mode (Fast Path)

**Purpose:** quick edit of queued items directly in the composer without changing queue order.

- Enter edit mode:
  - `Alt+Up` selects the most-recent queued item (or cycles to previous when already editing).
- Navigate items:
  - `Alt+Up` previous queued item
  - `Alt+Down` next queued item
- Save edit in place:
  - `Enter` updates the selected queued item (same `id`, same queue position) and exits edit mode.
- Cancel:
  - `Esc` discards draft and exits edit mode; queue unchanged.
- Delete (optional):
  - `Alt+Backspace` removes the selected queued item from the queue.
- Set per-item model / thinking (new):
  - `Alt+M` opens model picker for the currently edited queued item (sets `model_override`).
  - `Alt+T` opens thinking-level picker for the currently edited queued item (sets `thinking_override`).

**Hints while editing (example):**

- `Editing 2/5 · Enter save · Esc cancel · Alt+↑/↓ switch · Alt+M model · Alt+T thinking · Alt+Backspace delete`

#### 2) Queue Popup (Richer Manage UI)

**Purpose:** browse and manage the queue (select/edit/reorder/delete) with clear visibility.

- Open:
  - Keybind proposal: `Alt+Q` (or slash command `/queue`).
- Contents:
  - Scrollable list of queued items
  - Each row shows: index, short preview (first line), attachment indicator, and marker for “next to send”.
  - Each row also shows per-item overrides when present (e.g., model label and thinking level).
- Controls:
  - `↑/↓` move selection
  - `Enter` edit selected (either opens inline edit mode, or edits in a dedicated popup editor)
  - `D` delete selected (with confirm)
  - `J/K` (or `Alt+↑/↓`) reorder selected (move up/down in queue)
  - `S` “Send next” (optional): move selected to the front with explicit user intent
  - `M` set model for selected item (toggle/clear override as needed)
  - `T` set thinking level for selected item (toggle/clear override as needed)
  - `Esc` close popup

**Hints in popup (example):**

- `Enter edit · M model · T thinking · D delete · J/K reorder · Esc close`

### Queue Ordering Rules

- Submitting while a task is running appends new items to the back (existing behavior).
- Editing does **not** change ordering:
  - Select item `i` → save → replace item `i` in place.
- Reordering only happens explicitly via queue popup controls.

### Turn Completion / Auto-Send

- Baseline behavior remains: when a turn completes, auto-send **one** queued item from the front.
- To avoid surprising behavior while editing:
  - If inline queue edit mode is active or queue popup is open, **pause auto-send**.
  - Resume auto-send when the user exits edit mode / closes popup.
  - Optional “Send next” action in popup provides explicit control over which item is next.
- When auto-sending (or manual sending) a queued item with overrides:
  - Apply the item’s `model_override` and/or `thinking_override` for that single turn only.
  - After the turn begins, restore the UI/session defaults so subsequent items without overrides behave normally.

### Pause Current Turn (No Queue Flattening)

**Purpose:** allow pausing the current agent turn without moving queued items into the composer.

- Add a “pause streaming/output” toggle while a turn is running (keybind proposal: `Ctrl+P`).
- When paused:
  - Continue keeping queued items in the queue (no flattening into composer).
  - Allow opening the queue popup and editing queued items.
  - Suspend on-screen streaming updates (buffer deltas internally) until unpaused.
  - Suppress turn-complete auto-send until unpaused (same rule as “editing/popup open”).
- When unpaused:
  - Flush buffered output to the UI and restore normal streaming behavior.

### Attachments

- Entering edit mode loads both `text` and `image_paths` from the selected queue item.
- Saving writes both back into the same queue item (no attachment loss).

### Interrupt Behavior

- On user interrupt mid-turn:
  - Keep queued items in the queue (do not flatten them into composer text).
  - Optionally surface a hint to open queue popup (`Alt+Q`) if items exist.

## Open Decisions (Need Confirmation Before Implementation)

- Choose the queue popup entry point: `Alt+Q` vs `/queue` (or both).
- Whether reorder is allowed (and which keys: `J/K` vs `Alt+↑/↓` in popup).
- Whether editing happens in the main composer (preferred) vs inside the popup.
- Whether auto-send should pause during editing/popup (recommended) vs continue.
- Keybind and semantics for “Pause Current Turn”:
  - `Ctrl+P` (toggle) vs another binding, and whether “pause” means pausing UI streaming only (recommended) vs attempting to pause generation.
- How “thinking level” should map to the underlying API:
  - discrete levels (e.g., Low/Medium/High) vs an effort/verbosity-like slider, and which exact values are supported.
- Whether per-item overrides should persist across restarts (likely no; queue is in-memory today), or be ephemeral for the current run only.
