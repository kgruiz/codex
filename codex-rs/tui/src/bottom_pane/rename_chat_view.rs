use std::cell::RefCell;
use std::path::PathBuf;

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui::widgets::StatefulWidgetRef;
use ratatui::widgets::Widget;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::render::renderable::Renderable;

use super::CancellationEvent;
use super::bottom_pane_view::BottomPaneView;
use super::bottom_pane_view::ViewCompletionBehavior;
use super::popup_consts::standard_popup_hint_line;
use super::textarea::TextArea;
use super::textarea::TextAreaState;

pub(crate) struct RenameChatView {
    app_event_tx: AppEventSender,
    textarea: TextArea,
    textarea_state: RefCell<TextAreaState>,
    complete: bool,
    target: RenameTarget,
}

impl RenameChatView {
    pub(crate) fn new(
        app_event_tx: AppEventSender,
        current_title: Option<String>,
        target: RenameTarget,
    ) -> Self {
        let mut textarea = TextArea::new();
        if let Some(title) = current_title {
            textarea.set_text(&title);
            textarea.set_cursor(title.len());
        }
        Self {
            app_event_tx,
            textarea,
            textarea_state: RefCell::new(TextAreaState::default()),
            complete: false,
            target,
        }
    }

    fn submit(&mut self) {
        let text = self.textarea.text().trim().to_string();
        let title = if text.is_empty() { None } else { Some(text) };
        match &self.target {
            RenameTarget::CurrentSession => {
                self.app_event_tx.send(AppEvent::RenameSession { title });
            }
            RenameTarget::SessionPath(path) => {
                self.app_event_tx.send(AppEvent::RenameSessionPath {
                    path: path.clone(),
                    title,
                });
            }
        }
        self.complete = true;
    }
}

impl BottomPaneView for RenameChatView {
    fn completion_behavior(&self) -> ViewCompletionBehavior {
        ViewCompletionBehavior::Pop
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.on_ctrl_c();
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.submit();
            }
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => {}
            other => {
                self.textarea.input(other);
            }
        }
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.complete = true;
        CancellationEvent::Handled
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn handle_paste(&mut self, pasted: String) -> bool {
        if pasted.is_empty() {
            return false;
        }
        self.textarea.insert_str(&pasted);
        true
    }
}

impl Renderable for RenameChatView {
    fn desired_height(&self, width: u16) -> u16 {
        1u16 + self.input_height(width) + 2u16
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let input_height = self.input_height(area.width);

        let title_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };
        let title_spans: Vec<Span<'static>> = vec![gutter(), "Rename chat".bold()];
        Paragraph::new(Line::from(title_spans)).render(title_area, buf);

        let input_area = Rect {
            x: area.x,
            y: area.y.saturating_add(1),
            width: area.width,
            height: input_height,
        };
        if input_area.width >= 2 {
            for row in 0..input_area.height {
                Paragraph::new(Line::from(vec![gutter()])).render(
                    Rect {
                        x: input_area.x,
                        y: input_area.y.saturating_add(row),
                        width: 2,
                        height: 1,
                    },
                    buf,
                );
            }

            let text_area_height = input_area.height.saturating_sub(1);
            if text_area_height > 0 {
                if input_area.width > 2 {
                    let blank_rect = Rect {
                        x: input_area.x.saturating_add(2),
                        y: input_area.y,
                        width: input_area.width.saturating_sub(2),
                        height: 1,
                    };
                    Clear.render(blank_rect, buf);
                }
                let textarea_rect = Rect {
                    x: input_area.x.saturating_add(2),
                    y: input_area.y.saturating_add(1),
                    width: input_area.width.saturating_sub(2),
                    height: text_area_height,
                };
                let mut state = self.textarea_state.borrow_mut();
                StatefulWidgetRef::render_ref(&(&self.textarea), textarea_rect, buf, &mut state);
                if self.textarea.text().is_empty() {
                    Paragraph::new(Line::from("Enter a title".dim())).render(textarea_rect, buf);
                }
            }
        }

        let hint_y = input_area.y.saturating_add(input_height);
        if hint_y < area.y.saturating_add(area.height) {
            Paragraph::new(standard_popup_hint_line()).render(
                Rect {
                    x: area.x,
                    y: hint_y,
                    width: area.width,
                    height: 1,
                },
                buf,
            );
        }
    }

    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        if area.height < 2 || area.width <= 2 {
            return None;
        }
        let text_area_height = self.input_height(area.width).saturating_sub(1);
        if text_area_height == 0 {
            return None;
        }
        let textarea_rect = Rect {
            x: area.x.saturating_add(2),
            y: area.y.saturating_add(2),
            width: area.width.saturating_sub(2),
            height: text_area_height,
        };
        let state = *self.textarea_state.borrow();
        self.textarea.cursor_pos_with_state(textarea_rect, state)
    }
}

impl RenameChatView {
    fn input_height(&self, width: u16) -> u16 {
        let usable_width = width.saturating_sub(2);
        let text_height = self.textarea.desired_height(usable_width).clamp(1, 3);
        text_height.saturating_add(1).min(4)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum RenameTarget {
    CurrentSession,
    SessionPath(PathBuf),
}

fn gutter() -> Span<'static> {
    "▌ ".cyan()
}
