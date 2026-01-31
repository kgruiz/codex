use diffy::Hunk;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line as RtLine;
use ratatui::text::Span as RtSpan;
use ratatui::widgets::Paragraph;
use similar::ChangeTag;
use similar::TextDiff;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::Color as SyntectColor;
use syntect::highlighting::FontStyle as SyntectFontStyle;
use syntect::highlighting::Style as SyntectStyle;
use syntect::highlighting::Theme;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use unicode_width::UnicodeWidthStr;

use crate::exec_command::relativize_to_home;
use crate::render::highlight::highlight_code_block_to_lines;
use crate::render::line_utils::line_to_static;
use crate::render::line_utils::prefix_lines;
use crate::render::renderable::Renderable;
use crate::wrapping::RtOptions;
use crate::wrapping::word_wrap_line;
use codex_core::config::types::DiffHighlighter;
use codex_core::config::types::DiffView;
use codex_core::git_info::get_git_repo_root;
use codex_core::protocol::FileChange;

// Internal representation for diff line rendering
#[derive(Copy, Clone)]
enum DiffLineType {
    Insert,
    Delete,
    Context,
}

const SIDE_BY_SIDE_SEPARATOR: &str = " │ ";

#[derive(Clone)]
struct ColumnLine {
    line_number: Option<usize>,
    spans: Vec<RtSpan<'static>>,
}

#[derive(Clone)]
struct SideBySideRow {
    left: Option<ColumnLine>,
    right: Option<ColumnLine>,
}

pub struct DiffSummary {
    changes: HashMap<PathBuf, FileChange>,
    cwd: PathBuf,
    view: DiffView,
    highlighter: DiffHighlighter,
}

impl DiffSummary {
    pub fn new(
        changes: HashMap<PathBuf, FileChange>,
        cwd: PathBuf,
        view: DiffView,
        highlighter: DiffHighlighter,
    ) -> Self {
        Self {
            changes,
            cwd,
            view,
            highlighter,
        }
    }
}

impl Renderable for DiffSummary {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = create_diff_summary(
            &self.changes,
            &self.cwd,
            area.width as usize,
            self.view,
            self.highlighter,
        );
        Paragraph::new(lines).render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        let lines = create_diff_summary(
            &self.changes,
            &self.cwd,
            width as usize,
            self.view,
            self.highlighter,
        );
        lines.len() as u16
    }
}

impl Renderable for FileChange {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let mut lines = vec![];
        render_change_line(self, &mut lines, area.width as usize);
        Paragraph::new(lines).render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        let mut lines = vec![];
        render_change_line(self, &mut lines, width as usize);
        lines.len() as u16
    }
}

pub(crate) fn create_diff_summary(
    changes: &HashMap<PathBuf, FileChange>,
    cwd: &Path,
    wrap_cols: usize,
    view: DiffView,
    highlighter: DiffHighlighter,
) -> Vec<RtLine<'static>> {
    let view_lines = render_diff_view(changes, cwd, wrap_cols, view, highlighter);
    if view_lines.is_empty() {
        vec![RtLine::from("(no changes)".dim().italic())]
    } else {
        view_lines
    }
}

pub(crate) fn render_diff_view(
    changes: &HashMap<PathBuf, FileChange>,
    cwd: &Path,
    wrap_cols: usize,
    view: DiffView,
    highlighter: DiffHighlighter,
) -> Vec<RtLine<'static>> {
    if changes.is_empty() {
        return Vec::new();
    }
    let rows = collect_rows(changes);
    match view {
        DiffView::Pretty => render_changes_pretty(rows, wrap_cols, cwd, highlighter),
        _ => render_changes_block(rows, wrap_cols, cwd, view),
    }
}

// Shared row for per-file presentation
#[derive(Clone)]
struct Row {
    #[allow(dead_code)]
    path: PathBuf,
    move_path: Option<PathBuf>,
    added: usize,
    removed: usize,
    change: FileChange,
}

#[derive(Clone)]
struct PrettyDiffLine {
    line_number: usize,
    kind: DiffLineType,
    text: String,
}

fn collect_rows(changes: &HashMap<PathBuf, FileChange>) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for (path, change) in changes.iter() {
        let (added, removed) = match change {
            FileChange::Add { content } => (content.lines().count(), 0),
            FileChange::Delete { content } => (0, content.lines().count()),
            FileChange::Update { unified_diff, .. } => calculate_add_remove_from_diff(unified_diff),
        };
        let move_path = match change {
            FileChange::Update {
                move_path: Some(new),
                ..
            } => Some(new.clone()),
            _ => None,
        };
        rows.push(Row {
            path: path.clone(),
            move_path,
            added,
            removed,
            change: change.clone(),
        });
    }
    rows.sort_by_key(|r| r.path.clone());
    rows
}

fn render_line_count_summary(added: usize, removed: usize) -> Vec<RtSpan<'static>> {
    let mut spans = Vec::new();
    spans.push("(".into());
    spans.push(format!("+{added}").green());
    spans.push(" ".into());
    spans.push(format!("-{removed}").red());
    spans.push(")".into());
    spans
}

fn render_changes_block(
    rows: Vec<Row>,
    wrap_cols: usize,
    cwd: &Path,
    view: DiffView,
) -> Vec<RtLine<'static>> {
    let mut out: Vec<RtLine<'static>> = Vec::new();

    let render_path = |row: &Row| -> Vec<RtSpan<'static>> {
        let mut spans = Vec::new();
        spans.push(display_path_for(&row.path, cwd).into());
        if let Some(move_path) = &row.move_path {
            let move_display = display_path_for(move_path, cwd);
            spans.push(format!(" → {move_display}").into());
        }
        spans
    };

    // Header
    let total_added: usize = rows.iter().map(|r| r.added).sum();
    let total_removed: usize = rows.iter().map(|r| r.removed).sum();
    let file_count = rows.len();
    let noun = if file_count == 1 { "file" } else { "files" };
    let mut header_spans: Vec<RtSpan<'static>> = vec!["• ".dim()];
    if let [row] = &rows[..] {
        let verb = match &row.change {
            FileChange::Add { .. } => "Added",
            FileChange::Delete { .. } => "Deleted",
            _ => "Edited",
        };
        header_spans.push(verb.bold());
        header_spans.push(" ".into());
        header_spans.extend(render_path(row));
        header_spans.push(" ".into());
        header_spans.extend(render_line_count_summary(row.added, row.removed));
    } else {
        header_spans.push("Edited".bold());
        header_spans.push(format!(" {file_count} {noun} ").into());
        header_spans.extend(render_line_count_summary(total_added, total_removed));
    }
    out.push(RtLine::from(header_spans));

    for (idx, r) in rows.into_iter().enumerate() {
        // Insert a blank separator between file chunks (except before the first)
        if idx > 0 {
            out.push("".into());
        }
        // File header line (skip when single-file header already shows the name)
        let skip_file_header = file_count == 1;
        if !skip_file_header {
            let mut header: Vec<RtSpan<'static>> = Vec::new();
            header.push("  └ ".dim());
            header.extend(render_path(&r));
            header.push(" ".into());
            header.extend(render_line_count_summary(r.added, r.removed));
            out.push(RtLine::from(header));
        }

        let mut lines = vec![];
        render_change_with_view(&r.change, &mut lines, wrap_cols - 4, view);
        out.extend(prefix_lines(lines, "    ".into(), "    ".into()));
    }

    out
}

fn render_changes_pretty(
    rows: Vec<Row>,
    wrap_cols: usize,
    cwd: &Path,
    highlighter: DiffHighlighter,
) -> Vec<RtLine<'static>> {
    let mut out: Vec<RtLine<'static>> = Vec::new();
    let mut first_excerpt = true;

    for row in rows {
        let (display_path, highlight_path) = pretty_paths(&row, cwd);
        match &row.change {
            FileChange::Add { content } => {
                let mut lines: Vec<PrettyDiffLine> = Vec::new();
                for (idx, raw) in content.lines().enumerate() {
                    lines.push(PrettyDiffLine {
                        line_number: idx + 1,
                        kind: DiffLineType::Insert,
                        text: raw.to_string(),
                    });
                }
                push_pretty_excerpt(
                    &mut out,
                    "Added",
                    &display_path,
                    lines.len(),
                    0,
                    lines,
                    wrap_cols,
                    &highlight_path,
                    highlighter,
                    &mut first_excerpt,
                );
            }
            FileChange::Delete { content } => {
                let mut lines: Vec<PrettyDiffLine> = Vec::new();
                for (idx, raw) in content.lines().enumerate() {
                    lines.push(PrettyDiffLine {
                        line_number: idx + 1,
                        kind: DiffLineType::Delete,
                        text: raw.to_string(),
                    });
                }
                push_pretty_excerpt(
                    &mut out,
                    "Deleted",
                    &display_path,
                    0,
                    lines.len(),
                    lines,
                    wrap_cols,
                    &highlight_path,
                    highlighter,
                    &mut first_excerpt,
                );
            }
            FileChange::Update { unified_diff, .. } => {
                let Ok(patch) = diffy::Patch::from_str(unified_diff) else {
                    continue;
                };
                for h in patch.hunks() {
                    let mut lines: Vec<PrettyDiffLine> = Vec::new();
                    let mut added = 0usize;
                    let mut removed = 0usize;
                    let mut old_ln = h.old_range().start();
                    let mut new_ln = h.new_range().start();
                    for line in h.lines() {
                        match line {
                            diffy::Line::Insert(text) => {
                                let s = text.trim_end_matches('\n');
                                lines.push(PrettyDiffLine {
                                    line_number: new_ln,
                                    kind: DiffLineType::Insert,
                                    text: s.to_string(),
                                });
                                added += 1;
                                new_ln += 1;
                            }
                            diffy::Line::Delete(text) => {
                                let s = text.trim_end_matches('\n');
                                lines.push(PrettyDiffLine {
                                    line_number: old_ln,
                                    kind: DiffLineType::Delete,
                                    text: s.to_string(),
                                });
                                removed += 1;
                                old_ln += 1;
                            }
                            diffy::Line::Context(text) => {
                                let s = text.trim_end_matches('\n');
                                lines.push(PrettyDiffLine {
                                    line_number: new_ln,
                                    kind: DiffLineType::Context,
                                    text: s.to_string(),
                                });
                                old_ln += 1;
                                new_ln += 1;
                            }
                        }
                    }
                    push_pretty_excerpt(
                        &mut out,
                        "Edited",
                        &display_path,
                        added,
                        removed,
                        lines,
                        wrap_cols,
                        &highlight_path,
                        highlighter,
                        &mut first_excerpt,
                    );
                }
            }
        }
    }

    out
}

fn pretty_paths(row: &Row, cwd: &Path) -> (String, PathBuf) {
    let mut display_path = display_path_for(&row.path, cwd);
    let highlight_path = row.move_path.as_ref().unwrap_or(&row.path).to_path_buf();
    if let Some(move_path) = &row.move_path {
        let move_display = display_path_for(move_path, cwd);
        display_path = format!("{display_path} → {move_display}");
    }
    (display_path, highlight_path)
}

fn push_pretty_excerpt(
    out: &mut Vec<RtLine<'static>>,
    verb: &str,
    display_path: &str,
    added: usize,
    removed: usize,
    lines: Vec<PrettyDiffLine>,
    wrap_cols: usize,
    highlight_path: &Path,
    highlighter: DiffHighlighter,
    first_excerpt: &mut bool,
) {
    if !*first_excerpt {
        out.push("".into());
    }

    out.push(RtLine::from(format!("{verb}({display_path})").bold()));
    out.push(RtLine::from(format!(
        "Added {added} lines, removed {removed} lines"
    )));

    if lines.is_empty() {
        *first_excerpt = false;
        return;
    }

    let max_line_number = lines.iter().map(|line| line.line_number).max().unwrap_or(0);
    let line_number_width = line_number_width(max_line_number);
    let highlighted_lines = highlight_pretty_lines(&lines, highlight_path, highlighter);

    for (line, spans) in lines.into_iter().zip(highlighted_lines.into_iter()) {
        out.extend(push_wrapped_pretty_diff_line(
            line.line_number,
            line.kind,
            spans,
            wrap_cols,
            line_number_width,
        ));
    }

    *first_excerpt = false;
}

fn render_change_with_view(
    change: &FileChange,
    out: &mut Vec<RtLine<'static>>,
    width: usize,
    view: DiffView,
) {
    match view {
        DiffView::Pretty => render_change_line(change, out, width),
        DiffView::Line => render_change_line(change, out, width),
        DiffView::Inline => render_change_inline(change, out, width),
        DiffView::SideBySide => render_change_side_by_side(change, out, width),
    }
}

fn render_change_line(change: &FileChange, out: &mut Vec<RtLine<'static>>, width: usize) {
    match change {
        FileChange::Add { content } => {
            let line_number_width = line_number_width(content.lines().count());
            for (i, raw) in content.lines().enumerate() {
                out.extend(push_wrapped_diff_line(
                    i + 1,
                    DiffLineType::Insert,
                    raw,
                    width,
                    line_number_width,
                    true,
                ));
            }
        }
        FileChange::Delete { content } => {
            let line_number_width = line_number_width(content.lines().count());
            for (i, raw) in content.lines().enumerate() {
                out.extend(push_wrapped_diff_line(
                    i + 1,
                    DiffLineType::Delete,
                    raw,
                    width,
                    line_number_width,
                    true,
                ));
            }
        }
        FileChange::Update { unified_diff, .. } => {
            if let Ok(patch) = diffy::Patch::from_str(unified_diff) {
                let mut max_line_number = 0;
                for h in patch.hunks() {
                    let mut old_ln = h.old_range().start();
                    let mut new_ln = h.new_range().start();
                    for l in h.lines() {
                        match l {
                            diffy::Line::Insert(_) => {
                                max_line_number = max_line_number.max(new_ln);
                                new_ln += 1;
                            }
                            diffy::Line::Delete(_) => {
                                max_line_number = max_line_number.max(old_ln);
                                old_ln += 1;
                            }
                            diffy::Line::Context(_) => {
                                max_line_number = max_line_number.max(new_ln);
                                old_ln += 1;
                                new_ln += 1;
                            }
                        }
                    }
                }
                let line_number_width = line_number_width(max_line_number);
                let mut is_first_hunk = true;
                for h in patch.hunks() {
                    if !is_first_hunk {
                        let spacer = format!("{:width$} ", "", width = line_number_width.max(1));
                        let spacer_span = RtSpan::styled(spacer, style_gutter());
                        out.push(RtLine::from(vec![spacer_span, "⋮".dim()]));
                    }
                    is_first_hunk = false;

                    let mut old_ln = h.old_range().start();
                    let mut new_ln = h.new_range().start();
                    for l in h.lines() {
                        match l {
                            diffy::Line::Insert(text) => {
                                let s = text.trim_end_matches('\n');
                                out.extend(push_wrapped_diff_line(
                                    new_ln,
                                    DiffLineType::Insert,
                                    s,
                                    width,
                                    line_number_width,
                                    true,
                                ));
                                new_ln += 1;
                            }
                            diffy::Line::Delete(text) => {
                                let s = text.trim_end_matches('\n');
                                out.extend(push_wrapped_diff_line(
                                    old_ln,
                                    DiffLineType::Delete,
                                    s,
                                    width,
                                    line_number_width,
                                    true,
                                ));
                                old_ln += 1;
                            }
                            diffy::Line::Context(text) => {
                                let s = text.trim_end_matches('\n');
                                out.extend(push_wrapped_diff_line(
                                    new_ln,
                                    DiffLineType::Context,
                                    s,
                                    width,
                                    line_number_width,
                                    true,
                                ));
                                old_ln += 1;
                                new_ln += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn render_change_inline(change: &FileChange, out: &mut Vec<RtLine<'static>>, width: usize) {
    match change {
        FileChange::Add { content } => {
            let line_number_width = line_number_width(content.lines().count());
            for (i, raw) in content.lines().enumerate() {
                out.extend(push_wrapped_diff_line(
                    i + 1,
                    DiffLineType::Insert,
                    raw,
                    width,
                    line_number_width,
                    false,
                ));
            }
        }
        FileChange::Delete { content } => {
            let line_number_width = line_number_width(content.lines().count());
            for (i, raw) in content.lines().enumerate() {
                out.extend(push_wrapped_diff_line(
                    i + 1,
                    DiffLineType::Delete,
                    raw,
                    width,
                    line_number_width,
                    false,
                ));
            }
        }
        FileChange::Update { unified_diff, .. } => {
            if let Ok(patch) = diffy::Patch::from_str(unified_diff) {
                let mut max_line_number = 0;
                for h in patch.hunks() {
                    let mut old_ln = h.old_range().start();
                    let mut new_ln = h.new_range().start();
                    for l in h.lines() {
                        match l {
                            diffy::Line::Insert(_) => {
                                max_line_number = max_line_number.max(new_ln);
                                new_ln += 1;
                            }
                            diffy::Line::Delete(_) => {
                                max_line_number = max_line_number.max(old_ln);
                                old_ln += 1;
                            }
                            diffy::Line::Context(_) => {
                                max_line_number = max_line_number.max(new_ln);
                                old_ln += 1;
                                new_ln += 1;
                            }
                        }
                    }
                }
                let line_number_width = line_number_width(max_line_number);
                let mut is_first_hunk = true;
                for h in patch.hunks() {
                    if !is_first_hunk {
                        let spacer = format!("{:width$} ", "", width = line_number_width.max(1));
                        let spacer_span = RtSpan::styled(spacer, style_gutter());
                        out.push(RtLine::from(vec![spacer_span, "⋮".dim()]));
                    }
                    is_first_hunk = false;

                    let mut old_ln = h.old_range().start();
                    let mut new_ln = h.new_range().start();
                    let mut idx = 0usize;
                    let hunk_lines = h.lines();
                    while idx < hunk_lines.len() {
                        match &hunk_lines[idx] {
                            diffy::Line::Delete(_text) => {
                                let mut deletes = Vec::new();
                                while idx < hunk_lines.len() {
                                    match &hunk_lines[idx] {
                                        diffy::Line::Delete(text) => {
                                            let s = text.trim_end_matches('\n').to_string();
                                            deletes.push((old_ln, s));
                                            old_ln += 1;
                                            idx += 1;
                                        }
                                        _ => break,
                                    }
                                }
                                let mut inserts = Vec::new();
                                while idx < hunk_lines.len() {
                                    match &hunk_lines[idx] {
                                        diffy::Line::Insert(text) => {
                                            let s = text.trim_end_matches('\n').to_string();
                                            inserts.push((new_ln, s));
                                            new_ln += 1;
                                            idx += 1;
                                        }
                                        _ => break,
                                    }
                                }

                                let paired = deletes.len().min(inserts.len());
                                for pair_idx in 0..paired {
                                    let (old_line, old_text) = &deletes[pair_idx];
                                    let (new_line, new_text) = &inserts[pair_idx];
                                    let (old_spans, new_spans) = inline_spans(old_text, new_text);
                                    out.extend(push_wrapped_inline_diff_line(
                                        *old_line,
                                        DiffLineType::Delete,
                                        old_spans,
                                        width,
                                        line_number_width,
                                    ));
                                    out.extend(push_wrapped_inline_diff_line(
                                        *new_line,
                                        DiffLineType::Insert,
                                        new_spans,
                                        width,
                                        line_number_width,
                                    ));
                                }
                                for (old_line, old_text) in deletes.into_iter().skip(paired) {
                                    out.extend(push_wrapped_diff_line(
                                        old_line,
                                        DiffLineType::Delete,
                                        &old_text,
                                        width,
                                        line_number_width,
                                        false,
                                    ));
                                }
                                for (new_line, new_text) in inserts.into_iter().skip(paired) {
                                    out.extend(push_wrapped_diff_line(
                                        new_line,
                                        DiffLineType::Insert,
                                        &new_text,
                                        width,
                                        line_number_width,
                                        false,
                                    ));
                                }
                            }
                            diffy::Line::Insert(text) => {
                                let s = text.trim_end_matches('\n');
                                out.extend(push_wrapped_diff_line(
                                    new_ln,
                                    DiffLineType::Insert,
                                    s,
                                    width,
                                    line_number_width,
                                    false,
                                ));
                                new_ln += 1;
                                idx += 1;
                            }
                            diffy::Line::Context(text) => {
                                let s = text.trim_end_matches('\n');
                                out.extend(push_wrapped_diff_line(
                                    new_ln,
                                    DiffLineType::Context,
                                    s,
                                    width,
                                    line_number_width,
                                    false,
                                ));
                                old_ln += 1;
                                new_ln += 1;
                                idx += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn render_change_side_by_side(change: &FileChange, out: &mut Vec<RtLine<'static>>, width: usize) {
    match change {
        FileChange::Add { content } => {
            let line_number_width = line_number_width(content.lines().count());
            let Some((left_width, right_width)) =
                side_by_side_column_widths(width, line_number_width)
            else {
                render_change_inline(change, out, width);
                return;
            };
            let rows = content
                .lines()
                .enumerate()
                .map(|(idx, raw)| SideBySideRow {
                    left: None,
                    right: Some(ColumnLine {
                        line_number: Some(idx + 1),
                        spans: vec![RtSpan::styled(raw.to_string(), style_add())],
                    }),
                })
                .collect();
            render_side_by_side_rows(rows, out, left_width, right_width, line_number_width);
        }
        FileChange::Delete { content } => {
            let line_number_width = line_number_width(content.lines().count());
            let Some((left_width, right_width)) =
                side_by_side_column_widths(width, line_number_width)
            else {
                render_change_inline(change, out, width);
                return;
            };
            let rows = content
                .lines()
                .enumerate()
                .map(|(idx, raw)| SideBySideRow {
                    left: Some(ColumnLine {
                        line_number: Some(idx + 1),
                        spans: vec![RtSpan::styled(raw.to_string(), style_del())],
                    }),
                    right: None,
                })
                .collect();
            render_side_by_side_rows(rows, out, left_width, right_width, line_number_width);
        }
        FileChange::Update { unified_diff, .. } => {
            let Ok(patch) = diffy::Patch::from_str(unified_diff) else {
                return;
            };
            let mut max_line_number = 0;
            for h in patch.hunks() {
                let mut old_ln = h.old_range().start();
                let mut new_ln = h.new_range().start();
                for l in h.lines() {
                    match l {
                        diffy::Line::Insert(_) => {
                            max_line_number = max_line_number.max(new_ln);
                            new_ln += 1;
                        }
                        diffy::Line::Delete(_) => {
                            max_line_number = max_line_number.max(old_ln);
                            old_ln += 1;
                        }
                        diffy::Line::Context(_) => {
                            max_line_number = max_line_number.max(new_ln);
                            old_ln += 1;
                            new_ln += 1;
                        }
                    }
                }
            }
            let line_number_width = line_number_width(max_line_number);
            let Some((left_width, right_width)) =
                side_by_side_column_widths(width, line_number_width)
            else {
                render_change_inline(change, out, width);
                return;
            };

            let mut rows: Vec<SideBySideRow> = Vec::new();
            let mut is_first_hunk = true;
            for h in patch.hunks() {
                if !is_first_hunk {
                    let divider = ColumnLine {
                        line_number: None,
                        spans: vec!["⋮".dim()],
                    };
                    rows.push(SideBySideRow {
                        left: Some(divider.clone()),
                        right: Some(divider),
                    });
                }
                is_first_hunk = false;

                let mut old_ln = h.old_range().start();
                let mut new_ln = h.new_range().start();
                let mut idx = 0usize;
                let hunk_lines = h.lines();
                while idx < hunk_lines.len() {
                    match &hunk_lines[idx] {
                        diffy::Line::Delete(_) => {
                            let mut deletes = Vec::new();
                            while idx < hunk_lines.len() {
                                match &hunk_lines[idx] {
                                    diffy::Line::Delete(text) => {
                                        let s = text.trim_end_matches('\n').to_string();
                                        deletes.push((old_ln, s));
                                        old_ln += 1;
                                        idx += 1;
                                    }
                                    _ => break,
                                }
                            }
                            let mut inserts = Vec::new();
                            while idx < hunk_lines.len() {
                                match &hunk_lines[idx] {
                                    diffy::Line::Insert(text) => {
                                        let s = text.trim_end_matches('\n').to_string();
                                        inserts.push((new_ln, s));
                                        new_ln += 1;
                                        idx += 1;
                                    }
                                    _ => break,
                                }
                            }

                            let paired = deletes.len().min(inserts.len());
                            for pair_idx in 0..paired {
                                let (old_line, old_text) = &deletes[pair_idx];
                                let (new_line, new_text) = &inserts[pair_idx];
                                let (old_spans, new_spans) = inline_spans(old_text, new_text);
                                rows.push(SideBySideRow {
                                    left: Some(ColumnLine {
                                        line_number: Some(*old_line),
                                        spans: old_spans,
                                    }),
                                    right: Some(ColumnLine {
                                        line_number: Some(*new_line),
                                        spans: new_spans,
                                    }),
                                });
                            }
                            for (old_line, old_text) in deletes.into_iter().skip(paired) {
                                rows.push(SideBySideRow {
                                    left: Some(ColumnLine {
                                        line_number: Some(old_line),
                                        spans: vec![RtSpan::styled(old_text, style_del())],
                                    }),
                                    right: None,
                                });
                            }
                            for (new_line, new_text) in inserts.into_iter().skip(paired) {
                                rows.push(SideBySideRow {
                                    left: None,
                                    right: Some(ColumnLine {
                                        line_number: Some(new_line),
                                        spans: vec![RtSpan::styled(new_text, style_add())],
                                    }),
                                });
                            }
                        }
                        diffy::Line::Insert(text) => {
                            let s = text.trim_end_matches('\n').to_string();
                            rows.push(SideBySideRow {
                                left: None,
                                right: Some(ColumnLine {
                                    line_number: Some(new_ln),
                                    spans: vec![RtSpan::styled(s, style_add())],
                                }),
                            });
                            new_ln += 1;
                            idx += 1;
                        }
                        diffy::Line::Context(text) => {
                            let s = text.trim_end_matches('\n').to_string();
                            let spans = vec![RtSpan::styled(s, style_context())];
                            rows.push(SideBySideRow {
                                left: Some(ColumnLine {
                                    line_number: Some(old_ln),
                                    spans: spans.clone(),
                                }),
                                right: Some(ColumnLine {
                                    line_number: Some(new_ln),
                                    spans,
                                }),
                            });
                            old_ln += 1;
                            new_ln += 1;
                            idx += 1;
                        }
                    }
                }
            }

            render_side_by_side_rows(rows, out, left_width, right_width, line_number_width);
        }
    }
}

fn inline_spans(old_text: &str, new_text: &str) -> (Vec<RtSpan<'static>>, Vec<RtSpan<'static>>) {
    let diff = TextDiff::from_words(old_text, new_text);
    let base_delete = style_del();
    let base_insert = style_add();
    let emph_delete = base_delete.add_modifier(Modifier::BOLD);
    let emph_insert = base_insert.add_modifier(Modifier::BOLD);

    let mut delete_spans = Vec::new();
    let mut insert_spans = Vec::new();

    for change in diff.iter_all_changes() {
        let text = change.to_string_lossy();
        if text.is_empty() {
            continue;
        }
        match change.tag() {
            ChangeTag::Equal => {
                delete_spans.push(RtSpan::styled(text.to_string(), base_delete));
                insert_spans.push(RtSpan::styled(text.to_string(), base_insert));
            }
            ChangeTag::Delete => {
                delete_spans.push(RtSpan::styled(text.to_string(), emph_delete));
            }
            ChangeTag::Insert => {
                insert_spans.push(RtSpan::styled(text.to_string(), emph_insert));
            }
        }
    }

    (delete_spans, insert_spans)
}

fn side_by_side_column_widths(
    total_width: usize,
    line_number_width: usize,
) -> Option<(usize, usize)> {
    let separator_width = UnicodeWidthStr::width(SIDE_BY_SIDE_SEPARATOR);
    let min_column_width = line_number_width + 1;
    if total_width < separator_width + min_column_width * 2 {
        return None;
    }
    let available = total_width.saturating_sub(separator_width);
    let left_width = available / 2;
    let right_width = available - left_width;
    Some((left_width, right_width))
}

fn render_side_by_side_rows(
    rows: Vec<SideBySideRow>,
    out: &mut Vec<RtLine<'static>>,
    left_width: usize,
    right_width: usize,
    line_number_width: usize,
) {
    let separator = SIDE_BY_SIDE_SEPARATOR.dim();
    for row in rows {
        let left_lines = row
            .left
            .map(|line| wrap_column_line(line, left_width, line_number_width))
            .unwrap_or_else(|| vec![blank_line(left_width)]);
        let right_lines = row
            .right
            .map(|line| wrap_column_line(line, right_width, line_number_width))
            .unwrap_or_else(|| vec![blank_line(right_width)]);
        let row_count = left_lines.len().max(right_lines.len());
        for idx in 0..row_count {
            let left_line = left_lines
                .get(idx)
                .cloned()
                .unwrap_or_else(|| blank_line(left_width));
            let right_line = right_lines
                .get(idx)
                .cloned()
                .unwrap_or_else(|| blank_line(right_width));
            let mut spans = Vec::new();
            spans.extend(left_line.spans);
            spans.push(separator.clone());
            spans.extend(right_line.spans);
            out.push(RtLine::from(spans));
        }
    }
}

fn wrap_column_line(
    line: ColumnLine,
    width: usize,
    line_number_width: usize,
) -> Vec<RtLine<'static>> {
    let gutter_width = line_number_width.max(1);
    let ln_str = line.line_number.map(|n| n.to_string()).unwrap_or_default();
    let gutter = RtSpan::styled(format!("{ln_str:>gutter_width$} "), style_gutter());
    let spacer = RtSpan::styled(format!("{:gutter_width$} ", ""), style_gutter());
    let content_width = width.saturating_sub(gutter_width + 1).max(1);

    let content = RtLine::from(line.spans);
    let wrapped = word_wrap_line(&content, RtOptions::new(content_width));
    if wrapped.is_empty() {
        return vec![pad_line_to_width(RtLine::from(vec![gutter]), width)];
    }

    wrapped
        .into_iter()
        .enumerate()
        .map(|(idx, line)| {
            let mut spans = Vec::with_capacity(line.spans.len() + 1);
            spans.push(if idx == 0 {
                gutter.clone()
            } else {
                spacer.clone()
            });
            spans.extend(
                line.spans
                    .into_iter()
                    .map(|span| RtSpan::styled(span.content.to_string(), span.style)),
            );
            pad_line_to_width(RtLine::from(spans), width)
        })
        .collect()
}

fn pad_line_to_width(mut line: RtLine<'static>, width: usize) -> RtLine<'static> {
    let current = line_width(&line);
    if current < width {
        line.spans.push(RtSpan::raw(" ".repeat(width - current)));
    }
    line
}

fn pad_pretty_line(mut line: RtLine<'static>, width: usize, _bg: Option<Color>) -> RtLine<'static> {
    let current = line_width(&line);
    if current < width {
        line.spans.push(RtSpan::raw(" ".repeat(width - current)));
    }
    line
}

fn line_width(line: &RtLine<'static>) -> usize {
    line.spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
}

fn blank_line(width: usize) -> RtLine<'static> {
    RtLine::from(vec![RtSpan::raw(" ".repeat(width))])
}

fn push_wrapped_pretty_diff_line(
    line_number: usize,
    kind: DiffLineType,
    spans: Vec<RtSpan<'static>>,
    width: usize,
    line_number_width: usize,
) -> Vec<RtLine<'static>> {
    let ln_str = line_number.to_string();
    let gutter_width = line_number_width.max(1);
    let bg = pretty_bg(kind);
    let gutter_style = apply_bg(style_gutter(), bg);
    let sign_style = apply_bg(pretty_sign_style(kind), bg);
    let sign_char = match kind {
        DiffLineType::Insert => '+',
        DiffLineType::Delete => '-',
        DiffLineType::Context => ' ',
    };

    let gutter = RtSpan::styled(format!("{ln_str:>gutter_width$} "), gutter_style);
    let sign = RtSpan::styled(sign_char.to_string(), sign_style);
    let sign_space = RtSpan::styled(" ".to_string(), sign_style);
    let indent_first = RtLine::from(vec![gutter.clone(), sign, sign_space.clone()]);
    let spacer_gutter = RtSpan::styled(format!("{:gutter_width$} ", ""), gutter_style);
    let spacer_sign = RtSpan::styled(" ".to_string(), sign_style);
    let indent_sub = RtLine::from(vec![spacer_gutter, spacer_sign, sign_space]);

    let content = RtLine::from(apply_bg_to_spans(spans, bg));
    let opts = RtOptions::new(width)
        .initial_indent(indent_first)
        .subsequent_indent(indent_sub);

    word_wrap_line(&content, opts)
        .iter()
        .map(line_to_static)
        .map(|line| pad_pretty_line(line, width, bg))
        .collect()
}

fn push_wrapped_inline_diff_line(
    line_number: usize,
    kind: DiffLineType,
    spans: Vec<RtSpan<'static>>,
    width: usize,
    line_number_width: usize,
) -> Vec<RtLine<'static>> {
    let ln_str = line_number.to_string();
    let gutter_width = line_number_width.max(1);
    let gutter = RtSpan::styled(format!("{ln_str:>gutter_width$} "), style_gutter());

    let line_style = match kind {
        DiffLineType::Insert => style_add(),
        DiffLineType::Delete => style_del(),
        DiffLineType::Context => style_context(),
    };
    let sign = RtSpan::styled(" ".to_string(), line_style);
    let indent_first = RtLine::from(vec![gutter.clone(), sign]);
    let spacer = RtSpan::styled(format!("{:gutter_width$} ", ""), style_gutter());
    let indent_sub = RtLine::from(vec![spacer, RtSpan::styled(" ".to_string(), line_style)]);

    let content = RtLine::from(spans);
    let opts = RtOptions::new(width)
        .initial_indent(indent_first)
        .subsequent_indent(indent_sub);

    word_wrap_line(&content, opts)
        .iter()
        .map(line_to_static)
        .collect()
}

fn highlight_pretty_lines(
    lines: &[PrettyDiffLine],
    path: &Path,
    highlighter: DiffHighlighter,
) -> Vec<Vec<RtSpan<'static>>> {
    let content_lines = lines
        .iter()
        .map(|line| line.text.clone())
        .collect::<Vec<_>>();
    match highlighter {
        DiffHighlighter::TreeSitter => {
            highlight_lines_tree_sitter(&content_lines, diff_language_for_path(path))
        }
        DiffHighlighter::Syntect => highlight_lines_syntect(&content_lines, path),
    }
}

fn highlight_lines_tree_sitter(
    lines: &[String],
    language: Option<&'static str>,
) -> Vec<Vec<RtSpan<'static>>> {
    lines
        .iter()
        .map(|line| {
            if line.is_empty() {
                return Vec::new();
            }
            let mut highlighted = highlight_code_block_to_lines(language, line);
            highlighted
                .pop()
                .map(|line| line.spans)
                .unwrap_or_else(|| vec![line.to_string().into()])
        })
        .collect()
}

fn highlight_lines_syntect(lines: &[String], path: &Path) -> Vec<Vec<RtSpan<'static>>> {
    let syntax_set = syntect_syntax_set();
    let syntax = path
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| syntax_set.find_syntax_by_extension(ext))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    let theme = syntect_theme();
    let mut highlighter = HighlightLines::new(syntax, theme);
    lines
        .iter()
        .map(|line| {
            if line.is_empty() {
                return Vec::new();
            }
            match highlighter.highlight_line(line, syntax_set) {
                Ok(ranges) => ranges
                    .into_iter()
                    .map(|(style, text)| {
                        RtSpan::styled(text.to_string(), syntect_style_to_ratatui(style))
                    })
                    .collect(),
                Err(_) => vec![line.to_string().into()],
            }
        })
        .collect()
}

fn diff_language_for_path(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "bash" | "bashrc" | "sh" | "zsh" | "zshrc" | "fish" => Some("bash"),
        "json" | "jsonc" | "json5" => Some("json"),
        "py" | "pyi" => Some("python"),
        "rs" => Some("rust"),
        "yaml" | "yml" => Some("yaml"),
        _ => None,
    }
}

fn syntect_syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn syntect_theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let theme_set = ThemeSet::load_defaults();
        #[expect(clippy::expect_used)]
        theme_set
            .themes
            .get("base16-ocean.dark")
            .or_else(|| theme_set.themes.values().next())
            .expect("syntect themes missing")
            .clone()
    })
}

fn syntect_style_to_ratatui(style: SyntectStyle) -> Style {
    let mut out = Style::default();
    if style.foreground.a != 0 {
        out = out.fg(syntect_color_to_ratatui(style.foreground));
    }
    if style.font_style.contains(SyntectFontStyle::BOLD) {
        out = out.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(SyntectFontStyle::ITALIC) {
        out = out.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(SyntectFontStyle::UNDERLINE) {
        out = out.add_modifier(Modifier::UNDERLINED);
    }
    out
}

fn syntect_color_to_ratatui(color: SyntectColor) -> Color {
    let r = color.r;
    let g = color.g;
    let b = color.b;
    if r == g && g == b {
        return if r < 64 {
            Color::Black
        } else if r < 128 {
            Color::DarkGray
        } else if r < 192 {
            Color::Gray
        } else {
            Color::White
        };
    }

    let max = r.max(g).max(b);
    let bright = max > 200;
    if r >= g && r >= b {
        if g > b {
            if bright {
                Color::LightYellow
            } else {
                Color::Yellow
            }
        } else if bright {
            Color::LightRed
        } else {
            Color::Red
        }
    } else if g >= r && g >= b {
        if r > b {
            if bright {
                Color::LightYellow
            } else {
                Color::Yellow
            }
        } else if bright {
            Color::LightGreen
        } else {
            Color::Green
        }
    } else if r > g {
        if bright {
            Color::LightMagenta
        } else {
            Color::Magenta
        }
    } else if bright {
        Color::LightBlue
    } else {
        Color::Blue
    }
}

pub(crate) fn display_path_for(path: &Path, cwd: &Path) -> String {
    let path_in_same_repo = match (get_git_repo_root(cwd), get_git_repo_root(path)) {
        (Some(cwd_repo), Some(path_repo)) => cwd_repo == path_repo,
        _ => false,
    };
    let chosen = if path_in_same_repo {
        pathdiff::diff_paths(path, cwd).unwrap_or_else(|| path.to_path_buf())
    } else {
        relativize_to_home(path)
            .map(|p| PathBuf::from_iter([Path::new("~"), p.as_path()]))
            .unwrap_or_else(|| path.to_path_buf())
    };
    chosen.display().to_string()
}

fn calculate_add_remove_from_diff(diff: &str) -> (usize, usize) {
    if let Ok(patch) = diffy::Patch::from_str(diff) {
        patch
            .hunks()
            .iter()
            .flat_map(Hunk::lines)
            .fold((0, 0), |(a, d), l| match l {
                diffy::Line::Insert(_) => (a + 1, d),
                diffy::Line::Delete(_) => (a, d + 1),
                diffy::Line::Context(_) => (a, d),
            })
    } else {
        // For unparsable diffs, return 0 for both counts.
        (0, 0)
    }
}

fn push_wrapped_diff_line(
    line_number: usize,
    kind: DiffLineType,
    text: &str,
    width: usize,
    line_number_width: usize,
    show_sign: bool,
) -> Vec<RtLine<'static>> {
    let ln_str = line_number.to_string();
    let mut remaining_text: &str = text;

    // Reserve a fixed number of spaces (equal to the widest line number plus a
    // trailing spacer) so the sign column stays aligned across the diff block.
    let gutter_width = line_number_width.max(1);
    let prefix_cols = gutter_width + 1;

    let mut first = true;
    let (sign_char, line_style) = match kind {
        DiffLineType::Insert => ('+', style_add()),
        DiffLineType::Delete => ('-', style_del()),
        DiffLineType::Context => (' ', style_context()),
    };
    let sign_char = if show_sign { sign_char } else { ' ' };
    let mut lines: Vec<RtLine<'static>> = Vec::new();

    loop {
        // Fit the content for the current terminal row:
        // compute how many columns are available after the prefix, then split
        // at a UTF-8 character boundary so this row's chunk fits exactly.
        let available_content_cols = width.saturating_sub(prefix_cols + 1).max(1);
        let split_at_byte_index = remaining_text
            .char_indices()
            .nth(available_content_cols)
            .map(|(i, _)| i)
            .unwrap_or_else(|| remaining_text.len());
        let (chunk, rest) = remaining_text.split_at(split_at_byte_index);
        remaining_text = rest;

        if first {
            // Build gutter (right-aligned line number plus spacer) as a dimmed span
            let gutter = format!("{ln_str:>gutter_width$} ");
            // Content with a sign ('+'/'-'/' ') styled per diff kind
            let content = format!("{sign_char}{chunk}");
            lines.push(RtLine::from(vec![
                RtSpan::styled(gutter, style_gutter()),
                RtSpan::styled(content, line_style),
            ]));
            first = false;
        } else {
            // Continuation lines keep a space for the sign column so content aligns
            let gutter = format!("{:gutter_width$}  ", "");
            lines.push(RtLine::from(vec![
                RtSpan::styled(gutter, style_gutter()),
                RtSpan::styled(chunk.to_string(), line_style),
            ]));
        }
        if remaining_text.is_empty() {
            break;
        }
    }
    lines
}

fn line_number_width(max_line_number: usize) -> usize {
    if max_line_number == 0 {
        1
    } else {
        max_line_number.to_string().len()
    }
}

fn style_gutter() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

fn style_context() -> Style {
    Style::default()
}

fn style_add() -> Style {
    Style::default().fg(Color::Green)
}

fn style_del() -> Style {
    Style::default().fg(Color::Red)
}

fn pretty_bg(kind: DiffLineType) -> Option<Color> {
    match kind {
        DiffLineType::Insert => Some(Color::Green),
        DiffLineType::Delete => Some(Color::Red),
        DiffLineType::Context => None,
    }
}

fn pretty_sign_style(kind: DiffLineType) -> Style {
    match kind {
        DiffLineType::Insert => style_add().add_modifier(Modifier::BOLD),
        DiffLineType::Delete => style_del().add_modifier(Modifier::BOLD),
        DiffLineType::Context => Style::default(),
    }
}

fn apply_bg(style: Style, bg: Option<Color>) -> Style {
    match bg {
        Some(color) => style.bg(color),
        None => style,
    }
}

fn apply_bg_to_spans(spans: Vec<RtSpan<'static>>, bg: Option<Color>) -> Vec<RtSpan<'static>> {
    let Some(bg) = bg else {
        return spans;
    };
    spans
        .into_iter()
        .map(|span| RtSpan::styled(span.content.to_string(), span.style.bg(bg)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use pretty_assertions::assert_eq;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::text::Text;
    use ratatui::widgets::Paragraph;
    use ratatui::widgets::WidgetRef;
    use ratatui::widgets::Wrap;
    fn diff_summary_for_tests(
        changes: &HashMap<PathBuf, FileChange>,
        view: DiffView,
    ) -> Vec<RtLine<'static>> {
        create_diff_summary(
            changes,
            &PathBuf::from("/"),
            80,
            view,
            DiffHighlighter::TreeSitter,
        )
    }

    fn snapshot_lines(name: &str, lines: Vec<RtLine<'static>>, width: u16, height: u16) {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|f| {
                Paragraph::new(Text::from(lines))
                    .wrap(Wrap { trim: false })
                    .render_ref(f.area(), f.buffer_mut())
            })
            .expect("draw");
        assert_snapshot!(name, terminal.backend());
    }

    fn snapshot_lines_text(name: &str, lines: &[RtLine<'static>]) {
        // Convert Lines to plain text rows and trim trailing spaces so it's
        // easier to validate indentation visually in snapshots.
        let text = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .map(|s| s.trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert_snapshot!(name, text);
    }

    #[test]
    fn ui_snapshot_wrap_behavior_insert() {
        // Narrow width to force wrapping within our diff line rendering
        let long_line = "this is a very long line that should wrap across multiple terminal columns and continue";

        // Call the wrapping function directly so we can precisely control the width
        let lines = push_wrapped_diff_line(
            1,
            DiffLineType::Insert,
            long_line,
            80,
            line_number_width(1),
            true,
        );

        // Render into a small terminal to capture the visual layout
        snapshot_lines("wrap_behavior_insert", lines, 90, 8);
    }

    #[test]
    fn ui_snapshot_apply_update_block() {
        let mut changes: HashMap<PathBuf, FileChange> = HashMap::new();
        let original = "line one\nline two\nline three\n";
        let modified = "line one\nline two changed\nline three\n";
        let patch = diffy::create_patch(original, modified).to_string();

        changes.insert(
            PathBuf::from("example.txt"),
            FileChange::Update {
                unified_diff: patch,
                move_path: None,
            },
        );

        let lines = diff_summary_for_tests(&changes, DiffView::Line);

        snapshot_lines("apply_update_block", lines, 80, 12);
    }

    #[test]
    fn ui_snapshot_pretty_update_block_text() {
        let mut changes: HashMap<PathBuf, FileChange> = HashMap::new();
        let original = "line one\nline two\nline three\n";
        let modified = "line one\nline two changed\nline three\n";
        let patch = diffy::create_patch(original, modified).to_string();

        changes.insert(
            PathBuf::from("example.txt"),
            FileChange::Update {
                unified_diff: patch,
                move_path: None,
            },
        );

        let lines = create_diff_summary(
            &changes,
            &PathBuf::from("/"),
            80,
            DiffView::Pretty,
            DiffHighlighter::TreeSitter,
        );

        snapshot_lines_text("pretty_update_block_text", &lines);
    }

    #[test]
    fn ui_snapshot_apply_update_with_rename_block() {
        let mut changes: HashMap<PathBuf, FileChange> = HashMap::new();
        let original = "A\nB\nC\n";
        let modified = "A\nB changed\nC\n";
        let patch = diffy::create_patch(original, modified).to_string();

        changes.insert(
            PathBuf::from("old_name.rs"),
            FileChange::Update {
                unified_diff: patch,
                move_path: Some(PathBuf::from("new_name.rs")),
            },
        );

        let lines = diff_summary_for_tests(&changes, DiffView::Line);

        snapshot_lines("apply_update_with_rename_block", lines, 80, 12);
    }

    #[test]
    fn ui_snapshot_apply_multiple_files_block() {
        // Two files: one update and one add, to exercise combined header and per-file rows
        let mut changes: HashMap<PathBuf, FileChange> = HashMap::new();

        // File a.txt: single-line replacement (one delete, one insert)
        let patch_a = diffy::create_patch("one\n", "one changed\n").to_string();
        changes.insert(
            PathBuf::from("a.txt"),
            FileChange::Update {
                unified_diff: patch_a,
                move_path: None,
            },
        );

        // File b.txt: newly added with one line
        changes.insert(
            PathBuf::from("b.txt"),
            FileChange::Add {
                content: "new\n".to_string(),
            },
        );

        let lines = diff_summary_for_tests(&changes, DiffView::Line);

        snapshot_lines("apply_multiple_files_block", lines, 80, 14);
    }

    #[test]
    fn ui_snapshot_apply_add_block() {
        let mut changes: HashMap<PathBuf, FileChange> = HashMap::new();
        changes.insert(
            PathBuf::from("new_file.txt"),
            FileChange::Add {
                content: "alpha\nbeta\n".to_string(),
            },
        );

        let lines = diff_summary_for_tests(&changes, DiffView::Line);

        snapshot_lines("apply_add_block", lines, 80, 10);
    }

    #[test]
    fn ui_snapshot_apply_delete_block() {
        let mut changes: HashMap<PathBuf, FileChange> = HashMap::new();
        changes.insert(
            PathBuf::from("tmp_delete_example.txt"),
            FileChange::Delete {
                content: "first\nsecond\nthird\n".to_string(),
            },
        );

        let lines = diff_summary_for_tests(&changes, DiffView::Line);

        snapshot_lines("apply_delete_block", lines, 80, 12);
    }

    #[test]
    fn ui_snapshot_apply_update_block_wraps_long_lines() {
        // Create a patch with a long modified line to force wrapping
        let original = "line 1\nshort\nline 3\n";
        let modified = "line 1\nshort this_is_a_very_long_modified_line_that_should_wrap_across_multiple_terminal_columns_and_continue_even_further_beyond_eighty_columns_to_force_multiple_wraps\nline 3\n";
        let patch = diffy::create_patch(original, modified).to_string();

        let mut changes: HashMap<PathBuf, FileChange> = HashMap::new();
        changes.insert(
            PathBuf::from("long_example.txt"),
            FileChange::Update {
                unified_diff: patch,
                move_path: None,
            },
        );

        let lines = create_diff_summary(
            &changes,
            &PathBuf::from("/"),
            72,
            DiffView::Line,
            DiffHighlighter::TreeSitter,
        );

        // Render with backend width wider than wrap width to avoid Paragraph auto-wrap.
        snapshot_lines("apply_update_block_wraps_long_lines", lines, 80, 12);
    }

    #[test]
    fn ui_snapshot_apply_update_block_wraps_long_lines_text() {
        // This mirrors the desired layout example: sign only on first inserted line,
        // subsequent wrapped pieces start aligned under the line number gutter.
        let original = "1\n2\n3\n4\n";
        let modified = "1\nadded long line which wraps and_if_there_is_a_long_token_it_will_be_broken\n3\n4 context line which also wraps across\n";
        let patch = diffy::create_patch(original, modified).to_string();

        let mut changes: HashMap<PathBuf, FileChange> = HashMap::new();
        changes.insert(
            PathBuf::from("wrap_demo.txt"),
            FileChange::Update {
                unified_diff: patch,
                move_path: None,
            },
        );

        let lines = create_diff_summary(
            &changes,
            &PathBuf::from("/"),
            28,
            DiffView::Line,
            DiffHighlighter::TreeSitter,
        );
        snapshot_lines_text("apply_update_block_wraps_long_lines_text", &lines);
    }

    #[test]
    fn ui_snapshot_apply_update_block_line_numbers_three_digits_text() {
        let original = (1..=110).map(|i| format!("line {i}\n")).collect::<String>();
        let modified = (1..=110)
            .map(|i| {
                if i == 100 {
                    format!("line {i} changed\n")
                } else {
                    format!("line {i}\n")
                }
            })
            .collect::<String>();
        let patch = diffy::create_patch(&original, &modified).to_string();

        let mut changes: HashMap<PathBuf, FileChange> = HashMap::new();
        changes.insert(
            PathBuf::from("hundreds.txt"),
            FileChange::Update {
                unified_diff: patch,
                move_path: None,
            },
        );

        let lines = create_diff_summary(
            &changes,
            &PathBuf::from("/"),
            80,
            DiffView::Line,
            DiffHighlighter::TreeSitter,
        );
        snapshot_lines_text("apply_update_block_line_numbers_three_digits_text", &lines);
    }

    #[test]
    fn ui_snapshot_apply_update_block_relativizes_path() {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let abs_old = cwd.join("abs_old.rs");
        let abs_new = cwd.join("abs_new.rs");

        let original = "X\nY\n";
        let modified = "X changed\nY\n";
        let patch = diffy::create_patch(original, modified).to_string();

        let mut changes: HashMap<PathBuf, FileChange> = HashMap::new();
        changes.insert(
            abs_old,
            FileChange::Update {
                unified_diff: patch,
                move_path: Some(abs_new),
            },
        );

        let lines = create_diff_summary(
            &changes,
            &cwd,
            80,
            DiffView::Line,
            DiffHighlighter::TreeSitter,
        );

        snapshot_lines("apply_update_block_relativizes_path", lines, 80, 10);
    }

    #[test]
    fn side_by_side_separator_stays_aligned() {
        let original = concat!(
            "const detailQuery = useRegulatorDetailQuery(selectedId);\n",
            "const lineTwo = fooBarBaz;\n",
        );
        let modified = concat!(
            "const detailQuery = useRegulatorDetailQuery(selectedId);\n",
            "const lineTwo = useRegulatorDetailQuerySelectedIdAndSomethingLonger;\n",
        );
        let patch = diffy::create_patch(original, modified).to_string();

        let mut changes: HashMap<PathBuf, FileChange> = HashMap::new();
        changes.insert(
            PathBuf::from("example.ts"),
            FileChange::Update {
                unified_diff: patch,
                move_path: None,
            },
        );

        let lines = create_diff_summary(
            &changes,
            &PathBuf::from("/"),
            60,
            DiffView::SideBySide,
            DiffHighlighter::TreeSitter,
        );
        let positions = lines
            .iter()
            .filter_map(|line| {
                let text = line
                    .spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>();
                text.find('│')
                    .map(|idx| UnicodeWidthStr::width(&text[..idx]))
            })
            .collect::<Vec<_>>();

        assert!(!positions.is_empty(), "expected side-by-side separator");
        let first = positions[0];
        for pos in positions.iter().skip(1) {
            assert_eq!(*pos, first, "separator drift: {positions:?}");
        }
    }
}
