## Slash Commands

### What are slash commands?

Slash commands are special commands you can type that start with `/`.

---

### Built-in slash commands

Control Codex’s behavior during an interactive session with slash commands.

| Command         | Purpose                                                                    |
| --------------- | -------------------------------------------------------------------------- |
| `/model`        | choose what model and reasoning effort to use                              |
| `/approvals`    | choose what Codex can do without approval                                  |
| `/plan`         | switch to plan mode (generate a plan only; no edits)                        |
| `/ask`          | switch to ask mode (answer questions; no edits)                             |
| `/normal`       | return to normal mode (full editing behavior)                               |
| `/review`       | review my current changes and find issues                                  |
| `/new`          | start a new chat during a conversation                                     |
| `/resume`       | resume an old chat                                                         |
| `/rename`       | rename the current chat                                                    |
| `/export`       | export the current chat transcript                                         |
| `/init`         | create an AGENTS.md file with instructions for Codex                       |
| `/compact`      | summarize conversation to prevent hitting the context limit                |
| `/diff`         | show git diff (including untracked files)                                  |
| `/mention`      | mention a file                                                             |
| `/status`       | show current session configuration and token usage                         |
| `/mcp`          | list configured MCP tools                                                  |
| `/experimental` | open the experimental menu to enable features from our beta program        |
| `/skills`       | browse and insert skills (experimental; see [docs/skills.md](./skills.md)) |
| `/logout`       | log out of Codex                                                           |
| `/quit`         | exit Codex                                                                 |
| `/exit`         | exit Codex                                                                 |
| `/feedback`     | send logs to maintainers                                                   |

---

Notes:
- `/diff` uses the configured diff view (`tui.diff_view`, or `--diff-view` on launch). Override per command with `--line`, `--inline`, `--side-by-side`, or `--view line|inline|side-by-side`.
- `/export` supports POSIX-style arguments:
  - `/export` opens the picker; defaults come from `tui.export_dir`, `tui.export_name`, and `tui.export_format`.
  - `/export -C DIR` writes to a directory (filename comes from the rollout name or `tui.export_name`).
  - `/export -o PATH` writes to an explicit file path.
  - `/export --name NAME` sets the base filename (no extension).
  - `/export --format md|json` (or `--md`, `--json`) chooses the format.
  - `/export PATH` treats `PATH` as a directory if it ends with `/` or already exists; otherwise it is a file path.
- Plan/ask/normal modes persist until you switch again.
