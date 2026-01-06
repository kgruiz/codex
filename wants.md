# Wants List

Add new items below as you think of them.

1. [x] Queued message editing + queue popup + per-item model/thinking + pause behavior (see `plan.md`)
2. [x] Add a keybind (e.g. `Tab`) to change thinking level quickly (<https://github.com/openai/codex/issues/2920>)
   - Allow changing thinking level after typing a prompt without cutting/pasting or leaving the editor.
   - Consider direct shortcuts like `Cmd+1/2/3…` for tiers (or a picker like `Cmd+M`).
3. [x] Add a keybind (e.g. `Tab`) to change model quickly (<https://github.com/openai/codex/issues/2920>)
   - Make model switching possible mid-edit, even after writing a long prompt.
   - Keep the UX comparable to ChatGPT (change any time) rather than requiring slash commands.
4. [x] Export chats as Markdown
5. [x] Add current model name to the status line (<https://github.com/openai/codex/issues/2920>)
   - Show the active model/thinking setting in the status bar so it is obvious what will be used on submit.
6. [x] Syntax highlighting (code blocks)
7. [x] Rename chats
8. [x] Customizable status line + live stats (tokens/sec, latency, cost, tool time) (<https://github.com/openai/codex/issues/2926>)
   - Show tokens/sec (and other live stats like latency, cost, tool time).
   - Show project context like current working directory and git branch in the status line.
   - Consider making the status line user-configurable (similar to Claude Code).
9. [x] Support handing off long prompts to external editor via `Ctrl+G`
10. [x] Edit last message without branching
11. [x] Input shortcuts & editor behavior
    - [x] Paste images from clipboard via `Cmd+V` when clipboard contains an image (toggleable)
    - [x] Use `Shift+Enter` for newline instead of `Option+Enter` (toggleable)
    - [x] Configure any shortcuts (keymap/config file) (<https://github.com/openai/codex/issues/3049>)
      - Add a `[keybindings]` section to `config.toml` mapping actions (newline, backspace, move, etc.) to key chords.
      - Document supported key names/modifiers (`Ctrl`, `Alt`, `Shift`, `Enter`, single chars) and defaults.
    - [x] Configurable submit vs newline behavior (Enter, Ctrl+Enter, Alt/Option+Enter, Shift+Enter) (<https://github.com/openai/codex/issues/5716>, <https://github.com/openai/codex/issues/6100>)
      - Provide a config option to swap defaults (Enter inserts newline, `Ctrl+Enter` submits) for multi-line-first workflows.
      - Consider restoring/making optional the “Alt/Option+Enter submits” behavior mentioned as a regression.
    - [x] Fix prompt editor copy/paste behavior (Ctrl+C/Ctrl+V, etc.) (<https://github.com/openai/codex/issues/5325>)
      - Make common shortcuts non-destructive, and support copying the current prompt text from within the TUI.
      - Ensure paste behaves as expected (instead of clearing prompt content).
    - [x] Support Alt+d delete-word-forward (readline-style) (<https://github.com/openai/codex/issues/5018>)
      - Implement readline `M-d`: delete from cursor to end of current word (word-kill forward).
    - [x] Platform-specific key-hint formatting on macOS (⌃ ⌥ ⇧, Fn+Arrow alternatives) (<https://github.com/openai/codex/issues/4914>)
      - Display macOS-style modifier symbols (⌃ ⌥ ⇧) and optionally omit verbose names for cleaner hints.
      - Include `Fn+Arrow` alternatives for PageUp/PageDown/Home/End on compact keyboards; keep other platforms unchanged.
12. [ ] Render images inline in terminal output when supported (iTerm image protocol, Kitty graphics, etc.) (<https://github.com/openai/codex/issues/6123>)
    - Render pasted images or image links/previews directly in the terminal when the terminal supports it.
    - Fall back gracefully (e.g. show URL/alt text) when inline rendering is unavailable.
13. [x] In-TUI session management (list/switch/rename/delete sessions) (<https://github.com/openai/codex/issues/6500>)
    - Add an interactive `/session` view to list and switch sessions with metadata (name, ID, last activity, directory).
    - Support `/session rename …` and `/session delete …`, plus optional non-TUI subcommands (`codex session …`).
14. [ ] TUI appearance settings (hide startup tips, session header, placeholder title) (<https://github.com/openai/codex/issues/6089>)
    - Add toggles to hide startup tips and session header, and to hide/replace the placeholder title.
    - Support a minimal “prompt + output” startup layout.
15. [x] `/status` should list global `~/.codex/AGENTS.md` / `$CODEX_HOME/AGENTS.md` (<https://github.com/openai/codex/issues/3793>)
    - `/status` should include the global AGENTS file in the “AGENTS files” list when present (expected vs actual).
16. [ ] Add colors/highlighting for emphasis (thinking vs final, headers) (<https://github.com/openai/codex/issues/2711>)
    - Add configurable color settings to distinguish thinking vs final output.
    - Highlight headers/sections like next steps/recommendations for better scanability.
17. [x] Interactive history reverse search (Ctrl+R) (<https://github.com/openai/codex/issues/2622>)
    - `Ctrl+R` enters reverse-i-search with fuzzy/substring matching; repeated `Ctrl+R` cycles matches (`Ctrl+S` forward).
    - Enter accepts, Escape cancels, and editing the selected prompt should be easy (exit search and edit in place).
    - Persist history with configurable size limits and optional frecency ordering.
18. [x] Edit selection navigation
    - Add back and forward actions in the edit selection menu.
    - Fix the keybind for navigating between edited prompts.
19. [x] Show command output while running (scrollable region)
    - Surface live command output in a scrollable area so users can follow along.
20. [x] Add plan/ask modes
    - Add modes that generate a concise implementation plan or answer questions without making changes.
21. [x] Allow configurable output path for `/export`
    - Let users choose the export destination filename/path (via CLI flags, prompt, or config).
22. [x] Allow switching modes via keybind while output is running and while typing a prompt
23. [x] Allow switching models while output is running and while typing a prompt
24. [x] Allow slash commands while typing in the prompt area
25. [ ] Make assistant output easier to copy (toggleable code blocks, no-indent/plain transcript)
26. [x] Add retry/resend options in edit/branch menu (retry same message, resend with different model/thinking)
    - Add a "resend with same model/thinking" action that shows which model/thinking will be reused.
    - Add a "resend with different model/thinking" action that lets users pick overrides before reissuing.
    - Ensure resend reissues the same prompt without editing (no manual copy/paste).
    - Likely touchpoints: backtrack action picker (edit/branch menu), backtrack action handling, model/thinking picker flow, and user-message metadata display.
27. [ ] Allow planning and interacting in editor like Cursor and Antigravity use
28. [ ] Investigate whether background terminal checks are too frequent (possible token usage impact)
29. [x] Fix attached image placeholder (`[<filename> <size>]`) turning into plain text when editing messages (edit flow or queue editor)
30. [ ] Use a model to name chats with configurable model selection and a full off switch to save costs
