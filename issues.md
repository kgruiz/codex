- https://github.com/openai/codex/issues/6123
  - Title: Show images in the terminal directly with iTerm Image Protocol
  - Labels: `enhancement`, `TUI`
  - Description: Render pasted images or image links directly in the terminal (or show an inline preview) instead of only printing URLs, using terminal image protocols when supported and falling back gracefully when not.

- https://github.com/openai/codex/issues/6500
  - Title: Add interactive session-management for Codex CLI
  - Labels: `enhancement`, `TUI`, `CLI`
  - Description: Add first-class session management inside the interactive TUI (and optionally via `codex session …` subcommands) to list/switch sessions without leaving the UI, plus rename/delete sessions and show useful metadata like last activity and project/directory context.

- https://github.com/openai/codex/issues/6100
  - Title: enter and alt+enter prompt sending
  - Labels: `enhancement`, `TUI`
  - Description: Restore or make configurable the “submit vs newline” keybinding behavior (report claims a recent change swapped behavior), so users can choose whether Enter submits and Alt+Enter inserts a newline (or vice versa).

- https://github.com/openai/codex/issues/6089
  - Title: TUI apperance configuration to hide startup tips, session header and replace/hide placeholder title
  - Labels: `enhancement`, `TUI`
  - Description: Add appearance/config options to reduce TUI chrome on startup (hide tips, hide/replace the session header and placeholder title) so the UI can start in a more minimal “just the prompt and output” layout; includes before/after screenshots and mentions prior PR discussion.

- https://github.com/openai/codex/issues/5716
  - Title: Config option that swaps the default Enter key behavior: Enter inserts newline, Ctrl+Enter submits input
  - Labels: `enhancement`, `TUI`
  - Description: Add a config flag to invert the default submit/newline behavior to reduce accidental submissions in multi-line workflows: Enter inserts a newline and Ctrl+Enter submits (instead of Enter submit and Ctrl+Enter newline).

- https://github.com/openai/codex/issues/5325
  - Title: Codex CLI Ctrl + C and Ctrl + V Copy and Paste
  - Labels: `enhancement`, `TUI`
  - Description: Make common terminal editing keys behave more like users expect for prompt text, especially copy/paste. Report says Ctrl+A/C/V currently clears the prompt and does not copy/paste; request is to support copying the current prompt text (and pasting) rather than destructive behavior.

- https://github.com/openai/codex/issues/5018
  - Title: Alt+d should delete word after cursor
  - Labels: `enhancement`, `TUI`
  - Description: Implement the standard readline “M-d” behavior: Alt+d deletes from the cursor to the end of the current word (word-kill forward), matching common terminal/editor shortcuts.

- https://github.com/openai/codex/issues/4914
  - Title: Platform-specific key-hint formatting for TUI
  - Labels: `enhancement`, `TUI`
  - Description: Improve how keyboard shortcuts are displayed in the TUI by using platform conventions. Proposal focuses on macOS: show modifier symbols (⌃ ⌥ ⇧) and document Fn+Arrow alternatives for PageUp/PageDown/Home/End on compact keyboards, while keeping Windows/Linux behavior unchanged.

- https://github.com/openai/codex/issues/3793
  - Title: /status doesn't mention global AGENTS.md
  - Labels: `enhancement`, `TUI`
  - Description: Bug/UX gap in `/status`: when a global `~/.codex/AGENTS.md` (or `$CODEX_HOME/AGENTS.md`) exists, `/status` should report it under “AGENTS files”, but currently reports “(none)”.

- https://github.com/openai/codex/issues/3049
  - Title: Configurable Hotkeys Support
  - Labels: `enhancement`, `TUI`
  - Description: Add user-configurable keybindings (instead of hardcoded Ctrl+… bindings) via `config.toml`, e.g. a `[keybindings]` section that maps actions like newline/backspace/movement to key chords, with clear docs for supported key names and modifiers.

- https://github.com/openai/codex/issues/2926
  - Title: Allow customizing the status line
  - Labels: `enhancement`, `TUI`
  - Description: Make the status line customizable and/or include common context by default (current working directory, git branch). Motivation is reducing context loss when switching terminals and matching the “custom status area” UX in similar tools.

- https://github.com/openai/codex/issues/2920
  - Title: Change model/thinking through shortcut
  - Labels: `enhancement`, `TUI`
  - Description: Provide a faster way to change model/thinking level after typing a prompt (since `/model` is cumbersome mid-edit). Suggested UX: a shortcut like Cmd+1/2/3… with the current model shown in the status bar, or a Cmd+M picker menu.

- https://github.com/openai/codex/issues/2711
  - Title: Add colors for emphasis and highlighting
  - Labels: `enhancement`, `TUI`
  - Description: Add configurable color/highlighting for the TUI output, e.g. visually distinguishing thinking vs final output and emphasizing headers/sections like “next steps” to improve scanability.

- https://github.com/openai/codex/issues/2622
  - Title: interactive history search
  - Labels: `enhancement`, `TUI`
  - Description: Add an interactive reverse history search (Ctrl+R) for prompt history, similar to bash/zsh, including fuzzy/substr matching, cycling through matches with repeated Ctrl+R, Enter to accept, Esc to cancel, and persistent history storage with optional size limits/frecency.
