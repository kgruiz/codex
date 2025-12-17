use crate::key_hint;
use crate::keybindings::Keybindings;
use crate::render::line_utils::prefix_lines;
use crate::status::format_tokens_compact;
use crate::ui_consts::FOOTER_INDENT_COLS;
use codex_protocol::openai_models::ReasoningEffort;
use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

#[derive(Clone, Copy, Debug)]
pub(crate) struct FooterProps<'a> {
    pub(crate) mode: FooterMode,
    pub(crate) esc_backtrack_hint: bool,
    pub(crate) is_task_running: bool,
    pub(crate) context_window_percent: Option<i64>,
    pub(crate) context_window_used_tokens: Option<i64>,
    pub(crate) model: &'a str,
    pub(crate) reasoning_effort: Option<ReasoningEffort>,
    pub(crate) keybindings: &'a Keybindings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FooterMode {
    CtrlCReminder,
    ShortcutSummary,
    ShortcutOverlay,
    EscHint,
    ContextOnly,
}

pub(crate) fn toggle_shortcut_mode(current: FooterMode, ctrl_c_hint: bool) -> FooterMode {
    if ctrl_c_hint && matches!(current, FooterMode::CtrlCReminder) {
        return current;
    }

    match current {
        FooterMode::ShortcutOverlay | FooterMode::CtrlCReminder => FooterMode::ShortcutSummary,
        _ => FooterMode::ShortcutOverlay,
    }
}

pub(crate) fn esc_hint_mode(current: FooterMode, is_task_running: bool) -> FooterMode {
    if is_task_running {
        current
    } else {
        FooterMode::EscHint
    }
}

pub(crate) fn reset_mode_after_activity(current: FooterMode) -> FooterMode {
    match current {
        FooterMode::EscHint
        | FooterMode::ShortcutOverlay
        | FooterMode::CtrlCReminder
        | FooterMode::ContextOnly => FooterMode::ShortcutSummary,
        other => other,
    }
}

pub(crate) fn footer_height(props: FooterProps<'_>) -> u16 {
    footer_lines(props).len() as u16
}

pub(crate) fn render_footer(area: Rect, buf: &mut Buffer, props: FooterProps<'_>) {
    Paragraph::new(prefix_lines(
        footer_lines(props),
        " ".repeat(FOOTER_INDENT_COLS).into(),
        " ".repeat(FOOTER_INDENT_COLS).into(),
    ))
    .render(area, buf);
}

fn footer_lines(props: FooterProps<'_>) -> Vec<Line<'static>> {
    // Show the context indicator on the left, appended after the primary hint
    // (e.g., "? for shortcuts"). Keep it visible even when typing (i.e., when
    // the shortcut hint is hidden). Hide it only for the multi-line
    // ShortcutOverlay.
    match props.mode {
        FooterMode::CtrlCReminder => vec![ctrl_c_reminder_line(CtrlCReminderState {
            is_task_running: props.is_task_running,
        })],
        FooterMode::ShortcutSummary => {
            let mut line = status_line_prefix(props.model, props.reasoning_effort);
            if !line.spans.is_empty() {
                line.push_span(" · ".dim());
            }

            let mut context = context_window_line(
                props.context_window_percent,
                props.context_window_used_tokens,
            );
            line.spans.append(&mut context.spans);
            line.push_span(" · ".dim());
            line.extend(vec![
                key_hint::plain(KeyCode::Char('?')).into(),
                " for shortcuts".dim(),
            ]);
            vec![line]
        }
        FooterMode::ShortcutOverlay => shortcut_overlay_lines(ShortcutsState {
            esc_backtrack_hint: props.esc_backtrack_hint,
            keybindings: props.keybindings,
        }),
        FooterMode::EscHint => vec![esc_hint_line(props.esc_backtrack_hint)],
        FooterMode::ContextOnly => {
            let mut line = status_line_prefix(props.model, props.reasoning_effort);
            if !line.spans.is_empty() {
                line.push_span(" · ".dim());
            }

            let mut context = context_window_line(
                props.context_window_percent,
                props.context_window_used_tokens,
            );
            line.spans.append(&mut context.spans);
            vec![line]
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CtrlCReminderState {
    is_task_running: bool,
}

#[derive(Clone, Copy, Debug)]
struct ShortcutsState<'a> {
    esc_backtrack_hint: bool,
    keybindings: &'a Keybindings,
}

fn ctrl_c_reminder_line(state: CtrlCReminderState) -> Line<'static> {
    let action = if state.is_task_running {
        "interrupt"
    } else {
        "quit"
    };
    Line::from(vec![
        key_hint::ctrl(KeyCode::Char('c')).into(),
        format!(" again to {action}").into(),
    ])
    .dim()
}

fn esc_hint_line(esc_backtrack_hint: bool) -> Line<'static> {
    let esc = key_hint::plain(KeyCode::Esc);
    if esc_backtrack_hint {
        Line::from(vec![esc.into(), " again to edit previous message".into()]).dim()
    } else {
        Line::from(vec![
            esc.into(),
            " ".into(),
            esc.into(),
            " to edit previous message".into(),
        ])
        .dim()
    }
}

fn shortcut_overlay_lines(state: ShortcutsState<'_>) -> Vec<Line<'static>> {
    let newline = state
        .keybindings
        .newline
        .first()
        .copied()
        .unwrap_or_else(|| key_hint::shift(KeyCode::Enter));

    let paste = state
        .keybindings
        .paste
        .first()
        .copied()
        .unwrap_or_else(|| key_hint::ctrl(KeyCode::Char('v')));

    let copy_prompt = state
        .keybindings
        .copy_prompt
        .first()
        .copied()
        .unwrap_or_else(|| key_hint::alt(KeyCode::Char('c')));

    let commands = Line::from(vec![
        key_hint::plain(KeyCode::Char('/')).into(),
        " for commands".into(),
    ]);

    let model = Line::from(vec![
        key_hint::plain(KeyCode::Tab).into(),
        " / ".into(),
        key_hint::shift(KeyCode::Tab).into(),
        " to change model".into(),
    ]);

    let thinking = Line::from(vec![
        key_hint::ctrl(KeyCode::Char('[')).into(),
        " / ".into(),
        key_hint::ctrl(KeyCode::Char(']')).into(),
        " to change thinking".into(),
    ]);

    let file_paths = Line::from(vec![
        key_hint::plain(KeyCode::Char('@')).into(),
        " for file paths".into(),
    ]);

    let edit_previous = if state.esc_backtrack_hint {
        Line::from(vec![
            key_hint::plain(KeyCode::Esc).into(),
            " again to edit previous message".into(),
        ])
    } else {
        Line::from(vec![
            key_hint::plain(KeyCode::Esc).into(),
            " ".into(),
            key_hint::plain(KeyCode::Esc).into(),
            " to edit previous message".into(),
        ])
    };

    let quit = Line::from(vec![
        key_hint::ctrl(KeyCode::Char('c')).into(),
        " to exit".into(),
    ]);

    let show_transcript = Line::from(vec![
        key_hint::ctrl(KeyCode::Char('t')).into(),
        " to view transcript".into(),
    ]);

    let ordered = vec![
        commands,
        Line::from(vec![newline.into(), " for newline".into()]),
        model,
        thinking,
        file_paths,
        Line::from(vec![paste.into(), " to paste from clipboard".into()]),
        Line::from(vec![copy_prompt.into(), " to copy prompt".into()]),
        edit_previous,
        quit,
        Line::from(""),
        Line::from(""),
        show_transcript,
    ];

    build_columns(ordered)
}

fn build_columns(entries: Vec<Line<'static>>) -> Vec<Line<'static>> {
    if entries.is_empty() {
        return Vec::new();
    }

    const COLUMNS: usize = 2;
    const COLUMN_PADDING: [usize; COLUMNS] = [4, 4];
    const COLUMN_GAP: usize = 4;

    let rows = entries.len().div_ceil(COLUMNS);
    let target_len = rows * COLUMNS;
    let mut entries = entries;
    if entries.len() < target_len {
        entries.extend(std::iter::repeat_n(
            Line::from(""),
            target_len - entries.len(),
        ));
    }

    let mut column_widths = [0usize; COLUMNS];

    for (idx, entry) in entries.iter().enumerate() {
        let column = idx % COLUMNS;
        column_widths[column] = column_widths[column].max(entry.width());
    }

    for (idx, width) in column_widths.iter_mut().enumerate() {
        *width += COLUMN_PADDING[idx];
    }

    entries
        .chunks(COLUMNS)
        .map(|chunk| {
            let mut line = Line::from("");
            for (col, entry) in chunk.iter().enumerate() {
                line.extend(entry.spans.clone());
                if col < COLUMNS - 1 {
                    let target_width = column_widths[col];
                    let padding = target_width.saturating_sub(entry.width()) + COLUMN_GAP;
                    line.push_span(Span::from(" ".repeat(padding)));
                }
            }
            line.dim()
        })
        .collect()
}

fn context_window_line(percent: Option<i64>, used_tokens: Option<i64>) -> Line<'static> {
    if let Some(percent) = percent {
        let percent = percent.clamp(0, 100);
        return Line::from(vec![Span::from(format!("{percent}% context left")).dim()]);
    }

    if let Some(tokens) = used_tokens {
        let used_fmt = format_tokens_compact(tokens);
        return Line::from(vec![Span::from(format!("{used_fmt} used")).dim()]);
    }

    Line::from(vec![Span::from("100% context left").dim()])
}

fn status_line_prefix(model: &str, effort: Option<ReasoningEffort>) -> Line<'static> {
    if model.trim().is_empty() {
        return Line::from("");
    }

    let mut line = Line::default();
    push_model_segment(&mut line, "Model", model, effort);
    line
}

fn push_model_segment(
    line: &mut Line<'static>,
    label: &'static str,
    model: &str,
    effort: Option<ReasoningEffort>,
) {
    line.push_span(format!("{label}: ").dim());
    line.push_span(model.to_string());

    if let Some(label) = thinking_label_for(model, effort) {
        line.push_span(format!(" (think {label})").dim());
    }
}

fn thinking_label_for(model: &str, effort: Option<ReasoningEffort>) -> Option<&'static str> {
    if model.starts_with("codex-auto-") {
        return None;
    }

    effort.map(thinking_label)
}

fn thinking_label(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::Minimal => "minimal",
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::XHigh => "xhigh",
        ReasoningEffort::None => "none",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::collections::HashMap;

    fn snapshot_footer(name: &str, props: FooterProps<'_>) {
        let height = footer_height(props).max(1);
        let mut terminal = Terminal::new(TestBackend::new(80, height)).unwrap();
        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, f.area().width, height);
                render_footer(area, f.buffer_mut(), props);
            })
            .unwrap();
        assert_snapshot!(name, terminal.backend());
    }

    #[test]
    fn footer_snapshots() {
        let empty: HashMap<String, Vec<String>> = HashMap::new();
        let keybindings_default = Keybindings::from_config(&empty, false, false);
        let keybindings_shift = Keybindings::from_config(&empty, true, false);

        snapshot_footer(
            "footer_shortcuts_default",
            FooterProps {
                mode: FooterMode::ShortcutSummary,
                esc_backtrack_hint: false,
                is_task_running: false,
                context_window_percent: None,
                context_window_used_tokens: None,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
            },
        );

        snapshot_footer(
            "footer_shortcuts_shift_and_esc",
            FooterProps {
                mode: FooterMode::ShortcutOverlay,
                esc_backtrack_hint: true,
                is_task_running: false,
                context_window_percent: None,
                context_window_used_tokens: None,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_shift,
            },
        );

        snapshot_footer(
            "footer_ctrl_c_quit_idle",
            FooterProps {
                mode: FooterMode::CtrlCReminder,
                esc_backtrack_hint: false,
                is_task_running: false,
                context_window_percent: None,
                context_window_used_tokens: None,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
            },
        );

        snapshot_footer(
            "footer_ctrl_c_quit_running",
            FooterProps {
                mode: FooterMode::CtrlCReminder,
                esc_backtrack_hint: false,
                is_task_running: true,
                context_window_percent: None,
                context_window_used_tokens: None,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
            },
        );

        snapshot_footer(
            "footer_esc_hint_idle",
            FooterProps {
                mode: FooterMode::EscHint,
                esc_backtrack_hint: false,
                is_task_running: false,
                context_window_percent: None,
                context_window_used_tokens: None,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
            },
        );

        snapshot_footer(
            "footer_esc_hint_primed",
            FooterProps {
                mode: FooterMode::EscHint,
                esc_backtrack_hint: true,
                is_task_running: false,
                context_window_percent: None,
                context_window_used_tokens: None,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
            },
        );

        snapshot_footer(
            "footer_shortcuts_context_running",
            FooterProps {
                mode: FooterMode::ShortcutSummary,
                esc_backtrack_hint: false,
                is_task_running: true,
                context_window_percent: Some(72),
                context_window_used_tokens: None,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
            },
        );

        snapshot_footer(
            "footer_context_tokens_used",
            FooterProps {
                mode: FooterMode::ShortcutSummary,
                esc_backtrack_hint: false,
                is_task_running: false,
                context_window_percent: None,
                context_window_used_tokens: Some(123_456),
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
            },
        );

        snapshot_footer(
            "footer_shortcuts_with_model",
            FooterProps {
                mode: FooterMode::ShortcutSummary,
                esc_backtrack_hint: false,
                is_task_running: false,
                context_window_percent: None,
                context_window_used_tokens: None,
                model: "gpt-5.1-codex",
                reasoning_effort: Some(ReasoningEffort::Medium),
                keybindings: &keybindings_default,
            },
        );
    }
}
