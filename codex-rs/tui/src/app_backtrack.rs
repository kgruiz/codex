use std::any::TypeId;
use std::path::PathBuf;
use std::sync::Arc;

use crate::app::App;
use crate::chatwidget::QueueSnapshot;
use crate::history_cell::SessionInfoCell;
use crate::history_cell::UserHistoryCell;
use crate::pager_overlay::Overlay;
use crate::tui;
use crate::tui::TuiEvent;
use codex_core::protocol::ConversationPathResponseEvent;
use codex_protocol::ConversationId;
use color_eyre::eyre::Result;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BacktrackAction {
    Branch,
    EditInPlace,
}

#[derive(Clone, Debug)]
pub(crate) struct BacktrackSelection {
    pub(crate) base_id: ConversationId,
    pub(crate) nth_user_message: usize,
    pub(crate) prefill: String,
    pub(crate) rollout_path: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingBacktrack {
    pub(crate) selection: BacktrackSelection,
    pub(crate) action: BacktrackAction,
    pub(crate) queue_snapshot: Option<QueueSnapshot>,
}

/// Aggregates all backtrack-related state used by the App.
#[derive(Default)]
pub(crate) struct BacktrackState {
    /// True when Esc has primed backtrack mode in the main view.
    pub(crate) primed: bool,
    /// Session id of the base conversation to fork from.
    pub(crate) base_id: Option<ConversationId>,
    /// Index in the transcript of the last user message.
    pub(crate) nth_user_message: usize,
    /// True when the transcript overlay is showing a backtrack preview.
    pub(crate) overlay_preview_active: bool,
    /// Pending backtrack action selected from the action picker.
    pub(crate) pending: Option<PendingBacktrack>,
    /// Pending selection awaiting user action.
    pub(crate) pending_selection: Option<BacktrackSelection>,
}

#[derive(Clone, Debug)]
pub(crate) struct EditVersion {
    pub(crate) conversation_id: ConversationId,
    pub(crate) rollout_path: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct EditVersionGroup {
    pub(crate) nth_user_message: usize,
    pub(crate) versions: Vec<EditVersion>,
    pub(crate) active_version_idx: usize,
}

#[derive(Default)]
pub(crate) struct EditVersionState {
    pub(crate) groups: Vec<EditVersionGroup>,
    pub(crate) last_target_nth: Option<usize>,
}

impl App {
    /// Route overlay events when transcript overlay is active.
    /// - If backtrack preview is active: Esc steps back, Shift+Esc steps forward; Enter confirms.
    /// - Otherwise: Esc begins preview; all other events forward to overlay.
    ///   interactions (Esc to step target, Enter to confirm) and overlay lifecycle.
    pub(crate) async fn handle_backtrack_overlay_event(
        &mut self,
        tui: &mut tui::Tui,
        event: TuiEvent,
    ) -> Result<bool> {
        if self.backtrack.overlay_preview_active {
            match event {
                TuiEvent::Key(KeyEvent {
                    code: KeyCode::Esc,
                    modifiers: KeyModifiers::SHIFT,
                    kind: KeyEventKind::Press | KeyEventKind::Repeat,
                    ..
                }) => {
                    self.overlay_step_backtrack_forward(tui, event)?;
                    Ok(true)
                }
                TuiEvent::Key(KeyEvent {
                    code: KeyCode::Esc,
                    kind: KeyEventKind::Press | KeyEventKind::Repeat,
                    ..
                }) => {
                    self.overlay_step_backtrack(tui, event)?;
                    Ok(true)
                }
                TuiEvent::Key(KeyEvent {
                    code: KeyCode::Enter,
                    kind: KeyEventKind::Press,
                    ..
                }) => {
                    self.overlay_confirm_backtrack(tui);
                    Ok(true)
                }
                // Catchall: forward any other events to the overlay widget.
                _ => {
                    self.overlay_forward_event(tui, event)?;
                    Ok(true)
                }
            }
        } else if let TuiEvent::Key(KeyEvent {
            code: KeyCode::Esc,
            kind: KeyEventKind::Press | KeyEventKind::Repeat,
            ..
        }) = event
        {
            // First Esc in transcript overlay: begin backtrack preview at latest user message.
            self.begin_overlay_backtrack_preview(tui);
            Ok(true)
        } else {
            // Not in backtrack mode: forward events to the overlay widget.
            self.overlay_forward_event(tui, event)?;
            Ok(true)
        }
    }

    /// Handle global Esc presses for backtracking when no overlay is present.
    pub(crate) fn handle_backtrack_esc_key(&mut self, tui: &mut tui::Tui) {
        if !self.chat_widget.composer_is_empty() {
            return;
        }

        if !self.backtrack.primed {
            self.prime_backtrack();
        } else if self.overlay.is_none() {
            self.open_backtrack_preview(tui);
        } else if self.backtrack.overlay_preview_active {
            self.step_backtrack_and_highlight(tui);
        }
    }

    /// Handle Shift+Esc presses to step forward through backtrack selections.
    pub(crate) fn handle_backtrack_shift_esc_key(&mut self, tui: &mut tui::Tui) {
        if !self.chat_widget.composer_is_empty() {
            return;
        }

        if !self.backtrack.primed {
            self.prime_backtrack();

            return;
        }

        if self.overlay.is_none() {
            self.open_backtrack_preview(tui);

            return;
        }

        if self.backtrack.overlay_preview_active {
            self.step_forward_backtrack_and_highlight(tui);
        }
    }

    pub(crate) fn handle_backtrack_action_selected(&mut self, action: BacktrackAction) {
        let Some(selection) = self.backtrack.pending_selection.take() else {
            return;
        };

        let queue_snapshot = match action {
            BacktrackAction::EditInPlace => self.chat_widget.queue_snapshot(),
            BacktrackAction::Branch => self
                .config
                .keep_queue_on_branch
                .then(|| self.chat_widget.queue_snapshot())
                .flatten(),
        };

        if matches!(action, BacktrackAction::EditInPlace) {
            self.edit_versions.ensure_group(
                selection.base_id,
                selection.nth_user_message,
                selection.rollout_path.clone(),
            );
        }

        self.request_backtrack(selection, action, queue_snapshot);
    }

    pub(crate) async fn switch_edit_version(
        &mut self,
        tui: &mut tui::Tui,
        direction: isize,
    ) -> bool {
        let Some(conversation_id) = self.chat_widget.conversation_id() else {
            return false;
        };

        let Some(group_index) = self.edit_versions.preferred_group_index(&conversation_id) else {
            return false;
        };

        let group = &mut self.edit_versions.groups[group_index];

        if group.versions.len() <= 1 {
            return false;
        }

        let current_idx = group
            .versions
            .iter()
            .position(|version| version.conversation_id == conversation_id)
            .unwrap_or(group.active_version_idx);

        let target_idx = match direction.cmp(&0) {
            std::cmp::Ordering::Less => current_idx.checked_sub(1),
            std::cmp::Ordering::Greater => {
                let next = current_idx.saturating_add(1);

                (next < group.versions.len()).then_some(next)
            }
            std::cmp::Ordering::Equal => Some(current_idx),
        };

        let Some(target_idx) = target_idx else {
            return true;
        };

        if target_idx == current_idx {
            return true;
        }

        group.active_version_idx = target_idx;
        self.edit_versions.last_target_nth = Some(group.nth_user_message);
        let rollout_path = group.versions[target_idx].rollout_path.clone();
        let queue_snapshot = self.chat_widget.queue_snapshot();
        self.reset_transcript_state_for_version_switch();

        if let Err(err) = self
            .switch_to_rollout_path(tui, rollout_path, queue_snapshot)
            .await
        {
            tracing::error!("failed to switch edit version: {err}");
        }

        true
    }

    fn reset_transcript_state_for_version_switch(&mut self) {
        self.transcript_cells.clear();
        self.deferred_history_lines.clear();
        self.has_emitted_history_lines = false;
        self.current_session_user_index = 0;
    }

    async fn switch_to_rollout_path(
        &mut self,
        tui: &mut tui::Tui,
        path: PathBuf,
        queue_snapshot: Option<QueueSnapshot>,
    ) -> Result<()> {
        let model_family = self
            .server
            .get_models_manager()
            .construct_model_family(self.current_model.as_str(), &self.config)
            .await;

        self.shutdown_current_conversation().await;

        match self
            .server
            .resume_conversation_from_rollout(
                self.config.clone(),
                path.clone(),
                self.auth_manager.clone(),
            )
            .await
        {
            Ok(resumed) => {
                let init = crate::chatwidget::ChatWidgetInit {
                    config: self.config.clone(),
                    frame_requester: tui.frame_requester(),
                    app_event_tx: self.app_event_tx.clone(),
                    initial_prompt: None,
                    initial_images: Vec::new(),
                    enhanced_keys_supported: self.enhanced_keys_supported,
                    auth_manager: self.auth_manager.clone(),
                    models_manager: self.server.get_models_manager(),
                    feedback: self.feedback.clone(),
                    is_first_run: false,
                    model_family: model_family.clone(),
                };
                self.chat_widget = crate::chatwidget::ChatWidget::new_from_existing(
                    init,
                    resumed.conversation,
                    resumed.session_configured,
                );
                self.current_model = model_family.get_model_slug().to_string();

                if let Some(snapshot) = queue_snapshot {
                    self.chat_widget.apply_queue_snapshot(snapshot);
                }
            }
            Err(err) => {
                self.chat_widget.add_error_message(format!(
                    "Failed to switch edit version from {path}: {err}",
                    path = path.display()
                ));
            }
        }

        tui.frame_requester().schedule_frame();
        Ok(())
    }

    /// Stage a backtrack action and request conversation history from the agent.
    pub(crate) fn request_backtrack(
        &mut self,
        selection: BacktrackSelection,
        action: BacktrackAction,
        queue_snapshot: Option<QueueSnapshot>,
    ) {
        self.backtrack.pending = Some(PendingBacktrack {
            selection: selection.clone(),
            action,
            queue_snapshot,
        });
        let ev = ConversationPathResponseEvent {
            conversation_id: selection.base_id,
            path: selection.rollout_path,
        };
        self.app_event_tx
            .send(crate::app_event::AppEvent::ConversationHistory(ev));
    }

    /// Open transcript overlay (enters alternate screen and shows full transcript).
    pub(crate) fn open_transcript_overlay(&mut self, tui: &mut tui::Tui) {
        let _ = tui.enter_alt_screen();
        self.overlay = Some(Overlay::new_transcript(self.transcript_cells.clone()));
        tui.frame_requester().schedule_frame();
    }

    /// Close transcript overlay and restore normal UI.
    pub(crate) fn close_transcript_overlay(&mut self, tui: &mut tui::Tui) {
        let _ = tui.leave_alt_screen();
        let was_backtrack = self.backtrack.overlay_preview_active;

        if !self.deferred_history_lines.is_empty() {
            let lines = std::mem::take(&mut self.deferred_history_lines);
            tui.insert_history_lines(lines);
        }

        self.overlay = None;
        self.backtrack.overlay_preview_active = false;

        if was_backtrack {
            // Ensure backtrack state is fully reset when overlay closes (e.g. via 'q').
            self.reset_backtrack_state();
        }
    }

    /// Re-render the full transcript into the terminal scrollback in one call.
    /// Useful when switching sessions to ensure prior history remains visible.
    pub(crate) fn render_transcript_once(&mut self, tui: &mut tui::Tui) {
        if !self.transcript_cells.is_empty() {
            let width = tui.terminal.last_known_screen_size.width;

            for cell in &self.transcript_cells {
                tui.insert_history_lines(cell.display_lines(width));
            }
        }
    }

    /// Initialize backtrack state and show composer hint.
    fn prime_backtrack(&mut self) {
        self.backtrack.primed = true;
        self.backtrack.nth_user_message = usize::MAX;
        self.backtrack.base_id = self.chat_widget.conversation_id();
        self.chat_widget.show_esc_backtrack_hint();
    }

    /// Open overlay and begin backtrack preview flow (first step + highlight).
    fn open_backtrack_preview(&mut self, tui: &mut tui::Tui) {
        self.open_transcript_overlay(tui);
        self.backtrack.overlay_preview_active = true;
        // Composer is hidden by overlay; clear its hint.
        self.chat_widget.clear_esc_backtrack_hint();
        self.step_backtrack_and_highlight(tui);
    }

    /// When overlay is already open, begin preview mode and select latest user message.
    fn begin_overlay_backtrack_preview(&mut self, tui: &mut tui::Tui) {
        self.backtrack.primed = true;
        self.backtrack.base_id = self.chat_widget.conversation_id();
        self.backtrack.overlay_preview_active = true;
        let count = user_count(&self.transcript_cells);

        if let Some(last) = count.checked_sub(1) {
            self.apply_backtrack_selection(last);
        }

        tui.frame_requester().schedule_frame();
    }

    /// Step selection to the next older user message and update overlay.
    fn step_backtrack_and_highlight(&mut self, tui: &mut tui::Tui) {
        let count = user_count(&self.transcript_cells);

        if count == 0 {
            return;
        }

        let last_index = count.saturating_sub(1);
        let next_selection = match self.backtrack.nth_user_message {
            value if value == usize::MAX => last_index,
            0 => 0,
            value => value.saturating_sub(1).min(last_index),
        };

        self.apply_backtrack_selection(next_selection);
        tui.frame_requester().schedule_frame();
    }

    /// Step selection to the next newer user message and update overlay.
    fn step_forward_backtrack_and_highlight(&mut self, tui: &mut tui::Tui) {
        let count = user_count(&self.transcript_cells);

        if count == 0 {
            return;
        }

        let last_index = count.saturating_sub(1);
        let next_selection = match self.backtrack.nth_user_message {
            value if value == usize::MAX => last_index,
            value => value.saturating_add(1).min(last_index),
        };

        self.apply_backtrack_selection(next_selection);
        tui.frame_requester().schedule_frame();
    }

    /// Apply a computed backtrack selection to the overlay and internal counter.
    fn apply_backtrack_selection(&mut self, nth_user_message: usize) {
        if let Some(cell_idx) = nth_user_position(&self.transcript_cells, nth_user_message) {
            self.backtrack.nth_user_message = nth_user_message;

            if let Some(Overlay::Transcript(t)) = &mut self.overlay {
                t.set_highlight_cell(Some(cell_idx));
            }
        } else {
            self.backtrack.nth_user_message = usize::MAX;

            if let Some(Overlay::Transcript(t)) = &mut self.overlay {
                t.set_highlight_cell(None);
            }
        }
    }

    /// Forward any event to the overlay and close it if done.
    fn overlay_forward_event(&mut self, tui: &mut tui::Tui, event: TuiEvent) -> Result<()> {
        if let Some(overlay) = &mut self.overlay {
            overlay.handle_event(tui, event)?;

            if overlay.is_done() {
                self.close_transcript_overlay(tui);
                tui.frame_requester().schedule_frame();
            }
        }

        Ok(())
    }

    /// Handle Enter in overlay backtrack preview: confirm selection and reset state.
    fn overlay_confirm_backtrack(&mut self, tui: &mut tui::Tui) {
        let nth_user_message = self.backtrack.nth_user_message;

        if let Some(base_id) = self.backtrack.base_id {
            let prefill = nth_user_position(&self.transcript_cells, nth_user_message)
                .and_then(|idx| self.transcript_cells.get(idx))
                .and_then(|cell| cell.as_any().downcast_ref::<UserHistoryCell>())
                .map(|c| c.message.clone())
                .unwrap_or_default();
            self.close_transcript_overlay(tui);
            self.stage_backtrack_action_picker(base_id, nth_user_message, prefill);
        }

        self.reset_backtrack_state();
    }

    /// Handle Esc in overlay backtrack preview: step back if armed, else forward.
    fn overlay_step_backtrack(&mut self, tui: &mut tui::Tui, event: TuiEvent) -> Result<()> {
        if self.backtrack.base_id.is_some() {
            self.step_backtrack_and_highlight(tui);
        } else {
            self.overlay_forward_event(tui, event)?;
        }

        Ok(())
    }

    /// Handle Shift+Esc in overlay backtrack preview: step forward if armed, else forward.
    fn overlay_step_backtrack_forward(
        &mut self,
        tui: &mut tui::Tui,
        event: TuiEvent,
    ) -> Result<()> {
        if self.backtrack.base_id.is_some() {
            self.step_forward_backtrack_and_highlight(tui);
        } else {
            self.overlay_forward_event(tui, event)?;
        }

        Ok(())
    }

    /// Confirm a primed backtrack from the main view (no overlay visible).
    /// Computes the prefill from the selected user message and requests history.
    pub(crate) fn confirm_backtrack_from_main(&mut self) {
        if let Some(base_id) = self.backtrack.base_id {
            let prefill =
                nth_user_position(&self.transcript_cells, self.backtrack.nth_user_message)
                    .and_then(|idx| self.transcript_cells.get(idx))
                    .and_then(|cell| cell.as_any().downcast_ref::<UserHistoryCell>())
                    .map(|c| c.message.clone())
                    .unwrap_or_default();
            self.stage_backtrack_action_picker(base_id, self.backtrack.nth_user_message, prefill);
        }

        self.reset_backtrack_state();
    }

    fn stage_backtrack_action_picker(
        &mut self,
        base_id: ConversationId,
        nth_user_message: usize,
        prefill: String,
    ) {
        let Some(rollout_path) = self.chat_widget.rollout_path() else {
            tracing::error!("rollout path unavailable; cannot backtrack");

            return;
        };

        self.backtrack.pending_selection = Some(BacktrackSelection {
            base_id,
            nth_user_message,
            prefill,
            rollout_path,
        });
        self.chat_widget.open_backtrack_action_picker();
    }

    /// Clear all backtrack-related state and composer hints.
    pub(crate) fn reset_backtrack_state(&mut self) {
        self.backtrack.primed = false;
        self.backtrack.base_id = None;
        self.backtrack.nth_user_message = usize::MAX;
        // In case a hint is somehow still visible (e.g., race with overlay open/close).
        self.chat_widget.clear_esc_backtrack_hint();
    }

    /// Handle a ConversationHistory response while a backtrack is pending.
    /// If it matches the primed base session, fork and switch to the new conversation.
    pub(crate) async fn on_conversation_history_for_backtrack(
        &mut self,
        tui: &mut tui::Tui,
        ev: ConversationPathResponseEvent,
    ) -> Result<()> {
        if let Some(pending) = self.backtrack.pending.as_ref()
            && ev.conversation_id == pending.selection.base_id
            && let Some(pending) = self.backtrack.pending.take()
        {
            match pending.action {
                BacktrackAction::Branch => {
                    self.fork_and_switch_to_new_conversation(
                        tui,
                        ev,
                        pending.selection.nth_user_message,
                        pending.selection.prefill,
                        pending.queue_snapshot,
                    )
                    .await;
                }
                BacktrackAction::EditInPlace => {
                    self.fork_and_switch_to_edit_version(tui, ev, pending).await;
                }
            }
        }

        Ok(())
    }

    /// Fork the conversation using provided history and switch UI/state accordingly.
    async fn fork_and_switch_to_new_conversation(
        &mut self,
        tui: &mut tui::Tui,
        ev: ConversationPathResponseEvent,
        nth_user_message: usize,
        prefill: String,
        queue_snapshot: Option<QueueSnapshot>,
    ) {
        let cfg = self.chat_widget.config_ref().clone();
        // Perform the fork via a thin wrapper for clarity/testability.
        let result = self
            .perform_fork(ev.path.clone(), nth_user_message, cfg.clone())
            .await;
        match result {
            Ok(new_conv) => self.install_forked_conversation(
                tui,
                cfg,
                new_conv,
                nth_user_message,
                &prefill,
                queue_snapshot,
            ),
            Err(e) => tracing::error!("error forking conversation: {e:#}"),
        }
    }

    async fn fork_and_switch_to_edit_version(
        &mut self,
        tui: &mut tui::Tui,
        ev: ConversationPathResponseEvent,
        pending: PendingBacktrack,
    ) {
        let selection = pending.selection;
        let cfg = self.chat_widget.config_ref().clone();
        let result = self
            .perform_fork(ev.path.clone(), selection.nth_user_message, cfg.clone())
            .await;
        match result {
            Ok(new_conv) => {
                self.edit_versions.add_version(
                    &selection.base_id,
                    selection.nth_user_message,
                    new_conv.conversation_id,
                    new_conv.session_configured.rollout_path.clone(),
                );
                self.install_forked_conversation(
                    tui,
                    cfg,
                    new_conv,
                    selection.nth_user_message,
                    &selection.prefill,
                    pending.queue_snapshot,
                );
            }
            Err(e) => tracing::error!("error forking conversation: {e:#}"),
        }
    }

    /// Thin wrapper around ConversationManager::fork_conversation.
    async fn perform_fork(
        &self,
        path: PathBuf,
        nth_user_message: usize,
        cfg: codex_core::config::Config,
    ) -> codex_core::error::Result<codex_core::NewConversation> {
        self.server
            .fork_conversation(nth_user_message, cfg, path)
            .await
    }

    /// Install a forked conversation into the ChatWidget and update UI to reflect selection.
    fn install_forked_conversation(
        &mut self,
        tui: &mut tui::Tui,
        cfg: codex_core::config::Config,
        new_conv: codex_core::NewConversation,
        nth_user_message: usize,
        prefill: &str,
        queue_snapshot: Option<QueueSnapshot>,
    ) {
        let conv = new_conv.conversation;
        let session_configured = new_conv.session_configured;
        let model_family = self.chat_widget.get_model_family();
        let init = crate::chatwidget::ChatWidgetInit {
            config: cfg,
            model_family: model_family.clone(),
            frame_requester: tui.frame_requester(),
            app_event_tx: self.app_event_tx.clone(),
            initial_prompt: None,
            initial_images: Vec::new(),
            enhanced_keys_supported: self.enhanced_keys_supported,
            auth_manager: self.auth_manager.clone(),
            models_manager: self.server.get_models_manager(),
            feedback: self.feedback.clone(),
            is_first_run: false,
        };
        self.chat_widget =
            crate::chatwidget::ChatWidget::new_from_existing(init, conv, session_configured);
        self.current_model = model_family.get_model_slug().to_string();
        // Trim transcript up to the selected user message and re-render it.
        self.trim_transcript_for_backtrack(nth_user_message);
        self.current_session_user_index = user_count(&self.transcript_cells);
        self.render_transcript_once(tui);

        if !prefill.is_empty() {
            self.chat_widget.set_composer_text(prefill.to_string());
        }

        if let Some(snapshot) = queue_snapshot {
            self.chat_widget.apply_queue_snapshot(snapshot);
        }

        tui.frame_requester().schedule_frame();
    }

    /// Trim transcript_cells to preserve only content up to the selected user message.
    fn trim_transcript_for_backtrack(&mut self, nth_user_message: usize) {
        trim_transcript_cells_to_nth_user(&mut self.transcript_cells, nth_user_message);
    }
}

impl EditVersionState {
    pub(crate) fn version_marker_for(
        &self,
        conversation_id: &ConversationId,
        nth_user_message: usize,
    ) -> Option<(usize, usize)> {
        let group = self.groups.iter().find(|group| {
            group.nth_user_message == nth_user_message
                && group
                    .versions
                    .iter()
                    .any(|version| version.conversation_id == *conversation_id)
        })?;

        let current_idx = group
            .versions
            .iter()
            .position(|version| version.conversation_id == *conversation_id)
            .unwrap_or(group.active_version_idx);

        Some((current_idx.saturating_add(1), group.versions.len()))
    }

    pub(crate) fn ensure_group(
        &mut self,
        conversation_id: ConversationId,
        nth_user_message: usize,
        rollout_path: PathBuf,
    ) {
        if let Some(group) = self.groups.iter_mut().find(|group| {
            group.nth_user_message == nth_user_message
                && group
                    .versions
                    .iter()
                    .any(|version| version.conversation_id == conversation_id)
        }) {
            if let Some(index) = group
                .versions
                .iter()
                .position(|version| version.conversation_id == conversation_id)
            {
                group.active_version_idx = index;
            }

            self.last_target_nth = Some(nth_user_message);

            return;
        }

        self.groups.push(EditVersionGroup {
            nth_user_message,
            versions: vec![EditVersion {
                conversation_id,
                rollout_path,
            }],
            active_version_idx: 0,
        });
        self.last_target_nth = Some(nth_user_message);
    }

    pub(crate) fn add_version(
        &mut self,
        base_conversation_id: &ConversationId,
        nth_user_message: usize,
        conversation_id: ConversationId,
        rollout_path: PathBuf,
    ) {
        let Some(group) = self.groups.iter_mut().find(|group| {
            group.nth_user_message == nth_user_message
                && group
                    .versions
                    .iter()
                    .any(|version| version.conversation_id == *base_conversation_id)
        }) else {
            return;
        };

        if let Some(index) = group
            .versions
            .iter()
            .position(|version| version.conversation_id == conversation_id)
        {
            group.active_version_idx = index;
            self.last_target_nth = Some(nth_user_message);

            return;
        }

        group.versions.push(EditVersion {
            conversation_id,
            rollout_path,
        });
        group.active_version_idx = group.versions.len().saturating_sub(1);
        self.last_target_nth = Some(nth_user_message);
    }

    pub(crate) fn preferred_group_index(&self, conversation_id: &ConversationId) -> Option<usize> {
        if let Some(preferred) = self.last_target_nth
            && let Some(index) = self.groups.iter().position(|group| {
                group.nth_user_message == preferred
                    && group
                        .versions
                        .iter()
                        .any(|version| version.conversation_id == *conversation_id)
            })
        {
            return Some(index);
        }

        self.groups
            .iter()
            .enumerate()
            .filter(|(_, group)| {
                group
                    .versions
                    .iter()
                    .any(|version| version.conversation_id == *conversation_id)
            })
            .max_by_key(|(_, group)| group.nth_user_message)
            .map(|(idx, _)| idx)
    }
}

fn trim_transcript_cells_to_nth_user(
    transcript_cells: &mut Vec<Arc<dyn crate::history_cell::HistoryCell>>,
    nth_user_message: usize,
) {
    if nth_user_message == usize::MAX {
        return;
    }

    if let Some(cut_idx) = nth_user_position(transcript_cells, nth_user_message) {
        transcript_cells.truncate(cut_idx);
    }
}

pub(crate) fn user_count(cells: &[Arc<dyn crate::history_cell::HistoryCell>]) -> usize {
    user_positions_iter(cells).count()
}

fn nth_user_position(
    cells: &[Arc<dyn crate::history_cell::HistoryCell>],
    nth: usize,
) -> Option<usize> {
    user_positions_iter(cells)
        .enumerate()
        .find_map(|(i, idx)| (i == nth).then_some(idx))
}

fn user_positions_iter(
    cells: &[Arc<dyn crate::history_cell::HistoryCell>],
) -> impl Iterator<Item = usize> + '_ {
    let session_start_type = TypeId::of::<SessionInfoCell>();
    let user_type = TypeId::of::<UserHistoryCell>();
    let type_of = |cell: &Arc<dyn crate::history_cell::HistoryCell>| cell.as_any().type_id();

    let start = cells
        .iter()
        .rposition(|cell| type_of(cell) == session_start_type)
        .map_or(0, |idx| idx + 1);

    cells
        .iter()
        .enumerate()
        .skip(start)
        .filter_map(move |(idx, cell)| (type_of(cell) == user_type).then_some(idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history_cell::AgentMessageCell;
    use crate::history_cell::HistoryCell;
    use ratatui::prelude::Line;
    use std::sync::Arc;

    #[test]
    fn trim_transcript_for_first_user_drops_user_and_newer_cells() {
        let mut cells: Vec<Arc<dyn HistoryCell>> = vec![
            Arc::new(UserHistoryCell {
                message: "first user".to_string(),
            }) as Arc<dyn HistoryCell>,
            Arc::new(AgentMessageCell::new(vec![Line::from("assistant")], true))
                as Arc<dyn HistoryCell>,
        ];
        trim_transcript_cells_to_nth_user(&mut cells, 0);

        assert!(cells.is_empty());
    }

    #[test]
    fn trim_transcript_preserves_cells_before_selected_user() {
        let mut cells: Vec<Arc<dyn HistoryCell>> = vec![
            Arc::new(AgentMessageCell::new(vec![Line::from("intro")], true))
                as Arc<dyn HistoryCell>,
            Arc::new(UserHistoryCell {
                message: "first".to_string(),
            }) as Arc<dyn HistoryCell>,
            Arc::new(AgentMessageCell::new(vec![Line::from("after")], false))
                as Arc<dyn HistoryCell>,
        ];
        trim_transcript_cells_to_nth_user(&mut cells, 0);

        assert_eq!(cells.len(), 1);
        let agent = cells[0]
            .as_any()
            .downcast_ref::<AgentMessageCell>()
            .expect("agent cell");
        let agent_lines = agent.display_lines(u16::MAX);
        assert_eq!(agent_lines.len(), 1);
        let intro_text: String = agent_lines[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(intro_text, "• intro");
    }

    #[test]
    fn trim_transcript_for_later_user_keeps_prior_history() {
        let mut cells: Vec<Arc<dyn HistoryCell>> = vec![
            Arc::new(AgentMessageCell::new(vec![Line::from("intro")], true))
                as Arc<dyn HistoryCell>,
            Arc::new(UserHistoryCell {
                message: "first".to_string(),
            }) as Arc<dyn HistoryCell>,
            Arc::new(AgentMessageCell::new(vec![Line::from("between")], false))
                as Arc<dyn HistoryCell>,
            Arc::new(UserHistoryCell {
                message: "second".to_string(),
            }) as Arc<dyn HistoryCell>,
            Arc::new(AgentMessageCell::new(vec![Line::from("tail")], false))
                as Arc<dyn HistoryCell>,
        ];
        trim_transcript_cells_to_nth_user(&mut cells, 1);

        assert_eq!(cells.len(), 3);
        let agent_intro = cells[0]
            .as_any()
            .downcast_ref::<AgentMessageCell>()
            .expect("intro agent");
        let intro_lines = agent_intro.display_lines(u16::MAX);
        let intro_text: String = intro_lines[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(intro_text, "• intro");

        let user_first = cells[1]
            .as_any()
            .downcast_ref::<UserHistoryCell>()
            .expect("first user");
        assert_eq!(user_first.message, "first");

        let agent_between = cells[2]
            .as_any()
            .downcast_ref::<AgentMessageCell>()
            .expect("between agent");
        let between_lines = agent_between.display_lines(u16::MAX);
        let between_text: String = between_lines[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(between_text, "  between");
    }
}
