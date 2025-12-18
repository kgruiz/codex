use std::path::Path;
use std::path::PathBuf;

use chrono::DateTime;
use chrono::Utc;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Stylize as _;
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::widgets::Widget;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::diff_render::display_path_for;
use crate::history_cell;
use crate::key_hint;
use crate::render::Insets;
use crate::render::RectExt as _;
use crate::session_manager::SessionManagerEntry;
use crate::session_manager::paths_match;
use crate::style::user_message_style;

use super::CancellationEvent;
use super::RenameTarget;
use super::bottom_pane_view::BottomPaneView;
use super::bottom_pane_view::ViewCompletionBehavior;
use super::popup_consts::MAX_POPUP_ROWS;
use super::scroll_state::ScrollState;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::measure_rows_height;
use super::selection_popup_common::render_rows;

pub(crate) struct SessionManagerView {
    sessions: Vec<SessionManagerEntry>,
    state: ScrollState,
    complete: bool,
    delete_confirm_path: Option<PathBuf>,
    app_event_tx: AppEventSender,
    loading: bool,
    error: Option<String>,
    current_cwd: PathBuf,
}

impl SessionManagerView {
    pub(crate) fn new(app_event_tx: AppEventSender, current_cwd: PathBuf) -> Self {
        Self {
            sessions: Vec::new(),
            state: ScrollState::new(),
            complete: false,
            delete_confirm_path: None,
            app_event_tx,
            loading: true,
            error: None,
            current_cwd,
        }
    }

    pub(crate) fn set_sessions(&mut self, sessions: Vec<SessionManagerEntry>) {
        let selected_path = self.selected_path();
        self.sessions = sessions;
        self.loading = false;
        self.error = None;
        self.delete_confirm_path = None;

        self.state.selected_idx = selected_path
            .and_then(|path| {
                self.sessions
                    .iter()
                    .position(|entry| paths_match(&entry.path, &path))
            })
            .or_else(|| self.sessions.iter().position(|entry| entry.is_current))
            .or_else(|| (!self.sessions.is_empty()).then_some(0));

        let len = self.sessions.len();
        self.state.clamp_selection(len);
        self.state.ensure_visible(len, Self::max_visible_rows(len));
    }

    pub(crate) fn set_error(&mut self, message: String) {
        self.loading = false;
        self.error = Some(message);
        self.sessions.clear();
        self.state.reset();
        self.delete_confirm_path = None;
    }

    pub(crate) fn apply_rename(&mut self, path: &Path, title: Option<String>) -> bool {
        let mut updated = false;
        for entry in &mut self.sessions {
            if paths_match(&entry.path, path) {
                entry.title = title;
                updated = true;
                break;
            }
        }
        updated
    }

    pub(crate) fn apply_delete(&mut self, path: &Path) -> bool {
        if let Some(idx) = self
            .sessions
            .iter()
            .position(|entry| paths_match(&entry.path, path))
        {
            self.sessions.remove(idx);
            let len = self.sessions.len();
            self.state.clamp_selection(len);
            self.state.ensure_visible(len, Self::max_visible_rows(len));
            self.delete_confirm_path = None;
            return true;
        }
        false
    }

    fn selected_path(&self) -> Option<PathBuf> {
        self.state
            .selected_idx
            .and_then(|idx| self.sessions.get(idx))
            .map(|entry| entry.path.clone())
    }

    fn selected_entry(&self) -> Option<&SessionManagerEntry> {
        self.state
            .selected_idx
            .and_then(|idx| self.sessions.get(idx))
    }

    fn move_up(&mut self) {
        let len = self.sessions.len();
        self.state.move_up_wrap(len);
        self.state.ensure_visible(len, Self::max_visible_rows(len));
        self.delete_confirm_path = None;
    }

    fn move_down(&mut self) {
        let len = self.sessions.len();
        self.state.move_down_wrap(len);
        self.state.ensure_visible(len, Self::max_visible_rows(len));
        self.delete_confirm_path = None;
    }

    fn page_up(&mut self, step: usize) {
        let len = self.sessions.len();
        if len == 0 {
            return;
        }

        if let Some(sel) = self.state.selected_idx {
            self.state.selected_idx = Some(sel.saturating_sub(step));
        } else {
            self.state.selected_idx = Some(0);
        }
        self.state.ensure_visible(len, Self::max_visible_rows(len));
        self.delete_confirm_path = None;
    }

    fn page_down(&mut self, step: usize) {
        let len = self.sessions.len();
        if len == 0 {
            return;
        }

        let max_index = len.saturating_sub(1);
        let new_idx = match self.state.selected_idx {
            Some(sel) => (sel + step).min(max_index),
            None => 0,
        };
        self.state.selected_idx = Some(new_idx);
        self.state.ensure_visible(len, Self::max_visible_rows(len));
        self.delete_confirm_path = None;
    }

    fn accept(&mut self) {
        let Some(selected) = self.selected_entry() else {
            self.complete = true;
            return;
        };

        if selected.is_current {
            self.complete = true;
            return;
        }

        self.app_event_tx.send(AppEvent::SessionManagerSwitch {
            path: selected.path.clone(),
        });
        self.complete = true;
    }

    fn request_rename(&mut self) {
        let Some(selected) = self.selected_entry() else {
            return;
        };

        let target = if selected.is_current {
            RenameTarget::CurrentSession
        } else {
            RenameTarget::SessionPath(selected.path.clone())
        };

        self.app_event_tx.send(AppEvent::OpenRenameSessionView {
            target,
            current_title: selected.title.clone(),
        });
    }

    fn request_delete(&mut self) {
        let Some(selected) = self.selected_entry() else {
            return;
        };

        if selected.is_current {
            self.send_info_message("Cannot delete the active session.".to_string());
            self.delete_confirm_path = None;
            return;
        }

        if self
            .delete_confirm_path
            .as_ref()
            .is_some_and(|path| paths_match(path, &selected.path))
        {
            let label = selected.display_title().to_string();
            self.app_event_tx.send(AppEvent::SessionManagerDelete {
                path: selected.path.clone(),
                label,
            });
            self.delete_confirm_path = None;
        } else {
            self.delete_confirm_path = Some(selected.path.clone());
        }
    }

    fn send_info_message(&self, message: String) {
        let cell = history_cell::new_info_event(message, None);
        self.app_event_tx
            .send(AppEvent::InsertHistoryCell(Box::new(cell)));
    }

    fn build_rows(&self) -> Vec<GenericDisplayRow> {
        self.sessions
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let is_selected = self.state.selected_idx == Some(idx);
                let prefix = if is_selected { '›' } else { ' ' };
                let marker = if entry.is_current { " (current)" } else { "" };
                let number = idx + 1;
                let title = entry.display_title();
                let name = format!("{prefix} {number}. {title}{marker}");
                GenericDisplayRow {
                    name,
                    display_shortcut: None,
                    match_indices: None,
                    description: self.entry_description(entry),
                    disabled_reason: None,
                    wrap_indent: None,
                }
            })
            .collect()
    }

    fn entry_description(&self, entry: &SessionManagerEntry) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(label) = format_updated_label(entry) {
            parts.push(format!("updated {label}"));
        }
        if let Some(branch) = entry.git_branch.as_deref() {
            parts.push(format!("branch {branch}"));
        }
        if let Some(cwd) = entry.cwd.as_ref() {
            parts.push(display_path_for(cwd, &self.current_cwd));
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" · "))
        }
    }

    fn header_lines(&self) -> Vec<Line<'static>> {
        let title = Line::from("Sessions".bold());
        let subtitle = if let Some(error) = self.error.as_deref() {
            Line::from(format!("Failed to load sessions: {error}").red())
        } else if self.loading {
            Line::from("Loading sessions…".dim())
        } else if self.sessions.is_empty() {
            Line::from("No sessions yet".dim())
        } else {
            Line::from(format!("{count} saved sessions", count = self.sessions.len()).dim())
        };
        vec![title, subtitle]
    }

    fn footer_hint(&self) -> Line<'static> {
        if self.delete_confirm_path.is_some() {
            Line::from(vec![
                "Press ".into(),
                key_hint::plain(KeyCode::Char('d')).into(),
                " again to delete · ".into(),
                key_hint::plain(KeyCode::Esc).into(),
                " cancel".into(),
            ])
        } else {
            Line::from(vec![
                key_hint::plain(KeyCode::Enter).into(),
                " switch · ".into(),
                key_hint::plain(KeyCode::Char('r')).into(),
                " rename · ".into(),
                key_hint::plain(KeyCode::Char('d')).into(),
                " delete · ".into(),
                key_hint::plain(KeyCode::Esc).into(),
                " close".into(),
            ])
        }
    }

    fn rows_width(total_width: u16) -> u16 {
        total_width.saturating_sub(2)
    }

    fn max_visible_rows(len: usize) -> usize {
        MAX_POPUP_ROWS.min(len.max(1))
    }
}

impl BottomPaneView for SessionManagerView {
    fn completion_behavior(&self) -> ViewCompletionBehavior {
        ViewCompletionBehavior::ClearStack
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.on_ctrl_c();
            }
            KeyEvent {
                code: KeyCode::Up, ..
            }
            | KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('\u{0010}'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.move_up(),
            KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.move_up(),
            KeyEvent {
                code: KeyCode::Down,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('\u{000e}'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.move_down(),
            KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.move_down(),
            KeyEvent {
                code: KeyCode::PageUp,
                ..
            } => self.page_up(Self::max_visible_rows(self.sessions.len())),
            KeyEvent {
                code: KeyCode::PageDown,
                ..
            } => self.page_down(Self::max_visible_rows(self.sessions.len())),
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => self.accept(),
            KeyEvent {
                code: KeyCode::Char('r'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.request_rename(),
            KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.request_delete(),
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.complete = true;
        CancellationEvent::Handled
    }
}

impl crate::render::renderable::Renderable for SessionManagerView {
    fn desired_height(&self, width: u16) -> u16 {
        let header_height = self.header_lines().len() as u16;
        let rows = self.build_rows();
        let rows_width = Self::rows_width(width);
        let rows_height = measure_rows_height(
            &rows,
            &self.state,
            MAX_POPUP_ROWS,
            rows_width.saturating_add(1),
        );
        let mut height = header_height.saturating_add(rows_height + 3);
        height = height.saturating_add(1);
        height
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let [content_area, footer_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

        Block::default()
            .style(user_message_style())
            .render(content_area, buf);

        let header_lines = self.header_lines();
        let header_height = header_lines.len() as u16;
        let rows = self.build_rows();
        let rows_width = Self::rows_width(content_area.width);
        let rows_height = measure_rows_height(
            &rows,
            &self.state,
            MAX_POPUP_ROWS,
            rows_width.saturating_add(1),
        );
        let [header_area, _, list_area] = Layout::vertical([
            Constraint::Length(header_height),
            Constraint::Length(1),
            Constraint::Length(rows_height),
        ])
        .areas(content_area.inset(Insets::vh(1, 2)));

        let visible_lines = header_area.height.min(header_height) as usize;
        for (idx, line) in header_lines.into_iter().take(visible_lines).enumerate() {
            let line_area = Rect {
                x: header_area.x,
                y: header_area.y.saturating_add(idx as u16),
                width: header_area.width,
                height: 1,
            };
            line.render(line_area, buf);
        }

        if list_area.height > 0 {
            let render_area = Rect {
                x: list_area.x.saturating_sub(2),
                y: list_area.y,
                width: rows_width.max(1),
                height: list_area.height,
            };
            let empty_message = if self.loading {
                "Loading sessions…"
            } else if self.error.is_some() {
                "Unable to load sessions"
            } else {
                "No sessions yet"
            };
            render_rows(
                render_area,
                buf,
                &rows,
                &self.state,
                render_area.height as usize,
                empty_message,
            );
        }

        let hint_area = Rect {
            x: footer_area.x + 2,
            y: footer_area.y,
            width: footer_area.width.saturating_sub(2),
            height: footer_area.height,
        };
        self.footer_hint().dim().render(hint_area, buf);
    }
}

fn human_time_ago(ts: DateTime<Utc>) -> String {
    let now = Utc::now();
    let delta = now - ts;
    let secs = delta.num_seconds();
    if secs < 60 {
        let n = secs.max(0);
        if n == 1 {
            format!("{n} second ago")
        } else {
            format!("{n} seconds ago")
        }
    } else if secs < 60 * 60 {
        let m = secs / 60;
        if m == 1 {
            format!("{m} minute ago")
        } else {
            format!("{m} minutes ago")
        }
    } else if secs < 60 * 60 * 24 {
        let h = secs / 3600;
        if h == 1 {
            format!("{h} hour ago")
        } else {
            format!("{h} hours ago")
        }
    } else {
        let d = secs / (60 * 60 * 24);
        if d == 1 {
            format!("{d} day ago")
        } else {
            format!("{d} days ago")
        }
    }
}

fn format_updated_label(entry: &SessionManagerEntry) -> Option<String> {
    match (entry.updated_at, entry.created_at) {
        (Some(updated), _) => Some(human_time_ago(updated)),
        (None, Some(created)) => Some(human_time_ago(created)),
        (None, None) => None,
    }
}
