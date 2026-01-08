use crate::exec_command::relativize_to_home;
use crate::key_hint;
use crate::key_hint::KeyBinding;
use crate::keybindings::Keybindings;
use crate::render::line_utils::prefix_lines;
use crate::status::format_tokens_compact;
use crate::ui_consts::FOOTER_INDENT_COLS;
use codex_common::elapsed::format_duration;
use codex_core::config::StatusLineItem;
use codex_core::protocol::SessionMode;
use codex_protocol::openai_models::ReasoningEffort;
use crossterm::event::KeyCode;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use std::time::Duration;
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub(crate) struct FooterProps<'a> {
    pub(crate) mode: FooterMode,
    pub(crate) esc_backtrack_hint: bool,
    pub(crate) is_task_running: bool,
    pub(crate) context_window_percent: Option<i64>,
    pub(crate) context_window_used_tokens: Option<i64>,
    pub(crate) session_mode: SessionMode,
    pub(crate) model: &'a str,
    pub(crate) reasoning_effort: Option<ReasoningEffort>,
    pub(crate) keybindings: &'a Keybindings,
    pub(crate) status_line_items: &'a [StatusLineItem],
    pub(crate) status_line_cwd: Option<&'a std::path::Path>,
    pub(crate) status_line_git_branch: Option<&'a str>,
    pub(crate) status_line_metrics: &'a StatusLineMetrics,
    pub(crate) status_line_notice: Option<&'a StatusLineNotice>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct StatusLineMetrics {
    pub(crate) tokens_per_sec: Option<f64>,
    pub(crate) latency: Option<std::time::Duration>,
    pub(crate) tool_time: Option<std::time::Duration>,
    pub(crate) cost: Option<f64>,
}

#[derive(Clone, Debug)]
pub(crate) struct StatusLineNotice {
    message: String,
    expires_at: Instant,
}

impl StatusLineNotice {
    pub(crate) fn new(message: String, duration: Duration) -> Self {
        Self {
            message,
            expires_at: Instant::now() + duration,
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        Instant::now() < self.expires_at
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
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
            let mut line = status_line_line(props);
            if !line.spans.is_empty() {
                line.push_span(" · ".dim());
            }
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
            vec![status_line_line(props)]
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
        Line::from(vec![
            esc.into(),
            " again to edit or branch previous message".into(),
        ])
        .dim()
    } else {
        Line::from(vec![
            esc.into(),
            " ".into(),
            esc.into(),
            " to edit or branch previous message".into(),
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

    let history_search = Line::from(vec![
        key_hint::ctrl(KeyCode::Char('r')).into(),
        " to search history".into(),
    ]);

    let external_editor = Line::from(vec![
        key_hint::ctrl(KeyCode::Char('g')).into(),
        " to edit in external editor".into(),
    ]);

    let commands = Line::from(vec![
        key_hint::plain(KeyCode::Char('/')).into(),
        " for commands".into(),
    ]);

    let model = Line::from(vec![
        KeyBinding::new(
            KeyCode::Left,
            KeyModifiers::CONTROL.union(KeyModifiers::SHIFT),
        )
        .into(),
        " / ".into(),
        KeyBinding::new(
            KeyCode::Right,
            KeyModifiers::CONTROL.union(KeyModifiers::SHIFT),
        )
        .into(),
        " to change model".into(),
    ]);

    let thinking = Line::from(vec![
        key_hint::plain(KeyCode::Tab).into(),
        " / ".into(),
        key_hint::shift(KeyCode::Tab).into(),
        " to change thinking".into(),
    ]);

    let mode = Line::from(vec![
        key_hint::alt(KeyCode::Char('p')).into(),
        " / ".into(),
        key_hint::alt(KeyCode::Char('a')).into(),
        " / ".into(),
        key_hint::alt(KeyCode::Char('n')).into(),
        " to switch mode".into(),
    ]);

    let file_paths = Line::from(vec![
        key_hint::plain(KeyCode::Char('@')).into(),
        " for file paths".into(),
    ]);

    let edit_previous = if state.esc_backtrack_hint {
        Line::from(vec![
            key_hint::plain(KeyCode::Esc).into(),
            " again to edit or branch previous message".into(),
        ])
    } else {
        Line::from(vec![
            key_hint::plain(KeyCode::Esc).into(),
            " ".into(),
            key_hint::plain(KeyCode::Esc).into(),
            " to edit or branch previous message".into(),
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
        mode,
        file_paths,
        Line::from(vec![paste.into(), " to paste from clipboard".into()]),
        Line::from(vec![copy_prompt.into(), " to copy prompt".into()]),
        external_editor,
        history_search,
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

fn status_line_line(props: FooterProps<'_>) -> Line<'static> {
    if let Some(notice) = props.status_line_notice {
        return Line::from(vec![notice.message().to_string().green()]);
    }

    let mut line = Line::default();
    let mut push_segment = |mut segment: Line<'static>| {
        if !line.spans.is_empty() {
            line.push_span(" · ".dim());
        }

        line.spans.append(&mut segment.spans);
    };

    if let Some(segment) = mode_segment(props.session_mode) {
        push_segment(segment);
    }

    for item in props.status_line_items {
        let segment = match item {
            StatusLineItem::Model => {
                if props.model.trim().is_empty() {
                    None
                } else {
                    let mut seg = Line::default();
                    push_model_segment(&mut seg, "", props.model, props.reasoning_effort);
                    (!seg.spans.is_empty()).then_some(seg)
                }
            }
            StatusLineItem::Context => Some(context_window_line(
                props.context_window_percent,
                props.context_window_used_tokens,
            )),
            StatusLineItem::Cwd => props.status_line_cwd.map(format_cwd_segment),
            StatusLineItem::GitBranch => props
                .status_line_git_branch
                .filter(|branch| !branch.trim().is_empty())
                .map(|branch| Line::from(vec![Span::from(format!("git {branch}")).dim()])),
            StatusLineItem::TokensPerSec => props.status_line_metrics.tokens_per_sec.map(|rate| {
                Line::from(vec![
                    Span::from(format!("{} tok/s", format_tokens_per_sec(rate))).dim(),
                ])
            }),
            StatusLineItem::Latency => props.status_line_metrics.latency.map(|latency| {
                Line::from(vec![
                    Span::from(format!("latency {}", format_duration(latency))).dim(),
                ])
            }),
            StatusLineItem::ToolTime => props.status_line_metrics.tool_time.map(|duration| {
                Line::from(vec![
                    Span::from(format!("tool {}", format_duration(duration))).dim(),
                ])
            }),
            StatusLineItem::Cost => props
                .status_line_metrics
                .cost
                .map(|cost| Line::from(vec![Span::from(format!("cost ${cost:.4}")).dim()])),
        };

        if let Some(segment) = segment {
            push_segment(segment);
        }
    }

    line
}

fn mode_segment(mode: SessionMode) -> Option<Line<'static>> {
    match mode {
        SessionMode::Normal => None,
        SessionMode::Plan => Some(Line::from(vec!["mode ".dim(), "plan".red().bold()])),
        SessionMode::Ask => Some(Line::from(vec!["mode ".dim(), "ask".green().bold()])),
    }
}

fn format_cwd_segment(cwd: &std::path::Path) -> Line<'static> {
    let display = if let Some(rel) = relativize_to_home(cwd) {
        if rel.as_os_str().is_empty() {
            "~".to_string()
        } else {
            format!("~{}{}", std::path::MAIN_SEPARATOR, rel.display())
        }
    } else {
        dunce::simplified(cwd).display().to_string()
    };
    Line::from(vec![Span::from(format!("cwd {display}")).dim()])
}

fn format_tokens_per_sec(rate: f64) -> String {
    if rate < 1.0 {
        format!("{rate:.2}")
    } else if rate < 100.0 {
        format!("{rate:.1}")
    } else {
        format!("{rate:.0}")
    }
}

fn push_model_segment(
    line: &mut Line<'static>,
    label: &'static str,
    model: &str,
    effort: Option<ReasoningEffort>,
) {
    if !label.is_empty() {
        line.push_span(format!("{label}: ").dim());
    }

    line.push_span(model.to_string());

    if let Some(label) = thinking_label_for(model, effort) {
        line.push_span(format!(" (reasoning {label})").dim());
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
    use std::time::Duration;

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
        let status_line_context = vec![StatusLineItem::Context];
        let status_line_model = vec![StatusLineItem::Model, StatusLineItem::Context];
        let status_line_metrics_items = vec![
            StatusLineItem::TokensPerSec,
            StatusLineItem::Latency,
            StatusLineItem::ToolTime,
            StatusLineItem::Cost,
        ];
        let status_line_metrics_empty = StatusLineMetrics::default();
        let status_line_metrics_sample = StatusLineMetrics {
            tokens_per_sec: Some(12.3),
            latency: Some(Duration::from_millis(450)),
            tool_time: Some(Duration::from_secs(2)),
            cost: Some(0.0042),
        };

        snapshot_footer(
            "footer_shortcuts_default",
            FooterProps {
                mode: FooterMode::ShortcutSummary,
                esc_backtrack_hint: false,
                is_task_running: false,
                context_window_percent: None,
                context_window_used_tokens: None,
                session_mode: SessionMode::Normal,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
                status_line_items: &status_line_context,
                status_line_cwd: None,
                status_line_git_branch: None,
                status_line_metrics: &status_line_metrics_empty,
                status_line_notice: None,
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
                session_mode: SessionMode::Normal,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_shift,
                status_line_items: &status_line_context,
                status_line_cwd: None,
                status_line_git_branch: None,
                status_line_metrics: &status_line_metrics_empty,
                status_line_notice: None,
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
                session_mode: SessionMode::Normal,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
                status_line_items: &status_line_context,
                status_line_cwd: None,
                status_line_git_branch: None,
                status_line_metrics: &status_line_metrics_empty,
                status_line_notice: None,
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
                session_mode: SessionMode::Normal,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
                status_line_items: &status_line_context,
                status_line_cwd: None,
                status_line_git_branch: None,
                status_line_metrics: &status_line_metrics_empty,
                status_line_notice: None,
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
                session_mode: SessionMode::Normal,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
                status_line_items: &status_line_context,
                status_line_cwd: None,
                status_line_git_branch: None,
                status_line_metrics: &status_line_metrics_empty,
                status_line_notice: None,
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
                session_mode: SessionMode::Normal,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
                status_line_items: &status_line_context,
                status_line_cwd: None,
                status_line_git_branch: None,
                status_line_metrics: &status_line_metrics_empty,
                status_line_notice: None,
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
                session_mode: SessionMode::Normal,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
                status_line_items: &status_line_context,
                status_line_cwd: None,
                status_line_git_branch: None,
                status_line_metrics: &status_line_metrics_empty,
                status_line_notice: None,
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
                session_mode: SessionMode::Normal,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
                status_line_items: &status_line_context,
                status_line_cwd: None,
                status_line_git_branch: None,
                status_line_metrics: &status_line_metrics_empty,
                status_line_notice: None,
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
                session_mode: SessionMode::Normal,
                model: "gpt-5.1-codex",
                reasoning_effort: Some(ReasoningEffort::Medium),
                keybindings: &keybindings_default,
                status_line_items: &status_line_model,
                status_line_cwd: None,
                status_line_git_branch: None,
                status_line_metrics: &status_line_metrics_empty,
                status_line_notice: None,
            },
        );

        snapshot_footer(
            "footer_status_line_metrics",
            FooterProps {
                mode: FooterMode::ContextOnly,
                esc_backtrack_hint: false,
                is_task_running: false,
                context_window_percent: None,
                context_window_used_tokens: None,
                session_mode: SessionMode::Normal,
                model: "",
                reasoning_effort: None,
                keybindings: &keybindings_default,
                status_line_items: &status_line_metrics_items,
                status_line_cwd: None,
                status_line_git_branch: None,
                status_line_metrics: &status_line_metrics_sample,
                status_line_notice: None,
            },
        );
    }
}
