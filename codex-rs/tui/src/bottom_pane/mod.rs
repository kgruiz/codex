//! Bottom pane: shows the ChatComposer or a BottomPaneView, if one is active.
use std::any::Any;
use std::path::Path;
use std::path::PathBuf;

use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::queued_user_messages::QueuedUserMessages;
use crate::bottom_pane::unified_exec_footer::UnifiedExecFooter;
use crate::render::renderable::FlexRenderable;
use crate::render::renderable::Renderable;
use crate::render::renderable::RenderableItem;
use crate::session_manager::SessionManagerEntry;
use crate::tui::FrameRequester;
use bottom_pane_view::BottomPaneView;
pub(crate) use bottom_pane_view::ViewCompletionBehavior;
use codex_core::features::Features;
use codex_core::protocol::SessionMode;
use codex_core::skills::model::SkillMetadata;
use codex_file_search::FileMatch;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::time::Duration;

mod approval_overlay;
pub(crate) use approval_overlay::ApprovalOverlay;
pub(crate) use approval_overlay::ApprovalRequest;
mod bottom_pane_view;
mod chat_composer;
mod chat_composer_history;
mod command_popup;
pub mod custom_prompt_view;
mod experimental_features_view;
mod file_search_popup;
mod footer;
mod list_selection_view;
mod prompt_args;
pub(crate) use prompt_args::parse_positional_args;
pub(crate) use prompt_args::parse_slash_name;
mod skill_popup;
pub(crate) use list_selection_view::SelectionViewParams;
mod feedback_view;
pub(crate) use feedback_view::feedback_selection_params;
pub(crate) use feedback_view::feedback_upload_consent_params;
mod paste_burst;
pub mod popup_consts;
mod queue_popup;
mod queued_user_messages;
mod rename_chat_view;
mod scroll_state;
mod selection_popup_common;
mod session_manager_view;
mod textarea;
mod unified_exec_footer;
pub(crate) use feedback_view::FeedbackNoteView;
pub(crate) use rename_chat_view::RenameChatView;
pub(crate) use rename_chat_view::RenameTarget;
pub(crate) use session_manager_view::SessionManagerView;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancellationEvent {
    Handled,
    NotHandled,
}

pub(crate) use chat_composer::ChatComposer;
pub(crate) use chat_composer::ComposerAttachment;
pub(crate) use chat_composer::InputResult;
use codex_protocol::custom_prompts::CustomPrompt;
use codex_protocol::openai_models::ReasoningEffort;
pub(crate) use queue_popup::QueuePopup;
pub(crate) use queue_popup::QueuePopupItem;

use crate::status_indicator_widget::StatusIndicatorWidget;
use codex_core::config::StatusLineItem;
pub(crate) use experimental_features_view::BetaFeatureItem;
pub(crate) use experimental_features_view::ExperimentalFeaturesView;
pub(crate) use footer::StatusLineMetrics;
pub(crate) use list_selection_view::SelectionAction;
pub(crate) use list_selection_view::SelectionItem;

/// Pane displayed in the lower half of the chat UI.
pub(crate) struct BottomPane {
    /// Composer is retained even when a BottomPaneView is displayed so the
    /// input state is retained when the view is closed.
    composer: ChatComposer,

    /// Stack of views displayed instead of the composer (e.g. popups/modals).
    view_stack: Vec<Box<dyn BottomPaneView>>,

    app_event_tx: AppEventSender,
    frame_requester: FrameRequester,

    has_input_focus: bool,
    is_task_running: bool,
    ctrl_c_quit_hint: bool,
    esc_backtrack_hint: bool,
    animations_enabled: bool,

    /// Inline status indicator shown above the composer while a task is running.
    status: Option<StatusIndicatorWidget>,
    /// Unified exec session summary shown above the composer.
    unified_exec_footer: UnifiedExecFooter,
    /// Queued user messages to show above the composer while a turn is running.
    queued_user_messages: QueuedUserMessages,
    context_window_percent: Option<i64>,
    context_window_used_tokens: Option<i64>,
}

pub(crate) struct BottomPaneParams {
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) frame_requester: FrameRequester,
    pub(crate) has_input_focus: bool,
    pub(crate) enhanced_keys_supported: bool,
    pub(crate) placeholder_text: String,
    pub(crate) disable_paste_burst: bool,
    pub(crate) animations_enabled: bool,
    pub(crate) skills: Option<Vec<SkillMetadata>>,
    pub(crate) keybindings: crate::keybindings::Keybindings,
    pub(crate) status_line_items: Vec<StatusLineItem>,
    pub(crate) status_line_cwd: PathBuf,
}

impl BottomPane {
    pub fn new(params: BottomPaneParams) -> Self {
        let BottomPaneParams {
            app_event_tx,
            frame_requester,
            has_input_focus,
            enhanced_keys_supported,
            placeholder_text,
            disable_paste_burst,
            animations_enabled,
            skills,
            keybindings,
            status_line_items,
            status_line_cwd,
        } = params;
        let mut composer = ChatComposer::new(
            has_input_focus,
            app_event_tx.clone(),
            enhanced_keys_supported,
            placeholder_text,
            disable_paste_burst,
            keybindings,
        );
        composer.set_skill_mentions(skills);
        composer.set_status_line_items(status_line_items);
        composer.set_status_line_cwd(Some(status_line_cwd));

        Self {
            composer,
            view_stack: Vec::new(),
            app_event_tx,
            frame_requester,
            has_input_focus,
            is_task_running: false,
            ctrl_c_quit_hint: false,
            status: None,
            unified_exec_footer: UnifiedExecFooter::new(),
            queued_user_messages: QueuedUserMessages::new(),
            esc_backtrack_hint: false,
            animations_enabled,
            context_window_percent: None,
            context_window_used_tokens: None,
        }
    }

    fn refresh_queued_user_message_hints(&mut self) -> bool {
        let queue_edit_active = self
            .queued_user_messages
            .messages
            .iter()
            .any(|message| message.starts_with("✎ "));
        let show_send_next_hint = !self.is_task_running
            && !self.composer.popup_active()
            && !queue_edit_active
            && !self.queued_user_messages.messages.is_empty();

        if self.queued_user_messages.show_send_next_hint != show_send_next_hint {
            self.queued_user_messages.show_send_next_hint = show_send_next_hint;
            return true;
        }

        false
    }

    pub fn set_skills(&mut self, skills: Option<Vec<SkillMetadata>>) {
        self.composer.set_skill_mentions(skills);
        self.request_redraw();
    }

    pub fn status_widget(&self) -> Option<&StatusIndicatorWidget> {
        self.status.as_ref()
    }

    pub fn skills(&self) -> Option<&Vec<SkillMetadata>> {
        self.composer.skills()
    }

    #[cfg(test)]
    pub(crate) fn context_window_percent(&self) -> Option<i64> {
        self.context_window_percent
    }

    #[cfg(test)]
    pub(crate) fn context_window_used_tokens(&self) -> Option<i64> {
        self.context_window_used_tokens
    }

    fn active_view(&self) -> Option<&dyn BottomPaneView> {
        self.view_stack.last().map(std::convert::AsRef::as_ref)
    }

    fn push_view(&mut self, view: Box<dyn BottomPaneView>) {
        self.view_stack.push(view);
        self.request_redraw();
    }

    fn dismiss_completed_view(&mut self, behavior: ViewCompletionBehavior) {
        match behavior {
            ViewCompletionBehavior::Pop => {
                self.view_stack.pop();
            }
            ViewCompletionBehavior::ClearStack => {
                self.view_stack.clear();
            }
        }

        if self.view_stack.is_empty() {
            self.on_active_view_complete();
        }
    }

    /// Forward a key event to the active view or the composer.
    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> InputResult {
        // If a modal/view is active, handle it here; otherwise forward to composer.
        if let Some(view) = self.view_stack.last_mut() {
            let mut completion_behavior: Option<ViewCompletionBehavior> = None;

            if key_event.code == KeyCode::Esc
                && matches!(view.on_ctrl_c(), CancellationEvent::Handled)
                && view.is_complete()
            {
                completion_behavior = Some(ViewCompletionBehavior::Pop);
            } else {
                view.handle_key_event(key_event);
                if view.is_complete() {
                    completion_behavior = Some(view.completion_behavior());
                }
            }

            if let Some(behavior) = completion_behavior {
                self.dismiss_completed_view(behavior);
            }
            self.request_redraw();
            InputResult::None
        } else {
            // If a task is running and a status line is visible, allow Esc to
            // send an interrupt even while the composer has focus.
            if matches!(key_event.code, crossterm::event::KeyCode::Esc)
                && self.is_task_running
                && let Some(status) = &self.status
            {
                // Send Op::Interrupt
                status.interrupt();
                self.request_redraw();
                return InputResult::None;
            }
            let (input_result, needs_redraw) = self.composer.handle_key_event(key_event);
            let hints_changed = self.refresh_queued_user_message_hints();
            if needs_redraw || hints_changed {
                self.request_redraw();
            }
            if self.composer.is_in_paste_burst() {
                self.request_redraw_in(ChatComposer::recommended_paste_flush_delay());
            }
            input_result
        }
    }

    /// Handle Ctrl-C in the bottom pane. If a modal view is active it gets a
    /// chance to consume the event (e.g. to dismiss itself).
    pub(crate) fn on_ctrl_c(&mut self) -> CancellationEvent {
        if let Some(view) = self.view_stack.last_mut() {
            let event = view.on_ctrl_c();
            if matches!(event, CancellationEvent::Handled) {
                if view.is_complete() {
                    self.view_stack.pop();
                    if self.view_stack.is_empty() {
                        self.on_active_view_complete();
                    }
                }
                self.show_ctrl_c_quit_hint();
            }
            event
        } else if self.composer_is_empty() {
            CancellationEvent::NotHandled
        } else {
            self.view_stack.pop();
            self.clear_composer_for_ctrl_c();
            self.show_ctrl_c_quit_hint();
            CancellationEvent::Handled
        }
    }

    pub fn handle_paste(&mut self, pasted: String) {
        if let Some(view) = self.view_stack.last_mut() {
            let needs_redraw = view.handle_paste(pasted);
            let completion_behavior = view.is_complete().then(|| view.completion_behavior());
            if let Some(behavior) = completion_behavior {
                self.dismiss_completed_view(behavior);
            }
            if needs_redraw || completion_behavior.is_some() {
                self.request_redraw();
            }
        } else {
            let needs_redraw = self.composer.handle_paste(pasted);
            if needs_redraw {
                self.request_redraw();
            }
        }
    }

    pub(crate) fn insert_str(&mut self, text: &str) {
        self.composer.insert_str(text);
        self.request_redraw();
    }

    /// Replace the composer text with `text`.
    pub(crate) fn set_composer_text(&mut self, text: String) {
        self.composer.set_text_content(text);
        self.request_redraw();
    }

    pub(crate) fn set_composer_text_with_attachments(
        &mut self,
        text: String,
        attachments: Vec<ComposerAttachment>,
    ) {
        self.composer
            .set_text_content_with_attachments(text, attachments);
        self.request_redraw();
    }

    pub(crate) fn composer_attachments(&self) -> Vec<ComposerAttachment> {
        self.composer.current_attachments()
    }

    pub(crate) fn set_composer_commands_enabled(&mut self, enabled: bool) {
        self.composer.set_commands_enabled(enabled);
        self.request_redraw();
    }

    pub(crate) fn set_composer_footer_hint_override(
        &mut self,
        items: Option<Vec<(String, String)>>,
    ) {
        self.composer.set_footer_hint_override(items);
        self.request_redraw();
    }

    pub(crate) fn set_status_line_git_branch(&mut self, branch: Option<String>) {
        self.composer.set_status_line_git_branch(branch);
        self.request_redraw();
    }

    pub(crate) fn set_status_line_metrics(&mut self, metrics: StatusLineMetrics) {
        self.composer.set_status_line_metrics(metrics);
        self.request_redraw();
    }

    pub(crate) fn clear_composer_for_ctrl_c(&mut self) {
        self.composer.clear_for_ctrl_c();
        self.request_redraw();
    }

    /// Get the current composer text (for tests and programmatic checks).
    pub(crate) fn composer_text(&self) -> String {
        self.composer.current_text()
    }

    /// Update the animated header shown to the left of the brackets in the
    /// status indicator (defaults to "Working"). No-ops if the status
    /// indicator is not active.
    pub(crate) fn update_status_header(&mut self, header: String) {
        if let Some(status) = self.status.as_mut() {
            status.update_header(header);
            self.request_redraw();
        }
    }

    pub(crate) fn set_session_model(&mut self, model: String) {
        if self.composer.set_session_model(model) {
            self.request_redraw();
        }
    }

    pub(crate) fn set_session_mode(&mut self, mode: SessionMode) {
        if self.composer.set_session_mode(mode) {
            self.request_redraw();
        }
    }

    pub(crate) fn set_session_reasoning_effort(&mut self, effort: Option<ReasoningEffort>) {
        if self.composer.set_session_reasoning_effort(effort) {
            self.request_redraw();
        }
    }

    pub(crate) fn set_active_model(&mut self, model: Option<String>) {
        if let Some(status) = self.status.as_mut() {
            status.set_active_model(model);
            self.request_redraw();
        }
    }

    pub(crate) fn set_active_reasoning_effort(&mut self, effort: Option<ReasoningEffort>) {
        if let Some(status) = self.status.as_mut() {
            status.set_active_reasoning_effort(effort);
            self.request_redraw();
        }
    }

    pub(crate) fn show_ctrl_c_quit_hint(&mut self) {
        self.ctrl_c_quit_hint = true;
        self.composer
            .set_ctrl_c_quit_hint(true, self.has_input_focus);
        self.request_redraw();
    }

    pub(crate) fn clear_ctrl_c_quit_hint(&mut self) {
        if self.ctrl_c_quit_hint {
            self.ctrl_c_quit_hint = false;
            self.composer
                .set_ctrl_c_quit_hint(false, self.has_input_focus);
            self.request_redraw();
        }
    }

    #[cfg(test)]
    pub(crate) fn ctrl_c_quit_hint_visible(&self) -> bool {
        self.ctrl_c_quit_hint
    }

    #[cfg(test)]
    pub(crate) fn status_indicator_visible(&self) -> bool {
        self.status.is_some()
    }

    pub(crate) fn show_esc_backtrack_hint(&mut self) {
        self.esc_backtrack_hint = true;
        self.composer.set_esc_backtrack_hint(true);
        self.request_redraw();
    }

    pub(crate) fn clear_esc_backtrack_hint(&mut self) {
        if self.esc_backtrack_hint {
            self.esc_backtrack_hint = false;
            self.composer.set_esc_backtrack_hint(false);
            self.request_redraw();
        }
    }

    // esc_backtrack_hint_visible removed; hints are controlled internally.

    pub fn set_task_running(&mut self, running: bool) {
        let was_running = self.is_task_running;
        self.is_task_running = running;
        self.composer.set_task_running(running);
        let hints_changed = self.refresh_queued_user_message_hints();

        if running {
            if !was_running {
                if self.status.is_none() {
                    self.status = Some(StatusIndicatorWidget::new(
                        self.app_event_tx.clone(),
                        self.frame_requester.clone(),
                        self.animations_enabled,
                    ));
                }
                if let Some(status) = self.status.as_mut() {
                    status.set_interrupt_hint_visible(true);
                }
                self.request_redraw();
            } else if hints_changed {
                self.request_redraw();
            }
        } else {
            // Hide the status indicator when a task completes, but keep other modal views.
            self.hide_status_indicator();
            if hints_changed {
                self.request_redraw();
            }
        }
    }

    /// Hide the status indicator while leaving task-running state untouched.
    pub(crate) fn hide_status_indicator(&mut self) {
        if self.status.take().is_some() {
            self.request_redraw();
        }
    }

    pub(crate) fn ensure_status_indicator(&mut self) {
        if self.status.is_none() {
            self.status = Some(StatusIndicatorWidget::new(
                self.app_event_tx.clone(),
                self.frame_requester.clone(),
                self.animations_enabled,
            ));
            self.request_redraw();
        }
    }

    pub(crate) fn set_interrupt_hint_visible(&mut self, visible: bool) {
        if let Some(status) = self.status.as_mut() {
            status.set_interrupt_hint_visible(visible);
            self.request_redraw();
        }
    }

    pub(crate) fn set_context_window(&mut self, percent: Option<i64>, used_tokens: Option<i64>) {
        if self.context_window_percent == percent && self.context_window_used_tokens == used_tokens
        {
            return;
        }

        self.context_window_percent = percent;
        self.context_window_used_tokens = used_tokens;
        self.composer
            .set_context_window(percent, self.context_window_used_tokens);
        self.request_redraw();
    }

    /// Show a generic list selection view with the provided items.
    pub(crate) fn show_selection_view(&mut self, params: list_selection_view::SelectionViewParams) {
        let view = list_selection_view::ListSelectionView::new(params, self.app_event_tx.clone());
        self.push_view(Box::new(view));
    }

    /// Update the queued messages preview shown above the composer.
    pub(crate) fn set_queued_user_messages(&mut self, queued: Vec<String>) {
        self.queued_user_messages.messages = queued;
        self.refresh_queued_user_message_hints();
        self.request_redraw();
    }

    pub(crate) fn set_unified_exec_sessions(&mut self, sessions: Vec<String>) {
        if self.unified_exec_footer.set_sessions(sessions) {
            self.request_redraw();
        }
    }

    pub(crate) fn update_queue_popup_items(&mut self, items: Vec<QueuePopupItem>) {
        for view in self.view_stack.iter_mut().rev() {
            if let Some(popup) = (view.as_mut() as &mut dyn Any).downcast_mut::<QueuePopup>() {
                popup.set_items(items);
                self.request_redraw();
                return;
            }
        }
    }

    pub(crate) fn update_session_manager_sessions(&mut self, sessions: Vec<SessionManagerEntry>) {
        for view in self.view_stack.iter_mut().rev() {
            if let Some(manager) =
                (view.as_mut() as &mut dyn Any).downcast_mut::<SessionManagerView>()
            {
                manager.set_sessions(sessions);
                self.request_redraw();
                return;
            }
        }
    }

    pub(crate) fn set_session_manager_error(&mut self, message: String) {
        for view in self.view_stack.iter_mut().rev() {
            if let Some(manager) =
                (view.as_mut() as &mut dyn Any).downcast_mut::<SessionManagerView>()
            {
                manager.set_error(message);
                self.request_redraw();
                return;
            }
        }
    }

    pub(crate) fn apply_session_manager_rename(&mut self, path: &Path, title: Option<String>) {
        for view in self.view_stack.iter_mut().rev() {
            if let Some(manager) =
                (view.as_mut() as &mut dyn Any).downcast_mut::<SessionManagerView>()
            {
                if manager.apply_rename(path, title) {
                    self.request_redraw();
                }
                return;
            }
        }
    }

    pub(crate) fn apply_session_manager_delete(&mut self, path: &Path) {
        for view in self.view_stack.iter_mut().rev() {
            if let Some(manager) =
                (view.as_mut() as &mut dyn Any).downcast_mut::<SessionManagerView>()
            {
                if manager.apply_delete(path) {
                    self.request_redraw();
                }
                return;
            }
        }
    }

    /// Update custom prompts available for the slash popup.
    pub(crate) fn set_custom_prompts(&mut self, prompts: Vec<CustomPrompt>) {
        self.composer.set_custom_prompts(prompts);
        self.request_redraw();
    }

    pub(crate) fn composer_is_empty(&self) -> bool {
        self.composer.is_empty()
    }

    pub(crate) fn is_task_running(&self) -> bool {
        self.is_task_running
    }

    pub(crate) fn has_active_view(&self) -> bool {
        !self.view_stack.is_empty()
    }

    pub(crate) fn composer_popup_active(&self) -> bool {
        self.composer.popup_active()
    }

    /// Return true when the pane is in the regular composer state without any
    /// overlays or popups and not running a task. This is the safe context to
    /// use Esc-Esc for backtracking from the main view.
    pub(crate) fn is_normal_backtrack_mode(&self) -> bool {
        !self.is_task_running && self.view_stack.is_empty() && !self.composer.popup_active()
    }

    pub(crate) fn show_view(&mut self, view: Box<dyn BottomPaneView>) {
        self.push_view(view);
    }

    /// Called when the agent requests user approval.
    pub fn push_approval_request(&mut self, request: ApprovalRequest, features: &Features) {
        let request = if let Some(view) = self.view_stack.last_mut() {
            match view.try_consume_approval_request(request) {
                Some(request) => request,
                None => {
                    self.request_redraw();
                    return;
                }
            }
        } else {
            request
        };

        // Otherwise create a new approval modal overlay.
        let modal = ApprovalOverlay::new(request, self.app_event_tx.clone(), features.clone());
        self.pause_status_timer_for_modal();
        self.push_view(Box::new(modal));
    }

    fn on_active_view_complete(&mut self) {
        self.resume_status_timer_after_modal();
    }

    fn pause_status_timer_for_modal(&mut self) {
        if let Some(status) = self.status.as_mut() {
            status.pause_timer();
        }
    }

    fn resume_status_timer_after_modal(&mut self) {
        if let Some(status) = self.status.as_mut() {
            status.resume_timer();
        }
    }

    /// Height (terminal rows) required by the current bottom pane.
    pub(crate) fn request_redraw(&self) {
        self.frame_requester.schedule_frame();
    }

    pub(crate) fn request_redraw_in(&self, dur: Duration) {
        self.frame_requester.schedule_frame_in(dur);
    }

    // --- History helpers ---

    pub(crate) fn set_history_metadata(&mut self, log_id: u64, entry_count: usize) {
        self.composer.set_history_metadata(log_id, entry_count);
    }

    pub(crate) fn flush_paste_burst_if_due(&mut self) -> bool {
        self.composer.flush_paste_burst_if_due()
    }

    pub(crate) fn is_in_paste_burst(&self) -> bool {
        self.composer.is_in_paste_burst()
    }

    pub(crate) fn on_history_entry_response(
        &mut self,
        log_id: u64,
        offset: usize,
        entry: Option<String>,
    ) {
        let updated = self
            .composer
            .on_history_entry_response(log_id, offset, entry);

        if updated {
            self.request_redraw();
        }
    }

    pub(crate) fn on_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        self.composer.on_file_search_result(query, matches);
        self.request_redraw();
    }

    pub(crate) fn attach_image(
        &mut self,
        path: PathBuf,
        width: u32,
        height: u32,
        format_label: &str,
    ) {
        if self.view_stack.is_empty() {
            self.composer
                .attach_image(path, width, height, format_label);
            self.request_redraw();
        }
    }

    #[cfg(test)]
    pub(crate) fn take_recent_submission_images(&mut self) -> Vec<PathBuf> {
        self.composer.take_recent_submission_images()
    }

    pub(crate) fn take_recent_submission_attachments(&mut self) -> Vec<ComposerAttachment> {
        self.composer.take_recent_submission_attachments()
    }

    pub(crate) fn take_last_command_input(&mut self) -> Option<String> {
        self.composer.take_last_command_input()
    }

    fn as_renderable(&'_ self) -> RenderableItem<'_> {
        if let Some(view) = self.active_view() {
            RenderableItem::Borrowed(view)
        } else {
            let mut flex = FlexRenderable::new();
            if let Some(status) = &self.status {
                flex.push(0, RenderableItem::Borrowed(status));
            }
            if !self.unified_exec_footer.is_empty() {
                flex.push(0, RenderableItem::Borrowed(&self.unified_exec_footer));
            }
            flex.push(1, RenderableItem::Borrowed(&self.queued_user_messages));
            if self.status.is_some()
                || !self.unified_exec_footer.is_empty()
                || !self.queued_user_messages.messages.is_empty()
            {
                flex.push(0, RenderableItem::Owned("".into()));
            }
            let mut flex2 = FlexRenderable::new();
            flex2.push(1, RenderableItem::Owned(flex.into()));
            flex2.push(0, RenderableItem::Borrowed(&self.composer));
            RenderableItem::Owned(Box::new(flex2))
        }
    }
}

impl Renderable for BottomPane {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.as_renderable().render(area, buf);
    }
    fn desired_height(&self, width: u16) -> u16 {
        self.as_renderable().desired_height(width)
    }
    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.as_renderable().cursor_pos(area)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use codex_core::config::StatusLineItem;
    use insta::assert_snapshot;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use tokio::sync::mpsc::unbounded_channel;

    fn default_keybindings(enhanced_keys_supported: bool) -> crate::keybindings::Keybindings {
        crate::keybindings::Keybindings::from_config(
            &HashMap::new(),
            enhanced_keys_supported,
            false,
        )
    }

    fn default_status_line_items() -> Vec<StatusLineItem> {
        vec![
            StatusLineItem::Model,
            StatusLineItem::Context,
            StatusLineItem::TokensPerSec,
            StatusLineItem::Latency,
            StatusLineItem::ToolTime,
            StatusLineItem::Cost,
        ]
    }

    fn default_status_line_cwd() -> PathBuf {
        PathBuf::from("/test")
    }

    fn snapshot_buffer(buf: &Buffer) -> String {
        let mut lines = Vec::new();
        for y in 0..buf.area().height {
            let mut row = String::new();
            for x in 0..buf.area().width {
                row.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            lines.push(row);
        }
        lines.join("\n")
    }

    fn render_snapshot(pane: &BottomPane, area: Rect) -> String {
        let mut buf = Buffer::empty(area);
        pane.render(area, &mut buf);
        snapshot_buffer(&buf)
    }

    fn exec_request() -> ApprovalRequest {
        ApprovalRequest::Exec {
            id: "1".to_string(),
            command: vec!["echo".into(), "ok".into()],
            reason: None,
            proposed_execpolicy_amendment: None,
        }
    }

    #[test]
    fn ctrl_c_on_modal_consumes_and_shows_quit_hint() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let features = Features::with_defaults();
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            frame_requester: FrameRequester::test_dummy(),
            has_input_focus: true,
            enhanced_keys_supported: false,
            placeholder_text: "Ask Codex to do anything".to_string(),
            disable_paste_burst: false,
            animations_enabled: true,
            skills: Some(Vec::new()),
            keybindings: default_keybindings(false),
            status_line_items: default_status_line_items(),
            status_line_cwd: default_status_line_cwd(),
        });
        pane.push_approval_request(exec_request(), &features);
        assert_eq!(CancellationEvent::Handled, pane.on_ctrl_c());
        assert!(pane.ctrl_c_quit_hint_visible());
        assert_eq!(CancellationEvent::NotHandled, pane.on_ctrl_c());
    }

    // live ring removed; related tests deleted.

    #[test]
    fn overlay_not_shown_above_approval_modal() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let features = Features::with_defaults();
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            frame_requester: FrameRequester::test_dummy(),
            has_input_focus: true,
            enhanced_keys_supported: false,
            placeholder_text: "Ask Codex to do anything".to_string(),
            disable_paste_burst: false,
            animations_enabled: true,
            skills: Some(Vec::new()),
            keybindings: default_keybindings(false),
            status_line_items: default_status_line_items(),
            status_line_cwd: default_status_line_cwd(),
        });

        // Create an approval modal (active view).
        pane.push_approval_request(exec_request(), &features);

        // Render and verify the top row does not include an overlay.
        let area = Rect::new(0, 0, 60, 6);
        let mut buf = Buffer::empty(area);
        pane.render(area, &mut buf);

        let mut r0 = String::new();
        for x in 0..area.width {
            r0.push(buf[(x, 0)].symbol().chars().next().unwrap_or(' '));
        }
        assert!(
            !r0.contains("Working"),
            "overlay should not render above modal"
        );
    }

    #[test]
    fn composer_shown_after_denied_while_task_running() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let features = Features::with_defaults();
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            frame_requester: FrameRequester::test_dummy(),
            has_input_focus: true,
            enhanced_keys_supported: false,
            placeholder_text: "Ask Codex to do anything".to_string(),
            disable_paste_burst: false,
            animations_enabled: true,
            skills: Some(Vec::new()),
            keybindings: default_keybindings(false),
            status_line_items: default_status_line_items(),
            status_line_cwd: default_status_line_cwd(),
        });

        // Start a running task so the status indicator is active above the composer.
        pane.set_task_running(true);

        // Push an approval modal (e.g., command approval) which should hide the status view.
        pane.push_approval_request(exec_request(), &features);

        // Simulate pressing 'n' (No) on the modal.
        use crossterm::event::KeyCode;
        use crossterm::event::KeyEvent;
        use crossterm::event::KeyModifiers;
        pane.handle_key_event(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));

        // After denial, since the task is still running, the status indicator should be
        // visible above the composer. The modal should be gone.
        assert!(
            pane.view_stack.is_empty(),
            "no active modal view after denial"
        );

        // Render and ensure the top row includes the Working header and a composer line below.
        // Give the animation thread a moment to tick.
        std::thread::sleep(Duration::from_millis(120));
        let area = Rect::new(0, 0, 40, 6);
        let mut buf = Buffer::empty(area);
        pane.render(area, &mut buf);
        let mut row0 = String::new();
        for x in 0..area.width {
            row0.push(buf[(x, 0)].symbol().chars().next().unwrap_or(' '));
        }
        assert!(
            row0.contains("Working"),
            "expected Working header after denial on row 0: {row0:?}"
        );

        // Composer placeholder should be visible somewhere below.
        let mut found_composer = false;
        for y in 1..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            if row.contains("Ask Codex") {
                found_composer = true;
                break;
            }
        }
        assert!(
            found_composer,
            "expected composer visible under status line"
        );
    }

    #[test]
    fn status_indicator_visible_during_command_execution() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            frame_requester: FrameRequester::test_dummy(),
            has_input_focus: true,
            enhanced_keys_supported: false,
            placeholder_text: "Ask Codex to do anything".to_string(),
            disable_paste_burst: false,
            animations_enabled: true,
            skills: Some(Vec::new()),
            keybindings: default_keybindings(false),
            status_line_items: default_status_line_items(),
            status_line_cwd: default_status_line_cwd(),
        });

        // Begin a task: show initial status.
        pane.set_task_running(true);

        // Use a height that allows the status line to be visible above the composer.
        let area = Rect::new(0, 0, 40, 6);
        let mut buf = Buffer::empty(area);
        pane.render(area, &mut buf);

        let bufs = snapshot_buffer(&buf);
        assert!(bufs.contains("• Working"), "expected Working header");
    }

    #[test]
    fn status_and_composer_fill_height_without_bottom_padding() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            frame_requester: FrameRequester::test_dummy(),
            has_input_focus: true,
            enhanced_keys_supported: false,
            placeholder_text: "Ask Codex to do anything".to_string(),
            disable_paste_burst: false,
            animations_enabled: true,
            skills: Some(Vec::new()),
            keybindings: default_keybindings(false),
            status_line_items: default_status_line_items(),
            status_line_cwd: default_status_line_cwd(),
        });

        // Activate spinner (status view replaces composer) with no live ring.
        pane.set_task_running(true);

        // Use height == desired_height; expect spacer + status + composer rows without trailing padding.
        let height = pane.desired_height(30);
        assert!(
            height >= 3,
            "expected at least 3 rows to render spacer, status, and composer; got {height}"
        );
        let area = Rect::new(0, 0, 30, height);
        assert_snapshot!(
            "status_and_composer_fill_height_without_bottom_padding",
            render_snapshot(&pane, area)
        );
    }

    #[test]
    fn queued_messages_visible_when_status_hidden_snapshot() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            frame_requester: FrameRequester::test_dummy(),
            has_input_focus: true,
            enhanced_keys_supported: false,
            placeholder_text: "Ask Codex to do anything".to_string(),
            disable_paste_burst: false,
            animations_enabled: true,
            skills: Some(Vec::new()),
            keybindings: default_keybindings(false),
            status_line_items: default_status_line_items(),
            status_line_cwd: default_status_line_cwd(),
        });

        pane.set_task_running(true);
        pane.set_queued_user_messages(vec!["Queued follow-up question".to_string()]);
        pane.hide_status_indicator();

        let width = 48;
        let height = pane.desired_height(width);
        let area = Rect::new(0, 0, width, height);
        assert_snapshot!(
            "queued_messages_visible_when_status_hidden_snapshot",
            render_snapshot(&pane, area)
        );
    }

    #[test]
    fn status_and_queued_messages_snapshot() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            frame_requester: FrameRequester::test_dummy(),
            has_input_focus: true,
            enhanced_keys_supported: false,
            placeholder_text: "Ask Codex to do anything".to_string(),
            disable_paste_burst: false,
            animations_enabled: true,
            skills: Some(Vec::new()),
            keybindings: default_keybindings(false),
            status_line_items: default_status_line_items(),
            status_line_cwd: default_status_line_cwd(),
        });

        pane.set_task_running(true);
        pane.set_queued_user_messages(vec!["Queued follow-up question".to_string()]);

        let width = 48;
        let height = pane.desired_height(width);
        let area = Rect::new(0, 0, width, height);
        assert_snapshot!(
            "status_and_queued_messages_snapshot",
            render_snapshot(&pane, area)
        );
    }
}
