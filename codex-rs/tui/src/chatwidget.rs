use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use codex_app_server_protocol::AuthMode;
use codex_backend_client::Client as BackendClient;
use codex_core::config::Config;
use codex_core::config::types::DiffView;
use codex_core::config::types::ExportFormat;
use codex_core::config::types::Notifications;
use codex_core::features::FEATURES;
use codex_core::features::Feature;
use codex_core::git_info::current_branch_name;
use codex_core::git_info::local_git_branches;
use codex_core::models_manager::manager::ModelsManager;
use codex_core::models_manager::model_family::ModelFamily;
use codex_core::project_doc::DEFAULT_PROJECT_DOC_FILENAME;
use codex_core::protocol::AgentMessageDeltaEvent;
use codex_core::protocol::AgentMessageEvent;
use codex_core::protocol::AgentReasoningDeltaEvent;
use codex_core::protocol::AgentReasoningEvent;
use codex_core::protocol::AgentReasoningRawContentDeltaEvent;
use codex_core::protocol::AgentReasoningRawContentEvent;
use codex_core::protocol::ApplyPatchApprovalRequestEvent;
use codex_core::protocol::BackgroundEventEvent;
use codex_core::protocol::CreditsSnapshot;
use codex_core::protocol::DeprecationNoticeEvent;
use codex_core::protocol::ErrorEvent;
use codex_core::protocol::Event;
use codex_core::protocol::EventMsg;
use codex_core::protocol::ExecApprovalRequestEvent;
use codex_core::protocol::ExecCommandBeginEvent;
use codex_core::protocol::ExecCommandEndEvent;
use codex_core::protocol::ExecCommandSource;
use codex_core::protocol::ExitedReviewModeEvent;
use codex_core::protocol::ListCustomPromptsResponseEvent;
use codex_core::protocol::ListSkillsResponseEvent;
use codex_core::protocol::McpListToolsResponseEvent;
use codex_core::protocol::McpStartupCompleteEvent;
use codex_core::protocol::McpStartupStatus;
use codex_core::protocol::McpStartupUpdateEvent;
use codex_core::protocol::McpToolCallBeginEvent;
use codex_core::protocol::McpToolCallEndEvent;
use codex_core::protocol::Op;
use codex_core::protocol::PatchApplyBeginEvent;
use codex_core::protocol::RateLimitSnapshot;
use codex_core::protocol::ReviewRequest;
use codex_core::protocol::ReviewTarget;
use codex_core::protocol::SessionMode;
use codex_core::protocol::SessionTitleUpdatedEvent;
use codex_core::protocol::SkillsListEntry;
use codex_core::protocol::StreamErrorEvent;
use codex_core::protocol::TaskCompleteEvent;
use codex_core::protocol::TerminalInteractionEvent;
use codex_core::protocol::TokenUsage;
use codex_core::protocol::TokenUsageInfo;
use codex_core::protocol::TurnAbortReason;
use codex_core::protocol::TurnContextUpdatedEvent;
use codex_core::protocol::TurnDiffEvent;
use codex_core::protocol::UndoCompletedEvent;
use codex_core::protocol::UndoStartedEvent;
use codex_core::protocol::UserMessageEvent;
use codex_core::protocol::ViewImageToolCallEvent;
use codex_core::protocol::WarningEvent;
use codex_core::protocol::WebSearchBeginEvent;
use codex_core::protocol::WebSearchEndEvent;
use codex_core::skills::model::SkillMetadata;
use codex_protocol::ConversationId;
use codex_protocol::account::PlanType;
use codex_protocol::approvals::ElicitationRequestEvent;
use codex_protocol::parse_command::ParsedCommand;
use codex_protocol::user_input::UserInput;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use rand::Rng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;
use tracing::debug;

use crate::app_backtrack::BacktrackActionRequest;
use crate::app_backtrack::ResendOverrides;
use crate::app_event::AppEvent;
use crate::app_event::ChatExportFormat;
use crate::app_event::ExportOverrides;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::ApprovalRequest;
use crate::bottom_pane::BetaFeatureItem;
use crate::bottom_pane::BottomPane;
use crate::bottom_pane::BottomPaneParams;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::ComposerAttachment;
use crate::bottom_pane::ExperimentalFeaturesView;
use crate::bottom_pane::InputResult;
use crate::bottom_pane::QueuePopup;
use crate::bottom_pane::QueuePopupItem;
use crate::bottom_pane::SelectionAction;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::bottom_pane::StatusLineMetrics;
use crate::bottom_pane::ViewCompletionBehavior;
use crate::bottom_pane::custom_prompt_view::CustomPromptView;
use crate::bottom_pane::parse_positional_args;
use crate::bottom_pane::parse_slash_name;
use crate::bottom_pane::popup_consts::standard_popup_hint_line;
use crate::clipboard_paste::PasteImageError;
use crate::clipboard_paste::copy_text_to_clipboard;
use crate::clipboard_paste::paste_image_to_temp_png;
use crate::clipboard_paste::paste_text_from_clipboard;
use crate::diff_render::display_path_for;
use crate::exec_cell::CommandOutput;
use crate::exec_cell::ExecCell;
use crate::exec_cell::LiveOutputScrollAction;
use crate::exec_cell::new_active_exec_command;
use crate::exec_command::strip_bash_lc_and_escape;
use crate::export_markdown;
use crate::get_git_diff::GitDiffResult;
use crate::get_git_diff::get_git_diff;
use crate::history_cell;
use crate::history_cell::AgentMessageCell;
use crate::history_cell::HistoryCell;
use crate::history_cell::McpToolCallCell;
use crate::history_cell::PlainHistoryCell;
use crate::keybindings::Keybindings;
use crate::markdown::append_markdown;
use crate::render::Insets;
use crate::render::renderable::ColumnRenderable;
use crate::render::renderable::FlexRenderable;
use crate::render::renderable::Renderable;
use crate::render::renderable::RenderableExt;
use crate::render::renderable::RenderableItem;
use crate::session_manager::SessionManagerEntry;
use crate::session_manager::paths_match;
use crate::slash_command::SlashCommand;
use crate::status::RateLimitSnapshotDisplay;
use crate::text_formatting::truncate_text;
use crate::tui::FrameRequester;
mod interrupts;
use self::interrupts::InterruptManager;
mod agent;
use self::agent::spawn_agent;
use self::agent::spawn_agent_from_existing;
mod session_header;
use self::session_header::SessionHeader;
use crate::streaming::controller::StreamController;

use chrono::Local;
use codex_common::approval_presets::ApprovalPreset;
use codex_common::approval_presets::builtin_approval_presets;
use codex_core::AuthManager;
use codex_core::CodexAuth;
use codex_core::ConversationManager;
use codex_core::protocol::AskForApproval;
use codex_core::protocol::SandboxPolicy;
use codex_file_search::FileMatch;
use codex_protocol::openai_models::ModelPreset;
use codex_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use codex_protocol::plan_tool::UpdatePlanArgs;
use strum::IntoEnumIterator;

const USER_SHELL_COMMAND_HELP_TITLE: &str = "Prefix a command with ! to run it locally";
const USER_SHELL_COMMAND_HELP_HINT: &str = "Example: !ls";
// Track information about an in-flight exec command.
struct RunningCommand {
    command: Vec<String>,
    parsed_cmd: Vec<ParsedCommand>,
    source: ExecCommandSource,
}

#[derive(Clone, Debug, Default)]
struct TurnMetrics {
    started_at: Option<Instant>,
    first_response_at: Option<Instant>,
    tool_time: Duration,
    last_turn_duration: Option<Duration>,
    last_token_usage: Option<TokenUsage>,
}

impl TurnMetrics {
    fn reset(&mut self, now: Instant) {
        self.started_at = Some(now);
        self.first_response_at = None;
        self.tool_time = Duration::ZERO;
        self.last_turn_duration = None;
    }

    fn record_first_response(&mut self, now: Instant) -> Option<Duration> {
        let start = self.started_at?;
        if self.first_response_at.is_some() {
            return None;
        }
        self.first_response_at = Some(now);
        Some(now.saturating_duration_since(start))
    }

    fn finish(&mut self, now: Instant) {
        if let Some(start) = self.started_at {
            self.last_turn_duration = Some(now.saturating_duration_since(start));
        }
        self.started_at = None;
    }

    fn tokens_per_sec(&self, now: Instant) -> Option<f64> {
        let usage = self.last_token_usage.as_ref()?;
        let output_tokens = usage
            .output_tokens
            .saturating_add(usage.reasoning_output_tokens);
        if output_tokens <= 0 {
            return None;
        }
        let duration = if let Some(duration) = self.last_turn_duration {
            duration
        } else {
            let start = self.started_at?;
            now.saturating_duration_since(start)
        };
        let seconds = duration.as_secs_f64();
        if seconds <= 0.0 {
            return None;
        }
        Some(output_tokens as f64 / seconds)
    }
}

struct UnifiedExecSessionSummary {
    key: String,
    command_display: String,
}

struct UnifiedExecWaitState {
    command_display: String,
}

impl UnifiedExecWaitState {
    fn new(command_display: String) -> Self {
        Self { command_display }
    }

    fn is_duplicate(&self, command_display: &str) -> bool {
        self.command_display == command_display
    }
}

fn is_unified_exec_source(source: ExecCommandSource) -> bool {
    matches!(
        source,
        ExecCommandSource::UnifiedExecStartup | ExecCommandSource::UnifiedExecInteraction
    )
}

fn is_standard_tool_call(parsed_cmd: &[ParsedCommand]) -> bool {
    !parsed_cmd.is_empty()
        && parsed_cmd
            .iter()
            .all(|parsed| !matches!(parsed, ParsedCommand::Unknown { .. }))
}

const RATE_LIMIT_WARNING_THRESHOLDS: [f64; 3] = [75.0, 90.0, 95.0];
const NUDGE_MODEL_SLUG: &str = "gpt-5.1-codex-mini";
const RATE_LIMIT_SWITCH_PROMPT_THRESHOLD: f64 = 90.0;
const MIN_ACTIVE_CELL_HEIGHT: u16 = 6;

#[derive(Default)]
struct RateLimitWarningState {
    secondary_index: usize,
    primary_index: usize,
}

impl RateLimitWarningState {
    fn take_warnings(
        &mut self,
        secondary_used_percent: Option<f64>,
        secondary_window_minutes: Option<i64>,
        primary_used_percent: Option<f64>,
        primary_window_minutes: Option<i64>,
    ) -> Vec<String> {
        let reached_secondary_cap =
            matches!(secondary_used_percent, Some(percent) if percent == 100.0);
        let reached_primary_cap = matches!(primary_used_percent, Some(percent) if percent == 100.0);
        if reached_secondary_cap || reached_primary_cap {
            return Vec::new();
        }

        let mut warnings = Vec::new();

        if let Some(secondary_used_percent) = secondary_used_percent {
            let mut highest_secondary: Option<f64> = None;
            while self.secondary_index < RATE_LIMIT_WARNING_THRESHOLDS.len()
                && secondary_used_percent >= RATE_LIMIT_WARNING_THRESHOLDS[self.secondary_index]
            {
                highest_secondary = Some(RATE_LIMIT_WARNING_THRESHOLDS[self.secondary_index]);
                self.secondary_index += 1;
            }
            if let Some(threshold) = highest_secondary {
                let limit_label = secondary_window_minutes
                    .map(get_limits_duration)
                    .unwrap_or_else(|| "weekly".to_string());
                let remaining_percent = 100.0 - threshold;
                warnings.push(format!(
                    "Heads up, you have less than {remaining_percent:.0}% of your {limit_label} limit left. Run /status for a breakdown."
                ));
            }
        }

        if let Some(primary_used_percent) = primary_used_percent {
            let mut highest_primary: Option<f64> = None;
            while self.primary_index < RATE_LIMIT_WARNING_THRESHOLDS.len()
                && primary_used_percent >= RATE_LIMIT_WARNING_THRESHOLDS[self.primary_index]
            {
                highest_primary = Some(RATE_LIMIT_WARNING_THRESHOLDS[self.primary_index]);
                self.primary_index += 1;
            }
            if let Some(threshold) = highest_primary {
                let limit_label = primary_window_minutes
                    .map(get_limits_duration)
                    .unwrap_or_else(|| "5h".to_string());
                let remaining_percent = 100.0 - threshold;
                warnings.push(format!(
                    "Heads up, you have less than {remaining_percent:.0}% of your {limit_label} limit left. Run /status for a breakdown."
                ));
            }
        }

        warnings
    }
}

pub(crate) fn get_limits_duration(windows_minutes: i64) -> String {
    const MINUTES_PER_HOUR: i64 = 60;
    const MINUTES_PER_DAY: i64 = 24 * MINUTES_PER_HOUR;
    const MINUTES_PER_WEEK: i64 = 7 * MINUTES_PER_DAY;
    const MINUTES_PER_MONTH: i64 = 30 * MINUTES_PER_DAY;
    const ROUNDING_BIAS_MINUTES: i64 = 3;

    let windows_minutes = windows_minutes.max(0);

    if windows_minutes <= MINUTES_PER_DAY.saturating_add(ROUNDING_BIAS_MINUTES) {
        let adjusted = windows_minutes.saturating_add(ROUNDING_BIAS_MINUTES);
        let hours = std::cmp::max(1, adjusted / MINUTES_PER_HOUR);
        format!("{hours}h")
    } else if windows_minutes <= MINUTES_PER_WEEK.saturating_add(ROUNDING_BIAS_MINUTES) {
        "weekly".to_string()
    } else if windows_minutes <= MINUTES_PER_MONTH.saturating_add(ROUNDING_BIAS_MINUTES) {
        "monthly".to_string()
    } else {
        "annual".to_string()
    }
}

/// Common initialization parameters shared by all `ChatWidget` constructors.
pub(crate) struct ChatWidgetInit {
    pub(crate) config: Config,
    pub(crate) frame_requester: FrameRequester,
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) initial_prompt: Option<String>,
    pub(crate) initial_images: Vec<PathBuf>,
    pub(crate) enhanced_keys_supported: bool,
    pub(crate) auth_manager: Arc<AuthManager>,
    pub(crate) models_manager: Arc<ModelsManager>,
    pub(crate) feedback: codex_feedback::CodexFeedback,
    pub(crate) is_first_run: bool,
    pub(crate) model_family: ModelFamily,
}

#[derive(Default)]
enum RateLimitSwitchPromptState {
    #[default]
    Idle,
    Pending,
    Shown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExternalEditorState {
    #[default]
    Closed,
    Requested,
    Active,
}

pub(crate) struct ChatWidget {
    app_event_tx: AppEventSender,
    codex_op_tx: UnboundedSender<Op>,
    bottom_pane: BottomPane,
    active_cell: Option<Box<dyn HistoryCell>>,
    config: Config,
    keybindings: Keybindings,
    model_family: ModelFamily,
    auth_manager: Arc<AuthManager>,
    models_manager: Arc<ModelsManager>,
    session_header: SessionHeader,
    initial_user_message: Option<UserMessage>,
    token_info: Option<TokenUsageInfo>,
    rate_limit_snapshot: Option<RateLimitSnapshotDisplay>,
    plan_type: Option<PlanType>,
    rate_limit_warnings: RateLimitWarningState,
    rate_limit_switch_prompt: RateLimitSwitchPromptState,
    rate_limit_poller: Option<JoinHandle<()>>,
    // Stream lifecycle controller
    stream_controller: Option<StreamController>,
    stream_paused: bool,
    paused_status_header: Option<String>,
    paused_agent_deltas: String,
    paused_reasoning_blocks: Vec<String>,
    paused_pending_answer_flush: bool,
    running_commands: HashMap<String, RunningCommand>,
    suppressed_exec_calls: HashSet<String>,
    last_unified_wait: Option<UnifiedExecWaitState>,
    task_complete_pending: bool,
    unified_exec_sessions: Vec<UnifiedExecSessionSummary>,
    mcp_startup_status: Option<HashMap<String, McpStartupStatus>>,
    // Queue of interruptive UI events deferred during an active write cycle
    interrupts: InterruptManager,
    // Accumulates the current reasoning block text to extract a header
    reasoning_buffer: String,
    // Accumulates full reasoning content for transcript-only recording
    full_reasoning_buffer: String,
    // Current status header shown in the status indicator.
    current_status_header: String,
    // Previous status header to restore after a transient stream retry.
    retry_status_header: Option<String>,
    status_line_metrics: StatusLineMetrics,
    turn_metrics: TurnMetrics,
    conversation_id: Option<ConversationId>,
    frame_requester: FrameRequester,
    // Whether to include the initial welcome banner on session configured
    show_welcome_banner: bool,
    // When resuming an existing session (selected via resume picker), avoid an
    // immediate redraw on SessionConfigured to prevent a gratuitous UI flicker.
    suppress_session_configured_redraw: bool,
    // User messages queued while a turn is in progress
    queued_user_messages: VecDeque<QueuedUserMessage>,
    next_queued_user_message_id: u64,
    queued_edit_state: Option<QueuedEditState>,
    queued_auto_send_pending: bool,
    // Pending notification to show when unfocused on next Draw
    pending_notification: Option<Notification>,
    notification_focus_override: Option<bool>,
    // Simple review mode flag; used to adjust layout and banners.
    is_review_mode: bool,
    session_mode: SessionMode,
    // Snapshot of token usage to restore after review mode exits.
    pre_review_token_info: Option<Option<TokenUsageInfo>>,
    // Whether to add a final message separator after the last message
    needs_final_message_separator: bool,

    last_rendered_width: std::cell::Cell<Option<usize>>,
    // Feedback sink for /feedback
    feedback: codex_feedback::CodexFeedback,
    // Current session rollout path (if known)
    current_rollout_path: Option<PathBuf>,
    session_title: Option<String>,
    external_editor_state: ExternalEditorState,

    pending_active_turn_context: Option<PendingTurnContext>,
    active_turn_context: Option<PendingTurnContext>,
}

struct UserMessage {
    text: String,
    image_paths: Vec<PathBuf>,
    attachments: Vec<ComposerAttachment>,
}

#[derive(Clone, Debug)]
struct QueuedUserMessage {
    id: u64,
    text: String,
    attachments: Vec<ComposerAttachment>,
    model_override: Option<String>,
    effort_override: Option<Option<ReasoningEffortConfig>>,
}

#[derive(Clone, Debug)]
pub(crate) struct QueueSnapshot {
    messages: VecDeque<QueuedUserMessage>,
    next_id: u64,
}

#[derive(Clone, Debug)]
struct PendingTurnContext {
    model: String,
    reasoning_effort: Option<ReasoningEffortConfig>,
    model_family: ModelFamily,
    mode: SessionMode,
}

#[derive(Clone)]
struct QueuedComposerSnapshot {
    text: String,
    attachments: Vec<ComposerAttachment>,
}

#[derive(Clone)]
struct QueuedUserMessageDraft {
    text: String,
    attachments: Vec<ComposerAttachment>,
    model_override: Option<String>,
    effort_override: Option<Option<ReasoningEffortConfig>>,
}

struct QueuedEditState {
    selected_id: u64,
    composer_before_edit: QueuedComposerSnapshot,
    drafts: HashMap<u64, QueuedUserMessageDraft>,
}

impl From<String> for UserMessage {
    fn from(text: String) -> Self {
        Self {
            text,
            image_paths: Vec::new(),
            attachments: Vec::new(),
        }
    }
}

impl From<&str> for UserMessage {
    fn from(text: &str) -> Self {
        Self {
            text: text.to_string(),
            image_paths: Vec::new(),
            attachments: Vec::new(),
        }
    }
}

fn create_initial_user_message(text: String, image_paths: Vec<PathBuf>) -> Option<UserMessage> {
    if text.is_empty() && image_paths.is_empty() {
        None
    } else {
        Some(UserMessage {
            text,
            image_paths,
            attachments: Vec::new(),
        })
    }
}

impl ChatWidget {
    fn refresh_status_line_git_branch(app_event_tx: AppEventSender, cwd: PathBuf) {
        tokio::spawn(async move {
            let branch = current_branch_name(&cwd).await;
            app_event_tx.send(AppEvent::UpdateStatusLineGitBranch { branch });
        });
    }

    fn session_turn_context(&self) -> PendingTurnContext {
        PendingTurnContext {
            model: self
                .config
                .model
                .clone()
                .unwrap_or_else(|| self.model_family.get_model_slug().to_string()),
            reasoning_effort: self.effective_reasoning_effort(),
            model_family: self.model_family.clone(),
            mode: self.session_mode,
        }
    }

    fn flush_answer_stream_with_separator(&mut self) {
        if self.stream_paused {
            if self.stream_controller.is_some() || !self.paused_agent_deltas.is_empty() {
                self.paused_pending_answer_flush = true;
            }
            return;
        }

        if let Some(mut controller) = self.stream_controller.take()
            && let Some(cell) = controller.finalize()
        {
            self.add_boxed_history(cell);
        }
    }

    /// Update the status indicator header and details.
    ///
    /// Passing `None` clears any existing details.
    fn set_status(&mut self, header: String, details: Option<String>) {
        self.current_status_header = header.clone();
        self.bottom_pane.update_status(header, details);
    }

    /// Convenience wrapper around [`Self::set_status`];
    /// updates the status indicator header and clears any existing details.
    fn set_status_header(&mut self, header: String) {
        self.set_status(header, None);
    }

    fn sync_status_line_metrics(&mut self) {
        self.bottom_pane
            .set_status_line_metrics(self.status_line_metrics.clone());
    }

    fn reset_turn_metrics(&mut self) {
        self.turn_metrics.reset(Instant::now());
        self.status_line_metrics = StatusLineMetrics::default();
        self.sync_status_line_metrics();
    }

    fn record_first_response(&mut self) {
        if let Some(latency) = self.turn_metrics.record_first_response(Instant::now()) {
            self.status_line_metrics.latency = Some(latency);
            self.sync_status_line_metrics();
        }
    }

    fn record_tool_time(&mut self, duration: Duration) {
        self.turn_metrics.tool_time += duration;
        self.status_line_metrics.tool_time = Some(self.turn_metrics.tool_time);
        self.sync_status_line_metrics();
    }

    fn refresh_tokens_per_sec(&mut self) {
        self.status_line_metrics.tokens_per_sec = self.turn_metrics.tokens_per_sec(Instant::now());
        self.sync_status_line_metrics();
    }

    fn restore_retry_status_header_if_present(&mut self) {
        if let Some(header) = self.retry_status_header.take()
            && self.current_status_header != header
        {
            self.set_status_header(header);
        }
    }

    fn notification_focus_configured(&self) -> bool {
        let focus = &self.config.notification_focus;
        !focus.whitelist.is_empty()
            || !focus.blacklist.is_empty()
            || !focus.bundle_id_whitelist.is_empty()
            || !focus.bundle_id_blacklist.is_empty()
    }

    // --- Small event handlers ---
    fn on_session_configured(&mut self, event: codex_core::protocol::SessionConfiguredEvent) {
        self.bottom_pane
            .set_history_metadata(event.history_log_id, event.history_entry_count);
        self.set_skills(None);
        self.conversation_id = Some(event.session_id);
        self.current_rollout_path = Some(event.rollout_path.clone());
        self.session_title = None;
        let initial_messages = event.initial_messages.clone();
        let model_for_header = event.model.clone();
        self.config.model = Some(model_for_header.clone());
        self.session_mode = event.mode;
        self.session_header.set_model(&model_for_header);
        self.bottom_pane.set_session_model(model_for_header.clone());
        self.bottom_pane.set_session_mode(event.mode);
        self.bottom_pane.set_session_reasoning_effort(
            event.reasoning_effort.or(self.effective_reasoning_effort()),
        );
        self.bottom_pane.set_active_model(None);
        self.bottom_pane.set_active_reasoning_effort(None);
        self.pending_active_turn_context = None;
        self.active_turn_context = None;
        self.add_to_history(history_cell::new_session_info(
            &self.config,
            &model_for_header,
            event,
            self.show_welcome_banner,
        ));
        if let Some(messages) = initial_messages {
            self.replay_initial_messages(messages);
        }
        // Ask codex-core to enumerate custom prompts for this session.
        self.submit_op(Op::ListCustomPrompts);
        self.submit_op(Op::ListSkills {
            cwds: Vec::new(),
            force_reload: false,
        });
        if let Some(user_message) = self.initial_user_message.take() {
            self.submit_user_message(user_message);
        }
        if !self.suppress_session_configured_redraw {
            self.request_redraw();
        }
    }

    fn on_turn_context_updated(&mut self, event: TurnContextUpdatedEvent) {
        self.config.model = Some(event.model.clone());
        self.config.model_reasoning_effort = event.reasoning_effort;
        self.session_mode = event.mode;

        self.session_header.set_model(&event.model);
        self.bottom_pane.set_session_model(event.model.clone());
        self.bottom_pane.set_session_mode(event.mode);
        self.bottom_pane
            .set_session_reasoning_effort(event.reasoning_effort);

        // Update app-level state and refresh the model family asynchronously.
        self.app_event_tx.send(AppEvent::UpdateModel(event.model));
        self.app_event_tx
            .send(AppEvent::UpdateReasoningEffort(event.reasoning_effort));
    }

    fn on_session_title_updated(&mut self, event: SessionTitleUpdatedEvent) {
        self.session_title = event.title.clone();
        if let Some(path) = self.current_rollout_path.as_ref() {
            self.bottom_pane
                .apply_session_manager_rename(path, event.title.clone());
        }
        let message = match event.title.as_deref() {
            Some(title) => format!("Renamed chat to \"{title}\"."),
            None => "Cleared chat title.".to_string(),
        };
        self.add_info_message(message, None);
    }

    fn set_skills(&mut self, skills: Option<Vec<SkillMetadata>>) {
        self.bottom_pane.set_skills(skills);
    }

    fn set_skills_from_response(&mut self, response: &ListSkillsResponseEvent) {
        let skills = skills_for_cwd(&self.config.cwd, &response.skills);
        self.set_skills(Some(skills));
    }

    pub(crate) fn open_feedback_note(
        &mut self,
        category: crate::app_event::FeedbackCategory,
        include_logs: bool,
    ) {
        // Build a fresh snapshot at the time of opening the note overlay.
        let snapshot = self.feedback.snapshot(self.conversation_id);
        let rollout = if include_logs {
            self.current_rollout_path.clone()
        } else {
            None
        };
        let view = crate::bottom_pane::FeedbackNoteView::new(
            category,
            snapshot,
            rollout,
            self.app_event_tx.clone(),
            include_logs,
        );
        self.bottom_pane.show_view(Box::new(view));
        self.request_redraw();
    }

    pub(crate) fn open_feedback_consent(&mut self, category: crate::app_event::FeedbackCategory) {
        let params = crate::bottom_pane::feedback_upload_consent_params(
            self.app_event_tx.clone(),
            category,
            self.current_rollout_path.clone(),
        );
        self.bottom_pane.show_selection_view(params);
        self.request_redraw();
    }

    pub(crate) fn open_rename_chat_view(&mut self) {
        let view = crate::bottom_pane::RenameChatView::new(
            self.app_event_tx.clone(),
            self.session_title.clone(),
            crate::bottom_pane::RenameTarget::CurrentSession,
        );
        self.bottom_pane.show_view(Box::new(view));
        self.request_redraw();
    }

    pub(crate) fn open_rename_session_view(
        &mut self,
        target: crate::bottom_pane::RenameTarget,
        current_title: Option<String>,
    ) {
        let view = crate::bottom_pane::RenameChatView::new(
            self.app_event_tx.clone(),
            current_title,
            target,
        );
        self.bottom_pane.show_view(Box::new(view));
        self.request_redraw();
    }

    pub(crate) fn open_session_manager(&mut self) {
        let view = crate::bottom_pane::SessionManagerView::new(
            self.app_event_tx.clone(),
            self.config.cwd.clone(),
        );
        self.bottom_pane.show_view(Box::new(view));
        self.request_redraw();

        let codex_home = self.config.codex_home.clone();
        let default_provider = self.config.model_provider_id.clone();
        let current_path = self.current_rollout_path.clone();
        let tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            match crate::session_manager::load_session_entries(
                &codex_home,
                &default_provider,
                current_path.as_deref(),
            )
            .await
            {
                Ok(sessions) => {
                    tx.send(AppEvent::SessionManagerLoaded { sessions });
                }
                Err(err) => {
                    tx.send(AppEvent::SessionManagerLoadFailed {
                        message: err.to_string(),
                    });
                }
            }
        });
    }

    pub(crate) fn set_session_manager_sessions(&mut self, sessions: Vec<SessionManagerEntry>) {
        self.bottom_pane.update_session_manager_sessions(sessions);
        self.request_redraw();
    }

    pub(crate) fn set_session_manager_error(&mut self, message: String) {
        self.bottom_pane.set_session_manager_error(message);
        self.request_redraw();
    }

    pub(crate) fn handle_session_manager_rename_result(
        &mut self,
        path: PathBuf,
        title: Option<String>,
        error: Option<String>,
    ) {
        if let Some(error) = error {
            self.add_error_message(format!("Failed to rename chat: {error}"));
            return;
        }

        if self.is_current_session_path(&path) {
            return;
        }

        self.bottom_pane
            .apply_session_manager_rename(&path, title.clone());

        let message = match title.as_deref() {
            Some(title) => format!("Renamed saved chat to \"{title}\"."),
            None => "Cleared saved chat title.".to_string(),
        };
        self.add_info_message(message, None);
    }

    pub(crate) fn handle_session_manager_delete_result(
        &mut self,
        path: PathBuf,
        label: String,
        error: Option<String>,
    ) {
        if let Some(error) = error {
            self.add_error_message(format!("Failed to delete chat: {error}"));
            return;
        }

        if self.is_current_session_path(&path) {
            return;
        }

        self.bottom_pane.apply_session_manager_delete(&path);
        self.add_info_message(format!("Deleted saved chat \"{label}\"."), None);
    }

    fn is_current_session_path(&self, path: &Path) -> bool {
        self.current_rollout_path
            .as_ref()
            .is_some_and(|current| paths_match(current, path))
    }

    pub(crate) fn rename_session(&mut self, title: Option<String>) {
        self.submit_op(Op::UpdateSessionTitle { title });
    }

    fn on_agent_message(&mut self, message: String) {
        // If we have a stream_controller, then the final agent message is redundant and will be a
        // duplicate of what has already been streamed.
        if self.stream_controller.is_none() {
            self.handle_streaming_delta(message);
        }
        self.flush_answer_stream_with_separator();
        self.handle_stream_finished();
        self.request_redraw();
    }

    fn on_agent_message_delta(&mut self, delta: String) {
        self.handle_streaming_delta(delta);
    }

    fn on_agent_reasoning_delta(&mut self, delta: String) {
        // For reasoning deltas, do not stream to history. Accumulate the
        // current reasoning block and extract the first bold element
        // (between **/**) as the chunk header. Show this header as status.
        self.record_first_response();
        self.reasoning_buffer.push_str(&delta);

        if self.stream_paused {
            return;
        }

        if let Some(header) = extract_first_bold(&self.reasoning_buffer) {
            // Update the shimmer header to the extracted reasoning chunk header.
            self.set_status_header(header);
        } else {
            // Fallback while we don't yet have a bold header: leave existing header as-is.
        }
        self.request_redraw();
    }

    fn on_agent_reasoning_final(&mut self) {
        // At the end of a reasoning block, record transcript-only content.
        self.full_reasoning_buffer.push_str(&self.reasoning_buffer);
        if self.stream_paused {
            if !self.full_reasoning_buffer.is_empty() {
                self.paused_reasoning_blocks
                    .push(self.full_reasoning_buffer.clone());
            }
            self.reasoning_buffer.clear();
            self.full_reasoning_buffer.clear();
            return;
        }
        if !self.full_reasoning_buffer.is_empty() {
            let cell =
                history_cell::new_reasoning_summary_block(self.full_reasoning_buffer.clone());
            self.add_boxed_history(cell);
        }
        self.reasoning_buffer.clear();
        self.full_reasoning_buffer.clear();
        self.request_redraw();
    }

    fn on_reasoning_section_break(&mut self) {
        // Start a new reasoning block for header extraction and accumulate transcript.
        self.full_reasoning_buffer.push_str(&self.reasoning_buffer);
        self.full_reasoning_buffer.push_str("\n\n");
        self.reasoning_buffer.clear();
    }

    // Raw reasoning uses the same flow as summarized reasoning

    fn on_task_started(&mut self) {
        self.bottom_pane.clear_ctrl_c_quit_hint();
        self.bottom_pane.set_task_running(true);
        self.reset_turn_metrics();
        let mut active = self
            .pending_active_turn_context
            .take()
            .unwrap_or_else(|| self.session_turn_context());
        if active.model != self.model_family.get_model_slug() {
            active.model_family = self.model_family.clone();
        }
        self.active_turn_context = Some(active.clone());
        self.bottom_pane.set_active_model(Some(active.model));
        self.bottom_pane
            .set_active_reasoning_effort(active.reasoning_effort);
        self.bottom_pane.set_active_mode(Some(active.mode));
        self.retry_status_header = None;
        self.bottom_pane.set_interrupt_hint_visible(true);
        self.set_status_header(String::from("Working"));
        if self.stream_paused {
            self.set_status_header(String::from("Paused"));
        }
        self.full_reasoning_buffer.clear();
        self.reasoning_buffer.clear();
        self.request_redraw();
    }

    fn on_task_complete(&mut self, last_agent_message: Option<String>) {
        // If a stream is currently active, finalize it.
        self.flush_answer_stream_with_separator();
        self.turn_metrics.finish(Instant::now());
        self.refresh_tokens_per_sec();
        // Mark task stopped and request redraw now that all content is in history.
        self.bottom_pane.set_task_running(false);
        self.bottom_pane.set_active_model(None);
        self.bottom_pane.set_active_reasoning_effort(None);
        self.bottom_pane.set_active_mode(None);
        self.pending_active_turn_context = None;
        self.active_turn_context = None;
        self.running_commands.clear();
        self.suppressed_exec_calls.clear();
        self.last_unified_wait = None;
        self.request_redraw();

        // If there is a queued user message, send exactly one now to begin the next turn.
        self.queued_auto_send_pending =
            !self.queued_user_messages.is_empty() && self.bottom_pane.composer_is_empty();
        self.maybe_send_next_queued_input();
        // Emit a notification when the turn completes (suppressed if focused).
        self.notify(Notification::AgentTurnComplete {
            response: last_agent_message.unwrap_or_default(),
        });

        self.maybe_show_pending_rate_limit_prompt();
    }

    pub(crate) fn set_token_info(&mut self, info: Option<TokenUsageInfo>) {
        match info {
            Some(info) => self.apply_token_info(info),
            None => {
                self.bottom_pane.set_context_window(None, None);
                self.token_info = None;
            }
        }
    }

    fn apply_token_info(&mut self, info: TokenUsageInfo) {
        let percent = self.context_remaining_percent(&info);
        let used_tokens = self.context_used_tokens(&info, percent.is_some());
        self.bottom_pane.set_context_window(percent, used_tokens);
        self.turn_metrics.last_token_usage = Some(info.last_token_usage.clone());
        self.token_info = Some(info);
        self.refresh_tokens_per_sec();
    }

    fn context_remaining_percent(&self, info: &TokenUsageInfo) -> Option<i64> {
        info.model_context_window
            .or(self.model_family.context_window)
            .map(|window| {
                info.last_token_usage
                    .percent_of_context_window_remaining(window)
            })
    }

    fn context_used_tokens(&self, info: &TokenUsageInfo, percent_known: bool) -> Option<i64> {
        if percent_known {
            return None;
        }

        Some(info.total_token_usage.tokens_in_context_window())
    }

    fn restore_pre_review_token_info(&mut self) {
        if let Some(saved) = self.pre_review_token_info.take() {
            match saved {
                Some(info) => self.apply_token_info(info),
                None => {
                    self.bottom_pane.set_context_window(None, None);
                    self.token_info = None;
                }
            }
        }
    }

    pub(crate) fn on_rate_limit_snapshot(&mut self, snapshot: Option<RateLimitSnapshot>) {
        if let Some(mut snapshot) = snapshot {
            if snapshot.credits.is_none() {
                snapshot.credits = self
                    .rate_limit_snapshot
                    .as_ref()
                    .and_then(|display| display.credits.as_ref())
                    .map(|credits| CreditsSnapshot {
                        has_credits: credits.has_credits,
                        unlimited: credits.unlimited,
                        balance: credits.balance.clone(),
                    });
            }

            self.plan_type = snapshot.plan_type.or(self.plan_type);

            let warnings = self.rate_limit_warnings.take_warnings(
                snapshot
                    .secondary
                    .as_ref()
                    .map(|window| window.used_percent),
                snapshot
                    .secondary
                    .as_ref()
                    .and_then(|window| window.window_minutes),
                snapshot.primary.as_ref().map(|window| window.used_percent),
                snapshot
                    .primary
                    .as_ref()
                    .and_then(|window| window.window_minutes),
            );

            let high_usage = snapshot
                .secondary
                .as_ref()
                .map(|w| w.used_percent >= RATE_LIMIT_SWITCH_PROMPT_THRESHOLD)
                .unwrap_or(false)
                || snapshot
                    .primary
                    .as_ref()
                    .map(|w| w.used_percent >= RATE_LIMIT_SWITCH_PROMPT_THRESHOLD)
                    .unwrap_or(false);

            if high_usage
                && !self.rate_limit_switch_prompt_hidden()
                && self.model_family.get_model_slug() != NUDGE_MODEL_SLUG
                && !matches!(
                    self.rate_limit_switch_prompt,
                    RateLimitSwitchPromptState::Shown
                )
            {
                self.rate_limit_switch_prompt = RateLimitSwitchPromptState::Pending;
            }

            let display = crate::status::rate_limit_snapshot_display(&snapshot, Local::now());
            self.rate_limit_snapshot = Some(display);

            if !warnings.is_empty() {
                for warning in warnings {
                    self.add_to_history(history_cell::new_warning_event(warning));
                }
                self.request_redraw();
            }
        } else {
            self.rate_limit_snapshot = None;
        }
    }
    /// Finalize any active exec as failed and stop/clear running UI state.
    fn finalize_turn(&mut self) {
        // Ensure any spinner is replaced by a red ✗ and flushed into history.
        self.finalize_active_cell_as_failed();
        // Reset running state and clear streaming buffers.
        self.bottom_pane.set_task_running(false);
        self.bottom_pane.set_active_model(None);
        self.bottom_pane.set_active_reasoning_effort(None);
        self.pending_active_turn_context = None;
        self.active_turn_context = None;
        self.running_commands.clear();
        self.suppressed_exec_calls.clear();
        self.last_unified_wait = None;
        self.stream_controller = None;
        // Any deferred events belong to the aborted turn; discard them so the UI doesn't wedge.
        self.interrupts = InterruptManager::new();
        self.queued_auto_send_pending = false;
        self.maybe_show_pending_rate_limit_prompt();
    }
    pub(crate) fn get_model_family(&self) -> ModelFamily {
        self.model_family.clone()
    }

    fn on_error(&mut self, message: String) {
        self.finalize_turn();
        self.add_to_history(history_cell::new_error_event(message));
        self.request_redraw();

        // After an error ends the turn, try sending the next queued input.
        self.queued_auto_send_pending =
            !self.queued_user_messages.is_empty() && self.bottom_pane.composer_is_empty();
        self.maybe_send_next_queued_input();
    }

    fn on_warning(&mut self, message: impl Into<String>) {
        self.add_to_history(history_cell::new_warning_event(message.into()));
        self.request_redraw();
    }

    fn on_mcp_startup_update(&mut self, ev: McpStartupUpdateEvent) {
        let mut status = self.mcp_startup_status.take().unwrap_or_default();
        if let McpStartupStatus::Failed { error } = &ev.status {
            self.on_warning(error);
        }
        status.insert(ev.server, ev.status);
        self.mcp_startup_status = Some(status);
        self.bottom_pane.set_task_running(true);
        if let Some(current) = &self.mcp_startup_status {
            let total = current.len();
            let mut starting: Vec<_> = current
                .iter()
                .filter_map(|(name, state)| {
                    if matches!(state, McpStartupStatus::Starting) {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect();
            starting.sort();
            if let Some(first) = starting.first() {
                let completed = total.saturating_sub(starting.len());
                let max_to_show = 3;
                let mut to_show: Vec<String> = starting
                    .iter()
                    .take(max_to_show)
                    .map(ToString::to_string)
                    .collect();
                if starting.len() > max_to_show {
                    to_show.push("…".to_string());
                }
                let header = if total > 1 {
                    format!(
                        "Starting MCP servers ({completed}/{total}): {}",
                        to_show.join(", ")
                    )
                } else {
                    format!("Booting MCP server: {first}")
                };
                self.set_status_header(header);
            }
        }
        self.request_redraw();
    }

    fn on_mcp_startup_complete(&mut self, ev: McpStartupCompleteEvent) {
        let mut parts = Vec::new();
        if !ev.failed.is_empty() {
            let failed_servers: Vec<_> = ev.failed.iter().map(|f| f.server.clone()).collect();
            parts.push(format!("failed: {}", failed_servers.join(", ")));
        }
        if !ev.cancelled.is_empty() {
            self.on_warning(format!(
                "MCP startup interrupted. The following servers were not initialized: {}",
                ev.cancelled.join(", ")
            ));
        }
        if !parts.is_empty() {
            self.on_warning(format!("MCP startup incomplete ({})", parts.join("; ")));
        }

        self.mcp_startup_status = None;
        self.bottom_pane.set_task_running(false);
        self.queued_auto_send_pending =
            !self.queued_user_messages.is_empty() && self.bottom_pane.composer_is_empty();
        self.maybe_send_next_queued_input();
        self.request_redraw();
    }

    /// Handle a turn aborted due to user interrupt (Esc).
    /// Keep any queued messages in the queue for later.
    fn on_interrupted_turn(&mut self, reason: TurnAbortReason) {
        // Finalize, log a gentle prompt, and clear running state.
        self.finalize_turn();

        if reason != TurnAbortReason::ReviewEnded {
            self.add_to_history(history_cell::new_error_event(
                "Conversation interrupted - tell the model what to do differently. Something went wrong? Hit `/feedback` to report the issue.".to_owned(),
            ));
        }

        self.request_redraw();
    }

    fn on_plan_update(&mut self, update: UpdatePlanArgs) {
        self.add_to_history(history_cell::new_plan_update(update));
    }

    fn on_exec_approval_request(&mut self, id: String, ev: ExecApprovalRequestEvent) {
        let id2 = id.clone();
        let ev2 = ev.clone();
        self.defer_or_handle(
            |q| q.push_exec_approval(id, ev),
            |s| s.handle_exec_approval_now(id2, ev2),
        );
    }

    fn on_apply_patch_approval_request(&mut self, id: String, ev: ApplyPatchApprovalRequestEvent) {
        let id2 = id.clone();
        let ev2 = ev.clone();
        self.defer_or_handle(
            |q| q.push_apply_patch_approval(id, ev),
            |s| s.handle_apply_patch_approval_now(id2, ev2),
        );
    }

    fn on_elicitation_request(&mut self, ev: ElicitationRequestEvent) {
        let ev2 = ev.clone();
        self.defer_or_handle(
            |q| q.push_elicitation(ev),
            |s| s.handle_elicitation_request_now(ev2),
        );
    }

    fn on_exec_command_begin(&mut self, ev: ExecCommandBeginEvent) {
        self.flush_answer_stream_with_separator();
        if is_unified_exec_source(ev.source) {
            self.track_unified_exec_session_begin(&ev);
            if !is_standard_tool_call(&ev.parsed_cmd) {
                return;
            }
        }
        let ev2 = ev.clone();
        self.defer_or_handle(|q| q.push_exec_begin(ev), |s| s.handle_exec_begin_now(ev2));
    }

    fn on_exec_command_output_delta(
        &mut self,
        ev: codex_core::protocol::ExecCommandOutputDeltaEvent,
    ) {
        if self.suppressed_exec_calls.contains(&ev.call_id) {
            return;
        }
        let delta = String::from_utf8_lossy(&ev.chunk).to_string();
        if delta.is_empty() {
            return;
        }
        let appended = self
            .active_cell
            .as_mut()
            .and_then(|cell| cell.as_any_mut().downcast_mut::<ExecCell>())
            .is_some_and(|cell| cell.append_live_output(&ev.call_id, &delta));
        if appended {
            self.request_redraw();
        }
    }

    fn on_terminal_interaction(&mut self, ev: TerminalInteractionEvent) {
        self.flush_answer_stream_with_separator();
        let command_display = self
            .unified_exec_sessions
            .iter()
            .find(|session| session.key == ev.process_id)
            .map(|session| session.command_display.clone());
        self.add_to_history(history_cell::new_unified_exec_interaction(
            command_display,
            ev.stdin,
        ));
    }

    fn on_patch_apply_begin(&mut self, event: PatchApplyBeginEvent) {
        self.add_to_history(history_cell::new_patch_event(
            event.changes,
            &self.config.cwd,
            self.config.diff_view,
        ));
    }

    fn on_view_image_tool_call(&mut self, event: ViewImageToolCallEvent) {
        self.flush_answer_stream_with_separator();
        self.add_to_history(history_cell::new_view_image_tool_call(
            event.path,
            &self.config.cwd,
        ));
        self.request_redraw();
    }

    fn on_patch_apply_end(&mut self, event: codex_core::protocol::PatchApplyEndEvent) {
        let ev2 = event.clone();
        self.defer_or_handle(
            |q| q.push_patch_end(event),
            |s| s.handle_patch_apply_end_now(ev2),
        );
    }

    fn on_exec_command_end(&mut self, ev: ExecCommandEndEvent) {
        if is_unified_exec_source(ev.source) {
            self.track_unified_exec_session_end(&ev);
            if !self.bottom_pane.is_task_running() {
                return;
            }
        }
        let ev2 = ev.clone();
        self.defer_or_handle(|q| q.push_exec_end(ev), |s| s.handle_exec_end_now(ev2));
    }

    fn track_unified_exec_session_begin(&mut self, ev: &ExecCommandBeginEvent) {
        if ev.source != ExecCommandSource::UnifiedExecStartup {
            return;
        }
        let key = ev.process_id.clone().unwrap_or(ev.call_id.to_string());
        let command_display = strip_bash_lc_and_escape(&ev.command);
        if let Some(existing) = self
            .unified_exec_sessions
            .iter_mut()
            .find(|session| session.key == key)
        {
            existing.command_display = command_display;
        } else {
            self.unified_exec_sessions.push(UnifiedExecSessionSummary {
                key,
                command_display,
            });
        }
        self.sync_unified_exec_footer();
    }

    fn track_unified_exec_session_end(&mut self, ev: &ExecCommandEndEvent) {
        let key = ev.process_id.clone().unwrap_or(ev.call_id.to_string());
        let before = self.unified_exec_sessions.len();
        self.unified_exec_sessions
            .retain(|session| session.key != key);
        if self.unified_exec_sessions.len() != before {
            self.sync_unified_exec_footer();
        }
    }

    fn sync_unified_exec_footer(&mut self) {
        let sessions = self
            .unified_exec_sessions
            .iter()
            .map(|session| session.command_display.clone())
            .collect();
        self.bottom_pane.set_unified_exec_sessions(sessions);
    }

    fn on_mcp_tool_call_begin(&mut self, ev: McpToolCallBeginEvent) {
        let ev2 = ev.clone();
        self.defer_or_handle(|q| q.push_mcp_begin(ev), |s| s.handle_mcp_begin_now(ev2));
    }

    fn on_mcp_tool_call_end(&mut self, ev: McpToolCallEndEvent) {
        let ev2 = ev.clone();
        self.defer_or_handle(|q| q.push_mcp_end(ev), |s| s.handle_mcp_end_now(ev2));
    }

    fn on_web_search_begin(&mut self, _ev: WebSearchBeginEvent) {
        self.flush_answer_stream_with_separator();
    }

    fn on_web_search_end(&mut self, ev: WebSearchEndEvent) {
        self.flush_answer_stream_with_separator();
        self.add_to_history(history_cell::new_web_search_call(ev.query));
    }

    fn on_get_history_entry_response(
        &mut self,
        event: codex_core::protocol::GetHistoryEntryResponseEvent,
    ) {
        let codex_core::protocol::GetHistoryEntryResponseEvent {
            offset,
            log_id,
            entry,
        } = event;
        self.bottom_pane
            .on_history_entry_response(log_id, offset, entry.map(|e| e.text));
    }

    fn on_shutdown_complete(&mut self) {
        self.request_exit();
    }

    fn on_turn_diff(&mut self, unified_diff: String) {
        debug!("TurnDiffEvent: {unified_diff}");
    }

    fn on_deprecation_notice(&mut self, event: DeprecationNoticeEvent) {
        let DeprecationNoticeEvent { summary, details } = event;
        self.add_to_history(history_cell::new_deprecation_notice(summary, details));
        self.request_redraw();
    }

    fn on_background_event(&mut self, message: String) {
        debug!("BackgroundEvent: {message}");
        self.bottom_pane.ensure_status_indicator();
        self.bottom_pane.set_interrupt_hint_visible(true);
        self.set_status_header(message);
    }

    fn on_undo_started(&mut self, event: UndoStartedEvent) {
        self.bottom_pane.ensure_status_indicator();
        self.bottom_pane.set_interrupt_hint_visible(false);
        let message = event
            .message
            .unwrap_or_else(|| "Undo in progress...".to_string());
        self.set_status_header(message);
    }

    fn on_undo_completed(&mut self, event: UndoCompletedEvent) {
        let UndoCompletedEvent { success, message } = event;
        self.bottom_pane.hide_status_indicator();
        let message = message.unwrap_or_else(|| {
            if success {
                "Undo completed successfully.".to_string()
            } else {
                "Undo failed.".to_string()
            }
        });
        if success {
            self.add_info_message(message, None);
        } else {
            self.add_error_message(message);
        }
    }

    fn on_stream_error(&mut self, message: String, additional_details: Option<String>) {
        if self.retry_status_header.is_none() {
            self.retry_status_header = Some(self.current_status_header.clone());
        }
        self.set_status(message, additional_details);
    }

    /// Periodic tick to commit at most one queued line to history with a small delay,
    /// animating the output.
    pub(crate) fn on_commit_tick(&mut self) {
        if self.stream_paused {
            return;
        }

        if let Some(controller) = self.stream_controller.as_mut() {
            let (cell, is_idle) = controller.on_commit_tick();
            if let Some(cell) = cell {
                self.bottom_pane.hide_status_indicator();
                self.add_boxed_history(cell);
            }
            if is_idle {
                self.app_event_tx.send(AppEvent::StopCommitAnimation);
            }
        }
    }

    fn flush_interrupt_queue(&mut self) {
        let mut mgr = std::mem::take(&mut self.interrupts);
        mgr.flush_all(self);
        self.interrupts = mgr;
    }

    #[inline]
    fn defer_or_handle(
        &mut self,
        push: impl FnOnce(&mut InterruptManager),
        handle: impl FnOnce(&mut Self),
    ) {
        // Preserve deterministic FIFO across queued interrupts: once anything
        // is queued due to an active write cycle, continue queueing until the
        // queue is flushed to avoid reordering (e.g., ExecEnd before ExecBegin).
        if (self.stream_controller.is_some() && !self.stream_paused) || !self.interrupts.is_empty()
        {
            push(&mut self.interrupts);
        } else {
            handle(self);
        }
    }

    fn handle_stream_finished(&mut self) {
        if self.task_complete_pending {
            self.bottom_pane.hide_status_indicator();
            self.task_complete_pending = false;
        }
        // A completed stream indicates non-exec content was just inserted.
        self.flush_interrupt_queue();
    }

    #[inline]
    fn handle_streaming_delta(&mut self, delta: String) {
        self.record_first_response();
        if self.stream_paused {
            self.paused_agent_deltas.push_str(&delta);
            return;
        }

        // Before streaming agent content, flush any active exec cell group.
        self.flush_active_cell();

        if self.stream_controller.is_none() {
            if self.needs_final_message_separator {
                let elapsed_seconds = self
                    .bottom_pane
                    .status_widget()
                    .map(super::status_indicator_widget::StatusIndicatorWidget::elapsed_seconds);
                self.add_to_history(history_cell::FinalMessageSeparator::new(elapsed_seconds));
                self.needs_final_message_separator = false;
            }
            self.stream_controller = Some(StreamController::new(
                self.last_rendered_width.get().map(|w| w.saturating_sub(2)),
            ));
        }
        if let Some(controller) = self.stream_controller.as_mut()
            && controller.push(&delta)
        {
            self.app_event_tx.send(AppEvent::StartCommitAnimation);
        }
        self.request_redraw();
    }

    pub(crate) fn handle_exec_end_now(&mut self, ev: ExecCommandEndEvent) {
        let running = self.running_commands.remove(&ev.call_id);
        if self.suppressed_exec_calls.remove(&ev.call_id) {
            return;
        }
        let (command, parsed, source) = match running {
            Some(rc) => (rc.command, rc.parsed_cmd, rc.source),
            None => (ev.command.clone(), ev.parsed_cmd.clone(), ev.source),
        };
        let is_unified_exec_interaction =
            matches!(source, ExecCommandSource::UnifiedExecInteraction);
        if self.bottom_pane.is_task_running()
            && matches!(
                source,
                ExecCommandSource::Agent
                    | ExecCommandSource::UnifiedExecStartup
                    | ExecCommandSource::UnifiedExecInteraction
            )
        {
            self.record_tool_time(ev.duration);
        }

        let needs_new = self
            .active_cell
            .as_ref()
            .map(|cell| cell.as_any().downcast_ref::<ExecCell>().is_none())
            .unwrap_or(true);
        if needs_new {
            self.flush_active_cell();
            self.active_cell = Some(Box::new(new_active_exec_command(
                ev.call_id.clone(),
                command,
                parsed,
                source,
                ev.interaction_input.clone(),
                self.config.animations,
            )));
        }

        if let Some(cell) = self
            .active_cell
            .as_mut()
            .and_then(|c| c.as_any_mut().downcast_mut::<ExecCell>())
        {
            let output = if is_unified_exec_interaction {
                CommandOutput {
                    exit_code: ev.exit_code,
                    formatted_output: String::new(),
                    aggregated_output: String::new(),
                }
            } else {
                CommandOutput {
                    exit_code: ev.exit_code,
                    formatted_output: ev.formatted_output.clone(),
                    aggregated_output: ev.aggregated_output.clone(),
                }
            };
            cell.complete_call(&ev.call_id, output, ev.duration);
            if cell.should_flush() {
                self.flush_active_cell();
            }
        }
    }

    pub(crate) fn handle_patch_apply_end_now(
        &mut self,
        event: codex_core::protocol::PatchApplyEndEvent,
    ) {
        // If the patch was successful, just let the "Edited" block stand.
        // Otherwise, add a failure block.
        if !event.success {
            self.add_to_history(history_cell::new_patch_apply_failure(event.stderr));
        }
    }

    pub(crate) fn handle_exec_approval_now(&mut self, id: String, ev: ExecApprovalRequestEvent) {
        self.flush_answer_stream_with_separator();
        let command = shlex::try_join(ev.command.iter().map(String::as_str))
            .unwrap_or_else(|_| ev.command.join(" "));
        self.notify(Notification::ExecApprovalRequested { command });

        let request = ApprovalRequest::Exec {
            id,
            command: ev.command,
            reason: ev.reason,
            proposed_execpolicy_amendment: ev.proposed_execpolicy_amendment,
        };
        self.bottom_pane
            .push_approval_request(request, &self.config.features);
        self.request_redraw();
    }

    pub(crate) fn handle_apply_patch_approval_now(
        &mut self,
        id: String,
        ev: ApplyPatchApprovalRequestEvent,
    ) {
        self.flush_answer_stream_with_separator();

        let request = ApprovalRequest::ApplyPatch {
            id,
            reason: ev.reason,
            changes: ev.changes.clone(),
            cwd: self.config.cwd.clone(),
            diff_view: self.config.diff_view,
        };
        self.bottom_pane
            .push_approval_request(request, &self.config.features);
        self.request_redraw();
        self.notify(Notification::EditApprovalRequested {
            cwd: self.config.cwd.clone(),
            changes: ev.changes.keys().cloned().collect(),
        });
    }

    pub(crate) fn handle_elicitation_request_now(&mut self, ev: ElicitationRequestEvent) {
        self.flush_answer_stream_with_separator();

        self.notify(Notification::ElicitationRequested {
            server_name: ev.server_name.clone(),
        });

        let request = ApprovalRequest::McpElicitation {
            server_name: ev.server_name,
            request_id: ev.id,
            message: ev.message,
        };
        self.bottom_pane
            .push_approval_request(request, &self.config.features);
        self.request_redraw();
    }

    pub(crate) fn handle_exec_begin_now(&mut self, ev: ExecCommandBeginEvent) {
        // Ensure the status indicator is visible while the command runs.
        self.running_commands.insert(
            ev.call_id.clone(),
            RunningCommand {
                command: ev.command.clone(),
                parsed_cmd: ev.parsed_cmd.clone(),
                source: ev.source,
            },
        );
        let is_wait_interaction = matches!(ev.source, ExecCommandSource::UnifiedExecInteraction)
            && ev
                .interaction_input
                .as_deref()
                .map(str::is_empty)
                .unwrap_or(true);
        let command_display = ev.command.join(" ");
        let should_suppress_unified_wait = is_wait_interaction
            && self
                .last_unified_wait
                .as_ref()
                .is_some_and(|wait| wait.is_duplicate(&command_display));
        if is_wait_interaction {
            self.last_unified_wait = Some(UnifiedExecWaitState::new(command_display));
        } else {
            self.last_unified_wait = None;
        }
        if should_suppress_unified_wait {
            self.suppressed_exec_calls.insert(ev.call_id);
            return;
        }
        let interaction_input = ev.interaction_input.clone();
        if let Some(cell) = self
            .active_cell
            .as_mut()
            .and_then(|c| c.as_any_mut().downcast_mut::<ExecCell>())
            && let Some(new_exec) = cell.with_added_call(
                ev.call_id.clone(),
                ev.command.clone(),
                ev.parsed_cmd.clone(),
                ev.source,
                interaction_input.clone(),
            )
        {
            *cell = new_exec;
        } else {
            self.flush_active_cell();

            self.active_cell = Some(Box::new(new_active_exec_command(
                ev.call_id.clone(),
                ev.command.clone(),
                ev.parsed_cmd,
                ev.source,
                interaction_input,
                self.config.animations,
            )));
        }

        self.request_redraw();
    }

    pub(crate) fn handle_mcp_begin_now(&mut self, ev: McpToolCallBeginEvent) {
        self.flush_answer_stream_with_separator();
        self.flush_active_cell();
        self.active_cell = Some(Box::new(history_cell::new_active_mcp_tool_call(
            ev.call_id,
            ev.invocation,
            self.config.animations,
        )));
        self.request_redraw();
    }
    pub(crate) fn handle_mcp_end_now(&mut self, ev: McpToolCallEndEvent) {
        self.flush_answer_stream_with_separator();

        let McpToolCallEndEvent {
            call_id,
            invocation,
            duration,
            result,
        } = ev;

        if self.bottom_pane.is_task_running() {
            self.record_tool_time(duration);
        }

        let extra_cell = match self
            .active_cell
            .as_mut()
            .and_then(|cell| cell.as_any_mut().downcast_mut::<McpToolCallCell>())
        {
            Some(cell) if cell.call_id() == call_id => cell.complete(duration, result),
            _ => {
                self.flush_active_cell();
                let mut cell = history_cell::new_active_mcp_tool_call(
                    call_id,
                    invocation,
                    self.config.animations,
                );
                let extra_cell = cell.complete(duration, result);
                self.active_cell = Some(Box::new(cell));
                extra_cell
            }
        };

        self.flush_active_cell();
        if let Some(extra) = extra_cell {
            self.add_boxed_history(extra);
        }
    }

    pub(crate) fn new(
        common: ChatWidgetInit,
        conversation_manager: Arc<ConversationManager>,
    ) -> Self {
        let ChatWidgetInit {
            config,
            frame_requester,
            app_event_tx,
            initial_prompt,
            initial_images,
            enhanced_keys_supported,
            auth_manager,
            models_manager,
            feedback,
            is_first_run,
            model_family,
        } = common;
        let model_slug = model_family.get_model_slug().to_string();
        let mut config = config;
        config.model = Some(model_slug.clone());

        #[cfg(target_os = "linux")]
        let is_wsl = crate::clipboard_paste::is_probably_wsl();
        #[cfg(not(target_os = "linux"))]
        let is_wsl = false;

        let keybindings =
            Keybindings::from_config(&config.keybindings, enhanced_keys_supported, is_wsl);

        let mut rng = rand::rng();
        let placeholder = EXAMPLE_PROMPTS[rng.random_range(0..EXAMPLE_PROMPTS.len())].to_string();
        let codex_op_tx = spawn_agent(config.clone(), app_event_tx.clone(), conversation_manager);

        let mut widget = Self {
            app_event_tx: app_event_tx.clone(),
            frame_requester: frame_requester.clone(),
            codex_op_tx,
            bottom_pane: BottomPane::new(BottomPaneParams {
                frame_requester,
                app_event_tx,
                has_input_focus: true,
                enhanced_keys_supported,
                placeholder_text: placeholder,
                disable_paste_burst: config.disable_paste_burst,
                animations_enabled: config.animations,
                skills: None,
                keybindings: keybindings.clone(),
                status_line_items: config.status_line_items.clone(),
                status_line_cwd: config.cwd.clone(),
            }),
            active_cell: None,
            config,
            keybindings,
            model_family,
            auth_manager,
            models_manager,
            session_header: SessionHeader::new(model_slug.clone()),
            initial_user_message: create_initial_user_message(
                initial_prompt.unwrap_or_default(),
                initial_images,
            ),
            token_info: None,
            rate_limit_snapshot: None,
            plan_type: None,
            rate_limit_warnings: RateLimitWarningState::default(),
            rate_limit_switch_prompt: RateLimitSwitchPromptState::default(),
            rate_limit_poller: None,
            stream_controller: None,
            stream_paused: false,
            paused_status_header: None,
            paused_agent_deltas: String::new(),
            paused_reasoning_blocks: Vec::new(),
            paused_pending_answer_flush: false,
            running_commands: HashMap::new(),
            suppressed_exec_calls: HashSet::new(),
            last_unified_wait: None,
            task_complete_pending: false,
            unified_exec_sessions: Vec::new(),
            mcp_startup_status: None,
            interrupts: InterruptManager::new(),
            reasoning_buffer: String::new(),
            full_reasoning_buffer: String::new(),
            current_status_header: String::from("Working"),
            retry_status_header: None,
            status_line_metrics: StatusLineMetrics::default(),
            turn_metrics: TurnMetrics::default(),
            conversation_id: None,
            queued_user_messages: VecDeque::new(),
            next_queued_user_message_id: 1,
            queued_edit_state: None,
            queued_auto_send_pending: false,
            show_welcome_banner: is_first_run,
            suppress_session_configured_redraw: false,
            pending_notification: None,
            notification_focus_override: None,
            is_review_mode: false,
            session_mode: SessionMode::Normal,
            pre_review_token_info: None,
            needs_final_message_separator: false,
            last_rendered_width: std::cell::Cell::new(None),
            feedback,
            current_rollout_path: None,
            session_title: None,
            external_editor_state: ExternalEditorState::Closed,
            pending_active_turn_context: None,
            active_turn_context: None,
        };

        widget.prefetch_rate_limits();
        widget.bottom_pane.set_session_model(model_slug);
        widget
            .bottom_pane
            .set_session_reasoning_effort(widget.effective_reasoning_effort());
        Self::refresh_status_line_git_branch(
            widget.app_event_tx.clone(),
            widget.config.cwd.clone(),
        );
        widget
    }

    /// Create a ChatWidget attached to an existing conversation (e.g., a fork).
    pub(crate) fn new_from_existing(
        common: ChatWidgetInit,
        conversation: std::sync::Arc<codex_core::CodexConversation>,
        session_configured: codex_core::protocol::SessionConfiguredEvent,
    ) -> Self {
        let ChatWidgetInit {
            config,
            frame_requester,
            app_event_tx,
            initial_prompt,
            initial_images,
            enhanced_keys_supported,
            auth_manager,
            models_manager,
            feedback,
            model_family,
            ..
        } = common;
        let model_slug = model_family.get_model_slug().to_string();
        let mut rng = rand::rng();
        let placeholder = EXAMPLE_PROMPTS[rng.random_range(0..EXAMPLE_PROMPTS.len())].to_string();

        let session_mode = session_configured.mode;
        let codex_op_tx =
            spawn_agent_from_existing(conversation, session_configured, app_event_tx.clone());

        #[cfg(target_os = "linux")]
        let is_wsl = crate::clipboard_paste::is_probably_wsl();
        #[cfg(not(target_os = "linux"))]
        let is_wsl = false;

        let keybindings =
            Keybindings::from_config(&config.keybindings, enhanced_keys_supported, is_wsl);

        let mut widget = Self {
            app_event_tx: app_event_tx.clone(),
            frame_requester: frame_requester.clone(),
            codex_op_tx,
            bottom_pane: BottomPane::new(BottomPaneParams {
                frame_requester,
                app_event_tx,
                has_input_focus: true,
                enhanced_keys_supported,
                placeholder_text: placeholder,
                disable_paste_burst: config.disable_paste_burst,
                animations_enabled: config.animations,
                skills: None,
                keybindings: keybindings.clone(),
                status_line_items: config.status_line_items.clone(),
                status_line_cwd: config.cwd.clone(),
            }),
            active_cell: None,
            config,
            keybindings,
            model_family,
            auth_manager,
            models_manager,
            session_header: SessionHeader::new(model_slug.clone()),
            initial_user_message: create_initial_user_message(
                initial_prompt.unwrap_or_default(),
                initial_images,
            ),
            token_info: None,
            rate_limit_snapshot: None,
            plan_type: None,
            rate_limit_warnings: RateLimitWarningState::default(),
            rate_limit_switch_prompt: RateLimitSwitchPromptState::default(),
            rate_limit_poller: None,
            stream_controller: None,
            stream_paused: false,
            paused_status_header: None,
            paused_agent_deltas: String::new(),
            paused_reasoning_blocks: Vec::new(),
            paused_pending_answer_flush: false,
            running_commands: HashMap::new(),
            suppressed_exec_calls: HashSet::new(),
            last_unified_wait: None,
            task_complete_pending: false,
            unified_exec_sessions: Vec::new(),
            mcp_startup_status: None,
            interrupts: InterruptManager::new(),
            reasoning_buffer: String::new(),
            full_reasoning_buffer: String::new(),
            current_status_header: String::from("Working"),
            retry_status_header: None,
            status_line_metrics: StatusLineMetrics::default(),
            turn_metrics: TurnMetrics::default(),
            conversation_id: None,
            queued_user_messages: VecDeque::new(),
            next_queued_user_message_id: 1,
            queued_edit_state: None,
            queued_auto_send_pending: false,
            show_welcome_banner: false,
            suppress_session_configured_redraw: true,
            pending_notification: None,
            notification_focus_override: None,
            is_review_mode: false,
            session_mode,
            pre_review_token_info: None,
            needs_final_message_separator: false,
            last_rendered_width: std::cell::Cell::new(None),
            feedback,
            current_rollout_path: None,
            session_title: None,
            external_editor_state: ExternalEditorState::Closed,
            pending_active_turn_context: None,
            active_turn_context: None,
        };

        widget.prefetch_rate_limits();
        widget.bottom_pane.set_session_model(model_slug);
        widget
            .bottom_pane
            .set_session_reasoning_effort(widget.effective_reasoning_effort());
        Self::refresh_status_line_git_branch(
            widget.app_event_tx.clone(),
            widget.config.cwd.clone(),
        );
        widget
    }

    pub(crate) fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                ..
            } if !self.bottom_pane.has_active_view()
                && !self.bottom_pane.composer_popup_active() =>
            {
                self.cycle_thinking_effort(1);
                return;
            }
            KeyEvent {
                code: KeyCode::BackTab,
                kind: KeyEventKind::Press,
                ..
            }
            | KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::SHIFT,
                kind: KeyEventKind::Press,
                ..
            } if !self.bottom_pane.has_active_view()
                && !self.bottom_pane.composer_popup_active() =>
            {
                self.cycle_thinking_effort(-1);
                return;
            }
            KeyEvent {
                code: KeyCode::Right,
                modifiers,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } if modifiers == (KeyModifiers::CONTROL | KeyModifiers::SHIFT)
                && !self.bottom_pane.has_active_view()
                && !self.bottom_pane.composer_popup_active() =>
            {
                self.cycle_model(1);
                return;
            }
            KeyEvent {
                code: KeyCode::Left,
                modifiers,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } if modifiers == (KeyModifiers::CONTROL | KeyModifiers::SHIFT)
                && !self.bottom_pane.has_active_view()
                && !self.bottom_pane.composer_popup_active() =>
            {
                self.cycle_model(-1);
                return;
            }
            KeyEvent {
                code: KeyCode::Char('p' | 'P'),
                modifiers: KeyModifiers::ALT,
                kind: KeyEventKind::Press,
                ..
            } if !self.bottom_pane.has_active_view()
                && !self.bottom_pane.composer_popup_active() =>
            {
                self.set_session_mode(SessionMode::Plan);
                return;
            }
            KeyEvent {
                code: KeyCode::Char('a' | 'A'),
                modifiers: KeyModifiers::ALT,
                kind: KeyEventKind::Press,
                ..
            } if !self.bottom_pane.has_active_view()
                && !self.bottom_pane.composer_popup_active() =>
            {
                self.set_session_mode(SessionMode::Ask);
                return;
            }
            KeyEvent {
                code: KeyCode::Char('n' | 'N'),
                modifiers: KeyModifiers::ALT,
                kind: KeyEventKind::Press,
                ..
            } if !self.bottom_pane.has_active_view()
                && !self.bottom_pane.composer_popup_active() =>
            {
                self.set_session_mode(SessionMode::Normal);
                return;
            }
            KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('\u{0010}'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                ..
            } if !self.bottom_pane.has_active_view() => {
                self.toggle_stream_pause();
                return;
            }
            KeyEvent {
                code: KeyCode::Char('o' | 'O'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('\u{000f}'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                ..
            } if !self.bottom_pane.has_active_view()
                && !self.queued_user_messages.is_empty()
                && self.queued_edit_state.is_none() =>
            {
                self.open_queue_popup();
                return;
            }
            KeyEvent {
                code: KeyCode::Char('k' | 'K'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } if !self.bottom_pane.has_active_view()
                && !self.bottom_pane.composer_popup_active()
                && self.queued_edit_state.is_none() =>
            {
                self.open_model_popup();
                return;
            }
            KeyEvent {
                code: KeyCode::Char('\u{000b}'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                ..
            } if !self.bottom_pane.has_active_view()
                && !self.bottom_pane.composer_popup_active()
                && self.queued_edit_state.is_none() =>
            {
                self.open_model_popup();
                return;
            }
            KeyEvent {
                code: KeyCode::Char('y' | 'Y'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('\u{0019}'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                ..
            } if !self.bottom_pane.has_active_view()
                && !self.bottom_pane.composer_popup_active()
                && !self.bottom_pane.is_task_running()
                && !self.queued_user_messages.is_empty()
                && self.queued_edit_state.is_none() =>
            {
                self.send_next_queued_user_message();
                return;
            }
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                kind: KeyEventKind::Press,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) && c.eq_ignore_ascii_case(&'c') => {
                self.on_ctrl_c();
                return;
            }
            key_event if self.handle_live_output_scroll_key(key_event) => {
                return;
            }
            key_event
                if key_event.kind == KeyEventKind::Press
                    && self.keybindings.paste.iter().any(|b| b.matches(&key_event)) =>
            {
                self.paste_from_clipboard();
                return;
            }
            key_event
                if key_event.kind == KeyEventKind::Press
                    && self
                        .keybindings
                        .copy_prompt
                        .iter()
                        .any(|b| b.matches(&key_event)) =>
            {
                self.copy_prompt_to_clipboard();
                return;
            }
            other if other.kind == KeyEventKind::Press => {
                self.bottom_pane.clear_ctrl_c_quit_hint();
            }
            _ => {}
        }

        if self.queued_edit_state.is_some()
            && !self.bottom_pane.has_active_view()
            && self.handle_queue_edit_key_event(key_event)
        {
            return;
        }

        match key_event {
            KeyEvent {
                code: KeyCode::Char('q' | 'Q'),
                modifiers: KeyModifiers::ALT,
                kind: KeyEventKind::Press,
                ..
            } if !self.queued_user_messages.is_empty() && self.queued_edit_state.is_none() => {
                self.open_queue_popup();
            }
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::ALT,
                kind: KeyEventKind::Press,
                ..
            } if !self.queued_user_messages.is_empty() => {
                self.begin_queue_edit_most_recent();
            }
            _ => match self.bottom_pane.handle_key_event(key_event) {
                InputResult::Submitted(text) => {
                    let attachments = self.bottom_pane.take_recent_submission_attachments();
                    self.queue_user_message(text, attachments);
                }
                InputResult::Command(cmd) => {
                    let command_input = self.bottom_pane.take_last_command_input();
                    self.dispatch_command(cmd, command_input);
                }
                InputResult::None => {}
            },
        }

        self.maybe_send_next_queued_input();
    }

    fn handle_live_output_scroll_key(&mut self, key_event: KeyEvent) -> bool {
        if key_event.kind != KeyEventKind::Press {
            return false;
        }
        if self.bottom_pane.has_active_view() || self.bottom_pane.composer_popup_active() {
            return false;
        }
        if !self.bottom_pane.is_task_running() {
            return false;
        }
        let action = match key_event.code {
            KeyCode::PageUp => LiveOutputScrollAction::PageUp,
            KeyCode::PageDown => LiveOutputScrollAction::PageDown,
            _ => return false,
        };
        let Some(width) = self.last_rendered_width.get() else {
            return false;
        };
        let Some(exec) = self
            .active_cell
            .as_mut()
            .and_then(|cell| cell.as_any_mut().downcast_mut::<ExecCell>())
        else {
            return false;
        };
        if !exec.has_live_output_box() {
            return false;
        }
        if exec.scroll_live_output(width as u16, action) {
            self.request_redraw();
        }
        true
    }

    fn cycle_model(&mut self, direction: isize) {
        let current_model = self.model_family.get_model_slug().to_string();
        let presets: Vec<ModelPreset> =
            // todo(aibrahim): make this async function
            match self.models_manager.try_list_models(&self.config) {
                Ok(models) => models,
                Err(_) => {
                    self.add_info_message(
                        "Models are being updated; please try again in a moment.".to_string(),
                        None,
                    );
                    return;
                }
            };

        let (mut auto_presets, other_presets): (Vec<ModelPreset>, Vec<ModelPreset>) = presets
            .into_iter()
            .partition(|preset| Self::is_auto_model(&preset.model));

        let choices = if auto_presets.is_empty() {
            let mut presets = other_presets;
            presets.sort_by(|a, b| a.display_name.cmp(&b.display_name));
            presets
        } else {
            auto_presets.sort_by_key(|preset| Self::auto_model_order(&preset.model));
            if auto_presets
                .iter()
                .any(|preset| preset.model == current_model)
            {
                auto_presets
            } else {
                let current_preset = other_presets
                    .iter()
                    .find(|preset| preset.model == current_model)
                    .cloned();
                if let Some(current_preset) = current_preset {
                    auto_presets.insert(0, current_preset);
                }
                auto_presets
            }
        };

        if choices.len() <= 1 {
            return;
        }

        let current_idx = choices
            .iter()
            .position(|preset| preset.model == current_model)
            .unwrap_or(0);

        let len = choices.len() as isize;
        let next_idx = (current_idx as isize + direction).rem_euclid(len) as usize;
        let next = choices[next_idx].clone();
        let next_model = next.model.to_string();
        let next_effort = Some(next.default_reasoning_effort);

        self.app_event_tx
            .send(AppEvent::CodexOp(Op::OverrideTurnContext {
                cwd: None,
                approval_policy: None,
                sandbox_policy: None,
                model: Some(next_model.clone()),
                effort: Some(next_effort),
                summary: None,
                mode: None,
            }));

        self.app_event_tx
            .send(AppEvent::UpdateModel(next_model.clone()));

        self.app_event_tx
            .send(AppEvent::UpdateReasoningEffort(next_effort));

        self.app_event_tx.send(AppEvent::PersistModelSelection {
            model: next_model.clone(),
            effort: next_effort,
        });

        let effort_label = next_effort
            .map(|effort| effort.to_string())
            .unwrap_or_else(|| "default".to_string());
        tracing::info!("Selected model: {next_model}, Selected effort: {effort_label}");
    }

    fn cycle_thinking_effort(&mut self, direction: isize) {
        let model_slug = self.model_family.get_model_slug().to_string();
        let current_effort = self.config.model_reasoning_effort;

        let presets = match self.models_manager.try_list_models(&self.config) {
            Ok(presets) => presets,
            Err(_) => {
                self.add_info_message(
                    "Models are being updated; please try again in a moment.".to_string(),
                    None,
                );
                return;
            }
        };

        let Some(preset) = presets
            .into_iter()
            .find(|preset| preset.model == model_slug)
        else {
            self.add_info_message(
                format!("Model '{model_slug}' is not available right now."),
                None,
            );
            return;
        };

        let default_effort = preset.default_reasoning_effort;
        let mut supported: HashSet<ReasoningEffortConfig> = preset
            .supported_reasoning_efforts
            .into_iter()
            .map(|option| option.effort)
            .collect();
        supported.insert(default_effort);

        // Note: codex-core always normalizes "unset" effort to an explicit
        // effective effort (usually the model default), so cycling must avoid
        // a `None` slot or it will become unreachable and break wrap-around.
        let choices: Vec<ReasoningEffortConfig> = ReasoningEffortConfig::iter()
            .filter(|effort| *effort != ReasoningEffortConfig::None && supported.contains(effort))
            .collect();

        if choices.len() <= 1 {
            return;
        }

        let current_idx = current_effort
            .and_then(|effort| choices.iter().position(|choice| *choice == effort))
            .or_else(|| choices.iter().position(|choice| *choice == default_effort))
            .unwrap_or(0);

        let len = choices.len() as isize;
        let next_idx = (current_idx as isize + direction).rem_euclid(len) as usize;
        let next_effort = Some(choices[next_idx]);

        self.app_event_tx
            .send(AppEvent::CodexOp(Op::OverrideTurnContext {
                cwd: None,
                approval_policy: None,
                sandbox_policy: None,
                model: None,
                effort: Some(next_effort),
                summary: None,
                mode: None,
            }));

        self.app_event_tx
            .send(AppEvent::UpdateReasoningEffort(next_effort));

        self.app_event_tx.send(AppEvent::PersistModelSelection {
            model: self.model_family.get_model_slug().to_string(),
            effort: next_effort,
        });

        let effort_label = next_effort
            .map(|effort| effort.to_string())
            .unwrap_or_else(|| "default".to_string());
        tracing::info!("Selected effort: {effort_label}");
    }

    fn paste_from_clipboard(&mut self) {
        let active_view = self.bottom_pane.has_active_view();

        let mut image_error: Option<PasteImageError> = None;
        if !active_view {
            match paste_image_to_temp_png() {
                Ok((path, info)) => {
                    self.attach_image(path, info.width, info.height, info.encoded_format.label());
                    return;
                }
                Err(err) => {
                    image_error = Some(err);
                }
            }
        }

        match paste_text_from_clipboard() {
            Ok(text) => {
                self.bottom_pane.handle_paste(text);
            }
            Err(err) => {
                let message = if let Some(img_err) = image_error {
                    format!("Failed to paste from clipboard: {img_err}; {err}")
                } else {
                    format!("Failed to paste from clipboard: {err}")
                };

                self.add_to_history(history_cell::new_error_event(message));
                self.request_redraw();
            }
        }
    }

    fn copy_prompt_to_clipboard(&mut self) {
        if self.bottom_pane.has_active_view() {
            return;
        }

        let text = self.bottom_pane.composer_text();
        if text.trim().is_empty() {
            return;
        }

        match copy_text_to_clipboard(&text) {
            Ok(()) => {
                self.bottom_pane
                    .show_status_line_notice("Prompt copied".to_string(), Duration::from_secs(2));
            }
            Err(err) => {
                self.add_to_history(history_cell::new_error_event(format!(
                    "Failed to copy prompt to clipboard: {err}",
                )));
                self.request_redraw();
            }
        }
    }

    fn handle_queue_edit_key_event(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent {
                code: KeyCode::Esc,
                kind: KeyEventKind::Press,
                ..
            } => {
                self.exit_queue_edit(false);
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                ..
            } => {
                self.exit_queue_edit(true);
                true
            }
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::ALT,
                kind: KeyEventKind::Press,
                ..
            } => {
                self.switch_queue_edit(-1);
                true
            }
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::ALT,
                kind: KeyEventKind::Press,
                ..
            } => {
                self.switch_queue_edit(1);
                true
            }
            KeyEvent {
                code: KeyCode::Char('m' | 'M'),
                modifiers: KeyModifiers::ALT,
                kind: KeyEventKind::Press,
                ..
            } => {
                if let Some(id) = self
                    .queued_edit_state
                    .as_ref()
                    .map(|state| state.selected_id)
                {
                    self.open_queue_model_picker(id);
                }
                true
            }
            KeyEvent {
                code: KeyCode::Char('t' | 'T'),
                modifiers: KeyModifiers::ALT,
                kind: KeyEventKind::Press,
                ..
            } => {
                if let Some(id) = self
                    .queued_edit_state
                    .as_ref()
                    .map(|state| state.selected_id)
                {
                    self.open_queue_thinking_picker(id);
                }
                true
            }
            _ => false,
        }
    }

    fn queue_popup_items(&self) -> Vec<QueuePopupItem> {
        let model_presets = self.models_manager.try_list_models(&self.config).ok();

        self.queued_user_messages
            .iter()
            .map(|message| {
                let mut preview = message
                    .text
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if preview.is_empty() && !message.attachments.is_empty() {
                    preview = "[image]".to_string();
                }

                let effective_model = message
                    .model_override
                    .as_deref()
                    .unwrap_or_else(|| self.model_family.get_model_slug());
                let default_effort = model_presets.as_ref().and_then(|presets| {
                    presets
                        .iter()
                        .find(|preset| preset.model == effective_model)
                        .map(|preset| preset.default_reasoning_effort)
                });
                let effective_effort = match message.effort_override {
                    Some(effort) => effort.or(default_effort),
                    None => self.config.model_reasoning_effort.or(default_effort),
                };

                let mut meta_parts: Vec<String> = Vec::new();
                if !message.attachments.is_empty() {
                    meta_parts.push("img".to_string());
                }
                meta_parts.push(format!("model: {effective_model}"));
                meta_parts.push(format!(
                    "thinking: {}",
                    effective_effort
                        .map(|effort| effort.to_string())
                        .unwrap_or_else(|| "default".to_string())
                ));

                QueuePopupItem {
                    id: message.id,
                    preview,
                    meta: (!meta_parts.is_empty()).then(|| meta_parts.join(" · ")),
                }
            })
            .collect()
    }

    fn open_queue_popup(&mut self) {
        if self.bottom_pane.has_active_view()
            || self.queued_user_messages.is_empty()
            || self.queued_edit_state.is_some()
        {
            return;
        }

        let items = self.queue_popup_items();

        self.bottom_pane
            .show_view(Box::new(QueuePopup::new(items, self.app_event_tx.clone())));
        self.request_redraw();
    }

    fn begin_queue_edit_most_recent(&mut self) {
        let Some(selected_id) = self.queued_user_messages.back().map(|message| message.id) else {
            return;
        };

        self.start_queue_edit(selected_id);
    }

    pub(crate) fn start_queue_edit(&mut self, id: u64) {
        if self.queued_edit_state.is_some()
            || self.queued_user_messages.is_empty()
            || self.bottom_pane.has_active_view()
            || self.bottom_pane.composer_popup_active()
        {
            return;
        }

        if !self
            .queued_user_messages
            .iter()
            .any(|message| message.id == id)
        {
            return;
        }

        let composer_before_edit = QueuedComposerSnapshot {
            text: self.bottom_pane.composer_text(),
            attachments: self.bottom_pane.composer_attachments(),
        };

        self.queued_edit_state = Some(QueuedEditState {
            selected_id: id,
            composer_before_edit,
            drafts: HashMap::new(),
        });

        self.bottom_pane.set_composer_commands_enabled(false);
        self.load_queue_edit_draft(id);
        self.update_queue_edit_footer_hint();
        self.refresh_queued_user_messages();
        self.request_redraw();
    }

    pub(crate) fn delete_queued_user_message(&mut self, id: u64) {
        let Some(idx) = self
            .queued_user_messages
            .iter()
            .position(|message| message.id == id)
        else {
            return;
        };

        self.queued_user_messages.remove(idx);
        if self.queued_user_messages.is_empty() {
            self.queued_auto_send_pending = false;
        }

        self.refresh_queued_user_messages();
        self.request_redraw();
    }

    pub(crate) fn move_queued_user_message_up(&mut self, id: u64) {
        let Some(idx) = self
            .queued_user_messages
            .iter()
            .position(|message| message.id == id)
        else {
            return;
        };

        if idx == 0 {
            return;
        }

        self.queued_user_messages.swap(idx, idx - 1);
        self.refresh_queued_user_messages();
        self.request_redraw();
    }

    pub(crate) fn move_queued_user_message_down(&mut self, id: u64) {
        let len = self.queued_user_messages.len();
        let Some(idx) = self
            .queued_user_messages
            .iter()
            .position(|message| message.id == id)
        else {
            return;
        };

        if idx + 1 >= len {
            return;
        }

        self.queued_user_messages.swap(idx, idx + 1);
        self.refresh_queued_user_messages();
        self.request_redraw();
    }

    pub(crate) fn move_queued_user_message_to_front(&mut self, id: u64) {
        let Some(idx) = self
            .queued_user_messages
            .iter()
            .position(|message| message.id == id)
        else {
            return;
        };

        if idx == 0 {
            return;
        }

        let Some(message) = self.queued_user_messages.remove(idx) else {
            return;
        };

        self.queued_user_messages.push_front(message);
        self.refresh_queued_user_messages();
        self.request_redraw();
    }

    pub(crate) fn open_queue_model_picker(&mut self, id: u64) {
        let current_override = self
            .queued_edit_state
            .as_ref()
            .and_then(|state| state.drafts.get(&id))
            .map(|draft| draft.model_override.clone())
            .unwrap_or_else(|| {
                self.queued_user_messages
                    .iter()
                    .find(|message| message.id == id)
                    .map(|message| message.model_override.clone())
                    .unwrap_or_default()
            });

        let session_model = self.model_family.get_model_slug().to_string();
        let mut items: Vec<SelectionItem> = Vec::new();

        let clear_actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
            tx.send(AppEvent::QueueSetModelOverride { id, model: None });
        })];
        items.push(SelectionItem {
            name: "Use session model".to_string(),
            description: Some(format!("Current session model: {session_model}")),
            is_current: current_override.is_none(),
            actions: clear_actions,
            dismiss_on_select: true,
            ..Default::default()
        });

        let mut presets = match self.models_manager.try_list_models(&self.config) {
            Ok(presets) => presets,
            Err(_) => {
                self.add_info_message(
                    "Models are being updated; please try again in a moment.".to_string(),
                    None,
                );
                return;
            }
        };
        presets.sort_by(|a, b| a.display_name.cmp(&b.display_name));

        for preset in presets {
            let model_slug = preset.model.clone();
            let is_current = current_override.as_deref() == Some(model_slug.as_str());
            let description = (!preset.description.is_empty()).then_some(preset.description);
            let model_for_action = model_slug.clone();
            let actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
                tx.send(AppEvent::QueueSetModelOverride {
                    id,
                    model: Some(model_for_action.clone()),
                });
            })];

            items.push(SelectionItem {
                name: preset.display_name,
                description,
                is_current,
                actions,
                dismiss_on_select: true,
                ..Default::default()
            });
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select Model for Queued Message".to_string()),
            subtitle: Some("Applies only when this message is sent.".to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            completion_behavior: ViewCompletionBehavior::Pop,
            items,
            ..Default::default()
        });
    }

    pub(crate) fn open_queue_thinking_picker(&mut self, id: u64) {
        let model_override = self
            .queued_edit_state
            .as_ref()
            .and_then(|state| state.drafts.get(&id))
            .map(|draft| draft.model_override.clone())
            .unwrap_or_else(|| {
                self.queued_user_messages
                    .iter()
                    .find(|message| message.id == id)
                    .map(|message| message.model_override.clone())
                    .unwrap_or_default()
            });

        let model_slug =
            model_override.unwrap_or_else(|| self.model_family.get_model_slug().to_string());
        let current_override = self
            .queued_edit_state
            .as_ref()
            .and_then(|state| state.drafts.get(&id))
            .map(|draft| draft.effort_override)
            .unwrap_or_else(|| {
                self.queued_user_messages
                    .iter()
                    .find(|message| message.id == id)
                    .map(|message| message.effort_override)
                    .unwrap_or_default()
            });

        let presets = match self.models_manager.try_list_models(&self.config) {
            Ok(presets) => presets,
            Err(_) => {
                self.add_info_message(
                    "Models are being updated; please try again in a moment.".to_string(),
                    None,
                );
                return;
            }
        };
        let Some(preset) = presets
            .into_iter()
            .find(|preset| preset.model == model_slug)
        else {
            self.add_info_message(
                format!("Model '{model_slug}' is not available right now."),
                None,
            );
            return;
        };

        let mut items: Vec<SelectionItem> = Vec::new();
        let clear_actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
            tx.send(AppEvent::QueueSetThinkingOverride { id, effort: None });
        })];
        items.push(SelectionItem {
            name: "Use session thinking".to_string(),
            description: Some("Inherit the current session reasoning level.".to_string()),
            is_current: current_override.is_none(),
            actions: clear_actions,
            dismiss_on_select: true,
            ..Default::default()
        });

        let default_effort = preset.default_reasoning_effort;
        let default_actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
            tx.send(AppEvent::QueueSetThinkingOverride {
                id,
                effort: Some(None),
            });
        })];
        items.push(SelectionItem {
            name: format!("Default ({})", Self::reasoning_effort_label(default_effort)),
            description: Some("Use the model's default reasoning level.".to_string()),
            is_current: matches!(current_override, Some(None)),
            actions: default_actions,
            dismiss_on_select: true,
            ..Default::default()
        });

        for option in preset.supported_reasoning_efforts {
            let effort = option.effort;
            let mut label = Self::reasoning_effort_label(effort).to_string();
            if effort == default_effort {
                label.push_str(" (default)");
            }
            let description = (!option.description.is_empty()).then_some(option.description);
            let is_current = current_override == Some(Some(effort));
            let actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
                tx.send(AppEvent::QueueSetThinkingOverride {
                    id,
                    effort: Some(Some(effort)),
                });
            })];
            items.push(SelectionItem {
                name: label,
                description,
                is_current,
                actions,
                dismiss_on_select: true,
                ..Default::default()
            });
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select Thinking for Queued Message".to_string()),
            subtitle: Some(format!("Model: {model_slug}")),
            footer_hint: Some(standard_popup_hint_line()),
            completion_behavior: ViewCompletionBehavior::Pop,
            items,
            ..Default::default()
        });
    }

    pub(crate) fn set_queued_user_message_model_override(
        &mut self,
        id: u64,
        model: Option<String>,
    ) {
        if self.queued_edit_state.is_some() {
            let selected_id = self
                .queued_edit_state
                .as_ref()
                .map(|state| state.selected_id);
            let base = self
                .queued_edit_state
                .as_ref()
                .and_then(|state| state.drafts.get(&id).cloned())
                .or_else(|| {
                    if selected_id == Some(id) {
                        self.capture_queue_edit_draft(id)
                    } else {
                        self.queued_user_message_draft(id)
                    }
                });
            if let Some(mut draft) = base {
                draft.model_override = model;
                if let Some(state) = self.queued_edit_state.as_mut() {
                    state.drafts.insert(id, draft);
                }
            }
            return;
        }

        let Some(message) = self
            .queued_user_messages
            .iter_mut()
            .find(|message| message.id == id)
        else {
            return;
        };

        message.model_override = model;
        self.refresh_queued_user_messages();
        self.bottom_pane
            .update_queue_popup_items(self.queue_popup_items());
        self.request_redraw();
    }

    pub(crate) fn set_queued_user_message_thinking_override(
        &mut self,
        id: u64,
        effort: Option<Option<ReasoningEffortConfig>>,
    ) {
        if self.queued_edit_state.is_some() {
            let selected_id = self
                .queued_edit_state
                .as_ref()
                .map(|state| state.selected_id);
            let base = self
                .queued_edit_state
                .as_ref()
                .and_then(|state| state.drafts.get(&id).cloned())
                .or_else(|| {
                    if selected_id == Some(id) {
                        self.capture_queue_edit_draft(id)
                    } else {
                        self.queued_user_message_draft(id)
                    }
                });
            if let Some(mut draft) = base {
                draft.effort_override = effort;
                if let Some(state) = self.queued_edit_state.as_mut() {
                    state.drafts.insert(id, draft);
                }
            }
            return;
        }

        let Some(message) = self
            .queued_user_messages
            .iter_mut()
            .find(|message| message.id == id)
        else {
            return;
        };

        message.effort_override = effort;
        self.refresh_queued_user_messages();
        self.bottom_pane
            .update_queue_popup_items(self.queue_popup_items());
        self.request_redraw();
    }

    fn exit_queue_edit(&mut self, save: bool) {
        let Some(mut state) = self.queued_edit_state.take() else {
            return;
        };

        if save {
            if let Some(draft) = self.capture_queue_edit_draft(state.selected_id) {
                state.drafts.insert(state.selected_id, draft);
            }

            for message in self.queued_user_messages.iter_mut() {
                if let Some(draft) = state.drafts.get(&message.id) {
                    message.text = draft.text.clone();
                    message.attachments = draft.attachments.clone();
                    message.model_override = draft.model_override.clone();
                    message.effort_override = draft.effort_override;
                }
            }
        }

        self.bottom_pane.set_composer_commands_enabled(true);
        self.bottom_pane.set_composer_footer_hint_override(None);
        self.bottom_pane.set_composer_text_with_attachments(
            state.composer_before_edit.text,
            state.composer_before_edit.attachments,
        );

        self.refresh_queued_user_messages();
        self.request_redraw();
        self.maybe_send_next_queued_input();
    }

    fn switch_queue_edit(&mut self, direction: isize) {
        let Some(selected_id) = self
            .queued_edit_state
            .as_ref()
            .map(|state| state.selected_id)
        else {
            return;
        };

        if let Some(draft) = self.capture_queue_edit_draft(selected_id)
            && let Some(state) = self.queued_edit_state.as_mut()
        {
            state.drafts.insert(selected_id, draft);
        }

        let Some(current_idx) = self
            .queued_user_messages
            .iter()
            .position(|message| message.id == selected_id)
        else {
            self.exit_queue_edit(false);
            return;
        };

        let len = self.queued_user_messages.len();
        if len == 0 {
            self.exit_queue_edit(false);
            return;
        }

        let next_idx = ((current_idx as isize + direction).rem_euclid(len as isize)) as usize;
        let Some(next_id) = self
            .queued_user_messages
            .get(next_idx)
            .map(|message| message.id)
        else {
            return;
        };

        let draft = self
            .queued_edit_state
            .as_ref()
            .and_then(|state| state.drafts.get(&next_id).cloned())
            .or_else(|| self.queued_user_message_draft(next_id));
        let Some(draft) = draft else {
            return;
        };

        if let Some(state) = self.queued_edit_state.as_mut() {
            state.selected_id = next_id;
        }

        self.bottom_pane
            .set_composer_text_with_attachments(draft.text, draft.attachments);
        self.update_queue_edit_footer_hint();
        self.refresh_queued_user_messages();
        self.request_redraw();
    }

    fn capture_queue_edit_draft(&self, id: u64) -> Option<QueuedUserMessageDraft> {
        let message = self
            .queued_user_messages
            .iter()
            .find(|message| message.id == id)?;

        let (model_override, effort_override) = self
            .queued_edit_state
            .as_ref()
            .and_then(|state| state.drafts.get(&id))
            .map(|draft| (draft.model_override.clone(), draft.effort_override))
            .unwrap_or_else(|| (message.model_override.clone(), message.effort_override));

        Some(QueuedUserMessageDraft {
            text: self.bottom_pane.composer_text(),
            attachments: self.bottom_pane.composer_attachments(),
            model_override,
            effort_override,
        })
    }

    fn queued_user_message_draft(&self, id: u64) -> Option<QueuedUserMessageDraft> {
        let message = self
            .queued_user_messages
            .iter()
            .find(|message| message.id == id)?;

        Some(QueuedUserMessageDraft {
            text: message.text.clone(),
            attachments: message.attachments.clone(),
            model_override: message.model_override.clone(),
            effort_override: message.effort_override,
        })
    }

    fn load_queue_edit_draft(&mut self, id: u64) {
        let draft = self
            .queued_edit_state
            .as_ref()
            .and_then(|state| state.drafts.get(&id).cloned())
            .or_else(|| self.queued_user_message_draft(id));

        let Some(draft) = draft else {
            return;
        };

        self.bottom_pane
            .set_composer_text_with_attachments(draft.text, draft.attachments);
    }

    fn update_queue_edit_footer_hint(&mut self) {
        let Some(state) = self.queued_edit_state.as_ref() else {
            return;
        };

        let total = self.queued_user_messages.len();
        let position = self
            .queued_user_messages
            .iter()
            .position(|message| message.id == state.selected_id)
            .map(|idx| idx + 1)
            .unwrap_or_default();

        let hints = vec![
            ("Editing".to_string(), format!("{position}/{total}")),
            ("Enter".to_string(), "save".to_string()),
            ("Esc".to_string(), "cancel".to_string()),
            ("Alt+↑/↓".to_string(), "switch".to_string()),
            ("Alt+M".to_string(), "model".to_string()),
            ("Alt+T".to_string(), "thinking".to_string()),
        ];

        self.bottom_pane
            .set_composer_footer_hint_override(Some(hints));
    }

    fn toggle_stream_pause(&mut self) {
        if self.stream_paused {
            self.stream_paused = false;

            if let Some(header) = self.paused_status_header.take() {
                self.set_status_header(header);
            }

            let buffered = std::mem::take(&mut self.paused_agent_deltas);
            if !buffered.is_empty() {
                self.handle_streaming_delta(buffered);
            }

            if !self.paused_reasoning_blocks.is_empty() {
                for block in std::mem::take(&mut self.paused_reasoning_blocks) {
                    let cell = history_cell::new_reasoning_summary_block(block);
                    self.add_boxed_history(cell);
                }
            }

            if self.paused_pending_answer_flush {
                self.paused_pending_answer_flush = false;
                self.flush_answer_stream_with_separator();
                self.handle_stream_finished();
            }

            if self.stream_controller.is_some() {
                self.app_event_tx.send(AppEvent::StartCommitAnimation);
            }

            self.request_redraw();
            self.maybe_send_next_queued_input();
            return;
        }

        if !self.bottom_pane.is_task_running() {
            return;
        }

        self.paused_status_header = Some(self.current_status_header.clone());
        self.stream_paused = true;
        self.set_status_header("Paused".to_string());
        self.app_event_tx.send(AppEvent::StopCommitAnimation);
        self.request_redraw();
    }

    pub(crate) fn attach_image(
        &mut self,
        path: PathBuf,
        width: u32,
        height: u32,
        format_label: &str,
    ) {
        tracing::info!(
            "attach_image path={path:?} width={width} height={height} format={format_label}",
        );
        self.bottom_pane
            .attach_image(path, width, height, format_label);
        self.request_redraw();
    }

    pub(crate) fn composer_text_with_pending(&self) -> String {
        self.bottom_pane.composer_text_with_pending()
    }

    pub(crate) fn apply_external_edit(&mut self, text: String) {
        self.bottom_pane.apply_external_edit(text);
        self.request_redraw();
    }

    pub(crate) fn external_editor_state(&self) -> ExternalEditorState {
        self.external_editor_state
    }

    pub(crate) fn set_external_editor_state(&mut self, state: ExternalEditorState) {
        self.external_editor_state = state;
    }

    pub(crate) fn set_footer_hint_override(&mut self, items: Option<Vec<(String, String)>>) {
        self.bottom_pane.set_footer_hint_override(items);
    }

    pub(crate) fn can_launch_external_editor(&self) -> bool {
        self.bottom_pane.can_launch_external_editor()
    }

    fn dispatch_command(&mut self, cmd: SlashCommand, command_input: Option<String>) {
        if !cmd.available_during_task() && self.bottom_pane.is_task_running() {
            let message = format!(
                "'/{}' is disabled while a task is in progress.",
                cmd.command()
            );
            self.add_to_history(history_cell::new_error_event(message));
            self.request_redraw();
            return;
        }
        match cmd {
            SlashCommand::Feedback => {
                // Step 1: pick a category (UI built in feedback_view)
                let params =
                    crate::bottom_pane::feedback_selection_params(self.app_event_tx.clone());
                self.bottom_pane.show_selection_view(params);
                self.request_redraw();
            }
            SlashCommand::New => {
                self.app_event_tx.send(AppEvent::NewSession);
            }
            SlashCommand::Resume => {
                self.app_event_tx.send(AppEvent::OpenResumePicker);
            }
            SlashCommand::Session => {
                self.open_session_manager();
            }
            SlashCommand::Rename => {
                self.open_rename_chat_view();
            }
            SlashCommand::Export => {
                match parse_export_args(command_input.as_deref(), &self.config.cwd) {
                    Ok(Some(args)) => {
                        self.start_export(args.format, args.overrides);
                    }
                    Ok(None) => {
                        self.open_export_picker();
                    }
                    Err(message) => {
                        self.add_error_message(message);
                    }
                }
            }
            SlashCommand::Init => {
                let init_target = self.config.cwd.join(DEFAULT_PROJECT_DOC_FILENAME);
                if init_target.exists() {
                    let message = format!(
                        "{DEFAULT_PROJECT_DOC_FILENAME} already exists here. Skipping /init to avoid overwriting it."
                    );
                    self.add_info_message(message, None);
                    return;
                }
                const INIT_PROMPT: &str = include_str!("../prompt_for_init_command.md");
                self.submit_user_message(INIT_PROMPT.to_string().into());
            }
            SlashCommand::Compact => {
                self.clear_token_usage();
                self.app_event_tx.send(AppEvent::CodexOp(Op::Compact));
            }
            SlashCommand::Review => {
                self.open_review_popup();
            }
            SlashCommand::Plan => {
                self.set_session_mode(SessionMode::Plan);
            }
            SlashCommand::Ask => {
                self.set_session_mode(SessionMode::Ask);
            }
            SlashCommand::Normal => {
                self.set_session_mode(SessionMode::Normal);
            }
            SlashCommand::Model => {
                self.open_model_popup();
            }
            SlashCommand::Approvals => {
                self.open_approvals_popup();
            }
            SlashCommand::Notifications => {
                self.handle_notifications_command(command_input);
            }
            SlashCommand::Experimental => {
                self.open_experimental_popup();
            }
            SlashCommand::Quit | SlashCommand::Exit => {
                self.request_exit();
            }
            SlashCommand::Logout => {
                if let Err(e) = codex_core::auth::logout(
                    &self.config.codex_home,
                    self.config.cli_auth_credentials_store_mode,
                ) {
                    tracing::error!("failed to logout: {e}");
                }
                self.request_exit();
            }
            // SlashCommand::Undo => {
            //     self.app_event_tx.send(AppEvent::CodexOp(Op::Undo));
            // }
            SlashCommand::Diff => {
                let diff_view =
                    match diff_view_override(command_input.as_deref(), self.config.diff_view) {
                        Ok(view) => view,
                        Err(message) => {
                            self.add_error_message(message);
                            return;
                        }
                    };
                self.add_diff_in_progress();
                let width = self.last_rendered_width.get().unwrap_or(80);
                let tx = self.app_event_tx.clone();
                let cwd = self.config.cwd.clone();
                tokio::spawn(async move {
                    let result = match get_git_diff(&cwd, diff_view, width).await {
                        Ok(result) => result,
                        Err(e) => GitDiffResult::Error(format!("Failed to compute diff: {e}")),
                    };
                    tx.send(AppEvent::DiffResult(result));
                });
            }
            SlashCommand::Mention => {
                self.insert_str("@");
            }
            SlashCommand::Skills => {
                self.insert_str("$");
            }
            SlashCommand::Status => {
                self.add_status_output();
            }
            SlashCommand::Ps => {
                self.add_ps_output();
            }
            SlashCommand::Queue => {
                if self.queued_user_messages.is_empty() {
                    self.add_info_message("Queue is empty.".to_string(), None);
                } else {
                    self.open_queue_popup();
                }
            }
            SlashCommand::Mcp => {
                self.add_mcp_output();
            }
            SlashCommand::Rollout => {
                if let Some(path) = self.rollout_path() {
                    self.add_info_message(
                        format!("Current rollout path: {}", path.display()),
                        None,
                    );
                } else {
                    self.add_info_message("Rollout path is not available yet.".to_string(), None);
                }
            }
            SlashCommand::TestApproval => {
                use codex_core::protocol::EventMsg;
                use std::collections::HashMap;

                use codex_core::protocol::ApplyPatchApprovalRequestEvent;
                use codex_core::protocol::FileChange;

                self.app_event_tx.send(AppEvent::CodexEvent(Event {
                    id: "1".to_string(),
                    // msg: EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
                    //     call_id: "1".to_string(),
                    //     command: vec!["git".into(), "apply".into()],
                    //     cwd: self.config.cwd.clone(),
                    //     reason: Some("test".to_string()),
                    // }),
                    msg: EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
                        call_id: "1".to_string(),
                        turn_id: "turn-1".to_string(),
                        changes: HashMap::from([
                            (
                                PathBuf::from("/tmp/test.txt"),
                                FileChange::Add {
                                    content: "test".to_string(),
                                },
                            ),
                            (
                                PathBuf::from("/tmp/test2.txt"),
                                FileChange::Update {
                                    unified_diff: "+test\n-test2".to_string(),
                                    move_path: None,
                                },
                            ),
                        ]),
                        reason: None,
                        grant_root: Some(PathBuf::from("/tmp")),
                    }),
                }));
            }
        }
    }

    fn handle_notifications_command(&mut self, command_input: Option<String>) {
        let action = match parse_notification_focus_action(command_input.as_deref()) {
            Ok(action) => action,
            Err(message) => {
                self.add_error_message(message);
                return;
            }
        };
        let configured = self.notification_focus_configured();
        let current_effective = self.notification_focus_override.unwrap_or(configured);
        let (override_after, effective_after) = match action {
            NotificationFocusAction::Enable => (Some(true), true),
            NotificationFocusAction::Disable => (Some(false), false),
            NotificationFocusAction::Toggle => (Some(!current_effective), !current_effective),
            NotificationFocusAction::Reset => (None, configured),
            NotificationFocusAction::Status => {
                (self.notification_focus_override, current_effective)
            }
        };
        if action != NotificationFocusAction::Status {
            self.notification_focus_override = override_after;
            self.submit_op(Op::UpdateNotificationFocusFilter {
                enabled: override_after,
            });
        }
        let state = if effective_after {
            "enabled"
        } else {
            "disabled"
        };
        let scope = if override_after.is_some() {
            "session override"
        } else {
            "config default"
        };
        let mut message = format!("Focus-based notification filtering is {state} ({scope}).");
        if !configured {
            message.push_str(" Configure [notification_focus] in config.toml to use it.");
        }
        self.add_info_message(message, None);
    }

    fn open_export_picker(&mut self) {
        if self.current_rollout_path.is_none() {
            self.add_info_message("Export is not available yet.".to_string(), None);
            return;
        }

        let subtitle = match self.config.tui_export_dir.as_ref() {
            Some(dir) => format!("Creates a file in {}.", dir.display()),
            None => "Creates a file next to the rollout (.jsonl).".to_string(),
        };

        let items = vec![
            SelectionItem {
                name: "Markdown (.md)".to_string(),
                description: Some("Readable transcript for sharing.".to_string()),
                actions: vec![Box::new(|tx| {
                    tx.send(AppEvent::ExportChat {
                        format: Some(ChatExportFormat::Markdown),
                        overrides: ExportOverrides::default(),
                    });
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Markdown (.md) in current dir".to_string(),
                description: Some("Creates a file in the current directory.".to_string()),
                actions: vec![Box::new(|tx| {
                    tx.send(AppEvent::ExportChat {
                        format: Some(ChatExportFormat::Markdown),
                        overrides: ExportOverrides {
                            output_dir: Some(PathBuf::from(".")),
                            ..Default::default()
                        },
                    });
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Markdown (custom path...)".to_string(),
                description: Some("Choose a destination path.".to_string()),
                actions: vec![Box::new(|tx| {
                    tx.send(AppEvent::OpenExportPathPrompt {
                        format: ChatExportFormat::Markdown,
                    });
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "JSON (.json)".to_string(),
                description: Some("Structured messages for tooling.".to_string()),
                actions: vec![Box::new(|tx| {
                    tx.send(AppEvent::ExportChat {
                        format: Some(ChatExportFormat::Json),
                        overrides: ExportOverrides::default(),
                    });
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "JSON (.json) in current dir".to_string(),
                description: Some("Creates a file in the current directory.".to_string()),
                actions: vec![Box::new(|tx| {
                    tx.send(AppEvent::ExportChat {
                        format: Some(ChatExportFormat::Json),
                        overrides: ExportOverrides {
                            output_dir: Some(PathBuf::from(".")),
                            ..Default::default()
                        },
                    });
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "JSON (custom path...)".to_string(),
                description: Some("Choose a destination path.".to_string()),
                actions: vec![Box::new(|tx| {
                    tx.send(AppEvent::OpenExportPathPrompt {
                        format: ChatExportFormat::Json,
                    });
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Export Chat".to_string()),
            subtitle: Some(subtitle),
            footer_hint: Some(standard_popup_hint_line()),
            completion_behavior: ViewCompletionBehavior::Pop,
            items,
            ..Default::default()
        });
    }

    pub(crate) fn open_export_path_prompt(&mut self, format: ChatExportFormat) {
        let Some(rollout_path) = self.current_rollout_path.clone() else {
            self.add_info_message("Export is not available yet.".to_string(), None);
            return;
        };

        let defaults = ExportDefaults::from_config(&self.config);
        let default_path = match resolve_export_destination(
            &rollout_path,
            &defaults,
            Some(format),
            &ExportOverrides::default(),
        ) {
            Ok(destination) => destination.path,
            Err(message) => {
                self.add_error_message(message);
                return;
            }
        };

        let placeholder = format!("Path (default: {})", default_path.display());
        let context_label = Some(format!("Format: {}", format.label()));
        let tx = self.app_event_tx.clone();
        let cwd = self.config.cwd.clone();
        let view = CustomPromptView::new(
            "Export path".to_string(),
            placeholder,
            context_label,
            Box::new(move |input: String| {
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    return;
                }

                let overrides = export_overrides_from_path_input(trimmed, &cwd);
                tx.send(AppEvent::ExportChat {
                    format: Some(format),
                    overrides,
                });
            }),
        );
        self.bottom_pane.show_view(Box::new(view));
        self.request_redraw();
    }

    pub(crate) fn start_export(
        &mut self,
        format: Option<ChatExportFormat>,
        overrides: ExportOverrides,
    ) {
        let Some(rollout_path) = self.current_rollout_path.clone() else {
            self.add_info_message("Export is not available yet.".to_string(), None);
            return;
        };

        let defaults = ExportDefaults::from_config(&self.config);
        let destination =
            match resolve_export_destination(&rollout_path, &defaults, format, &overrides) {
                Ok(destination) => destination,
                Err(message) => {
                    self.add_error_message(message);
                    return;
                }
            };

        let out_path = destination.path;
        let format = destination.format;

        let tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = async {
                if let Some(parent) = out_path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }

                match format {
                    ChatExportFormat::Markdown => {
                        export_markdown::export_rollout_as_markdown(&rollout_path, &out_path).await
                    }
                    ChatExportFormat::Json => {
                        export_markdown::export_rollout_as_json(&rollout_path, &out_path).await
                    }
                }
            }
            .await;

            match result {
                Ok(messages) => tx.send(AppEvent::ExportResult {
                    path: out_path,
                    messages,
                    error: None,
                    format,
                }),
                Err(e) => tx.send(AppEvent::ExportResult {
                    path: out_path,
                    messages: 0,
                    error: Some(e.to_string()),
                    format,
                }),
            };
        });
    }

    pub(crate) fn handle_paste(&mut self, text: String) {
        if text.is_empty() {
            // Some terminals (like VS Code) route Cmd+V through terminal paste, which can
            // yield an empty payload for images. Fall back to reading the clipboard.
            self.paste_from_clipboard();
            return;
        }
        self.bottom_pane.handle_paste(text);
    }

    // Returns true if caller should skip rendering this frame (a future frame is scheduled).
    pub(crate) fn handle_paste_burst_tick(&mut self, frame_requester: FrameRequester) -> bool {
        if self.bottom_pane.flush_paste_burst_if_due() {
            // A paste just flushed; request an immediate redraw and skip this frame.
            self.request_redraw();
            true
        } else if self.bottom_pane.is_in_paste_burst() {
            // While capturing a burst, schedule a follow-up tick and skip this frame
            // to avoid redundant renders between ticks.
            frame_requester.schedule_frame_in(
                crate::bottom_pane::ChatComposer::recommended_paste_flush_delay(),
            );
            true
        } else {
            false
        }
    }

    fn flush_active_cell(&mut self) {
        if let Some(active) = self.active_cell.take() {
            self.needs_final_message_separator = true;
            self.app_event_tx.send(AppEvent::InsertHistoryCell(active));
        }
    }

    pub(crate) fn add_to_history(&mut self, cell: impl HistoryCell + 'static) {
        self.add_boxed_history(Box::new(cell));
    }

    fn add_boxed_history(&mut self, cell: Box<dyn HistoryCell>) {
        if !cell.display_lines(u16::MAX).is_empty() {
            // Only break exec grouping if the cell renders visible lines.
            self.flush_active_cell();
            self.needs_final_message_separator = true;
        }
        self.app_event_tx.send(AppEvent::InsertHistoryCell(cell));
    }

    fn queue_user_message(&mut self, text: String, attachments: Vec<ComposerAttachment>) {
        if self.bottom_pane.is_task_running() {
            let id = self.next_queued_user_message_id;
            self.next_queued_user_message_id = self.next_queued_user_message_id.saturating_add(1);

            self.queued_user_messages.push_back(QueuedUserMessage {
                id,
                text,
                attachments,
                model_override: None,
                effort_override: None,
            });
            self.refresh_queued_user_messages();
        } else {
            self.queued_auto_send_pending = false;
            let image_paths = attachments
                .iter()
                .map(|attachment| attachment.path.clone())
                .collect();
            self.submit_user_message(UserMessage {
                text,
                image_paths,
                attachments,
            });
        }
    }

    fn submit_queued_user_message(&mut self, queued: QueuedUserMessage) {
        let apply_model = queued.model_override.clone();
        let apply_effort = queued.effort_override;

        let session = self.session_turn_context();
        let effective_model = apply_model.as_deref().unwrap_or(session.model.as_str());
        let effective_effort = match apply_effort {
            Some(effort) => effort,
            None => session.reasoning_effort,
        };
        self.pending_active_turn_context = Some(PendingTurnContext {
            model: effective_model.to_string(),
            reasoning_effort: effective_effort,
            model_family: session.model_family.clone(),
            mode: session.mode,
        });

        let restore_model = apply_model.as_ref().map(|_| session.model.clone());
        let restore_effort = apply_effort.as_ref().map(|_| session.reasoning_effort);

        if apply_model.is_some() || apply_effort.is_some() {
            let session_model = session.model.as_str();
            let effective_model = apply_model.as_deref().unwrap_or(session_model);
            let effective_effort = apply_effort.unwrap_or(session.reasoning_effort);

            if effective_model != session_model || effective_effort != session.reasoning_effort {
                self.add_info_message(
                    self.queued_override_info_message(effective_model, effective_effort),
                    None,
                );
            }

            self.submit_op(Op::OverrideTurnContext {
                cwd: None,
                approval_policy: None,
                sandbox_policy: None,
                model: apply_model,
                effort: apply_effort,
                summary: None,
                mode: None,
            });
        }

        let image_paths = queued
            .attachments
            .iter()
            .map(|attachment| attachment.path.clone())
            .collect();
        self.submit_user_message(UserMessage {
            text: queued.text,
            image_paths,
            attachments: queued.attachments,
        });

        if restore_model.is_some() || restore_effort.is_some() {
            self.submit_op(Op::OverrideTurnContext {
                cwd: None,
                approval_policy: None,
                sandbox_policy: None,
                model: restore_model,
                effort: restore_effort,
                summary: None,
                mode: None,
            });
        }
    }

    pub(crate) fn submit_backtrack_message(
        &mut self,
        text: String,
        attachments: Vec<ComposerAttachment>,
        overrides: Option<ResendOverrides>,
    ) {
        if text.is_empty() && attachments.is_empty() {
            return;
        }

        let image_paths = attachments
            .iter()
            .map(|attachment| attachment.path.clone())
            .collect();

        let Some(overrides) = overrides else {
            self.submit_user_message(UserMessage {
                text,
                image_paths,
                attachments,
            });
            return;
        };

        let session = self.session_turn_context();
        let restore_model = session.model.clone();
        let restore_effort = session.reasoning_effort;
        let apply_effort = Some(overrides.effort);
        let effective_effort = match apply_effort {
            Some(effort) => effort,
            None => session.reasoning_effort,
        };
        self.pending_active_turn_context = Some(PendingTurnContext {
            model: overrides.model.clone(),
            reasoning_effort: effective_effort,
            model_family: session.model_family.clone(),
            mode: session.mode,
        });

        self.submit_op(Op::OverrideTurnContext {
            cwd: None,
            approval_policy: None,
            sandbox_policy: None,
            model: Some(overrides.model),
            effort: apply_effort,
            summary: None,
            mode: None,
        });
        self.submit_user_message(UserMessage {
            text,
            image_paths,
            attachments,
        });
        self.submit_op(Op::OverrideTurnContext {
            cwd: None,
            approval_policy: None,
            sandbox_policy: None,
            model: Some(restore_model),
            effort: Some(restore_effort),
            summary: None,
            mode: None,
        });
    }

    fn submit_user_message(&mut self, user_message: UserMessage) {
        let UserMessage {
            text,
            image_paths,
            attachments,
        } = user_message;
        if text.is_empty() && image_paths.is_empty() {
            return;
        }

        let mut items: Vec<UserInput> = Vec::new();

        // Special-case: "!cmd" executes a local shell command instead of sending to the model.
        if let Some(stripped) = text.strip_prefix('!') {
            let cmd = stripped.trim();
            if cmd.is_empty() {
                self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(
                    history_cell::new_info_event(
                        USER_SHELL_COMMAND_HELP_TITLE.to_string(),
                        Some(USER_SHELL_COMMAND_HELP_HINT.to_string()),
                    ),
                )));
                return;
            }
            self.submit_op(Op::RunUserShellCommand {
                command: cmd.to_string(),
            });
            return;
        }

        if !text.is_empty() {
            items.push(UserInput::Text { text: text.clone() });
        }

        for path in &image_paths {
            items.push(UserInput::LocalImage { path: path.clone() });
        }

        if let Some(skills) = self.bottom_pane.skills() {
            let skill_mentions = find_skill_mentions(&text, skills);
            for skill in skill_mentions {
                items.push(UserInput::Skill {
                    name: skill.name.clone(),
                    path: skill.path.clone(),
                });
            }
        }

        if self.pending_active_turn_context.is_none() {
            self.pending_active_turn_context = Some(self.session_turn_context());
        }

        self.codex_op_tx
            .send(Op::UserInput { items })
            .unwrap_or_else(|e| {
                tracing::error!("failed to send message: {e}");
            });

        // Persist the text to cross-session message history.
        if !text.is_empty() {
            self.codex_op_tx
                .send(Op::AddToHistory { text: text.clone() })
                .unwrap_or_else(|e| {
                    tracing::error!("failed to send AddHistory op: {e}");
                });
        }

        // Only show the text portion in conversation history.
        if !text.is_empty() {
            self.add_to_history(history_cell::new_user_prompt(text, attachments));
        }
        self.needs_final_message_separator = false;
    }

    /// Replay a subset of initial events into the UI to seed the transcript when
    /// resuming an existing session. This approximates the live event flow and
    /// is intentionally conservative: only safe-to-replay items are rendered to
    /// avoid triggering side effects. Event ids are passed as `None` to
    /// distinguish replayed events from live ones.
    fn replay_initial_messages(&mut self, events: Vec<EventMsg>) {
        for msg in events {
            if matches!(msg, EventMsg::SessionConfigured(_)) {
                continue;
            }
            // `id: None` indicates a synthetic/fake id coming from replay.
            self.dispatch_event_msg(None, msg, true);
        }
    }

    pub(crate) fn handle_codex_event(&mut self, event: Event) {
        let Event { id, msg } = event;
        self.dispatch_event_msg(Some(id), msg, false);
    }

    /// Dispatch a protocol `EventMsg` to the appropriate handler.
    ///
    /// `id` is `Some` for live events and `None` for replayed events from
    /// `replay_initial_messages()`. Callers should treat `None` as a "fake" id
    /// that must not be used to correlate follow-up actions.
    fn dispatch_event_msg(&mut self, id: Option<String>, msg: EventMsg, from_replay: bool) {
        let is_stream_error = matches!(&msg, EventMsg::StreamError(_));
        if !is_stream_error {
            self.restore_retry_status_header_if_present();
        }

        match msg {
            EventMsg::AgentMessageDelta(_)
            | EventMsg::AgentReasoningDelta(_)
            | EventMsg::TerminalInteraction(_)
            | EventMsg::ExecCommandOutputDelta(_) => {}
            _ => {
                tracing::trace!("handle_codex_event: {:?}", msg);
            }
        }

        match msg {
            EventMsg::SessionConfigured(e) => self.on_session_configured(e),
            EventMsg::SessionTitleUpdated(e) => self.on_session_title_updated(e),
            EventMsg::TurnContextUpdated(e) => self.on_turn_context_updated(e),
            EventMsg::AgentMessage(AgentMessageEvent { message }) => self.on_agent_message(message),
            EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }) => {
                self.on_agent_message_delta(delta)
            }
            EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta })
            | EventMsg::AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent {
                delta,
            }) => self.on_agent_reasoning_delta(delta),
            EventMsg::AgentReasoning(AgentReasoningEvent { .. }) => self.on_agent_reasoning_final(),
            EventMsg::AgentReasoningRawContent(AgentReasoningRawContentEvent { text }) => {
                self.on_agent_reasoning_delta(text);
                self.on_agent_reasoning_final();
            }
            EventMsg::AgentReasoningSectionBreak(_) => self.on_reasoning_section_break(),
            EventMsg::TaskStarted(_) => self.on_task_started(),
            EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message }) => {
                self.on_task_complete(last_agent_message)
            }
            EventMsg::TokenCount(ev) => {
                self.set_token_info(ev.info);
                self.on_rate_limit_snapshot(ev.rate_limits);
            }
            EventMsg::Warning(WarningEvent { message }) => self.on_warning(message),
            EventMsg::Error(ErrorEvent { message, .. }) => self.on_error(message),
            EventMsg::McpStartupUpdate(ev) => self.on_mcp_startup_update(ev),
            EventMsg::McpStartupComplete(ev) => self.on_mcp_startup_complete(ev),
            EventMsg::TurnAborted(ev) => match ev.reason {
                TurnAbortReason::Interrupted => {
                    self.on_interrupted_turn(ev.reason);
                }
                TurnAbortReason::Replaced => {
                    self.on_error("Turn aborted: replaced by a new task".to_owned())
                }
                TurnAbortReason::ReviewEnded => {
                    self.on_interrupted_turn(ev.reason);
                }
            },
            EventMsg::PlanUpdate(update) => self.on_plan_update(update),
            EventMsg::ExecApprovalRequest(ev) => {
                // For replayed events, synthesize an empty id (these should not occur).
                self.on_exec_approval_request(id.unwrap_or_default(), ev)
            }
            EventMsg::ApplyPatchApprovalRequest(ev) => {
                self.on_apply_patch_approval_request(id.unwrap_or_default(), ev)
            }
            EventMsg::ElicitationRequest(ev) => {
                self.on_elicitation_request(ev);
            }
            EventMsg::ExecCommandBegin(ev) => self.on_exec_command_begin(ev),
            EventMsg::TerminalInteraction(delta) => self.on_terminal_interaction(delta),
            EventMsg::ExecCommandOutputDelta(delta) => self.on_exec_command_output_delta(delta),
            EventMsg::PatchApplyBegin(ev) => self.on_patch_apply_begin(ev),
            EventMsg::PatchApplyEnd(ev) => self.on_patch_apply_end(ev),
            EventMsg::ExecCommandEnd(ev) => self.on_exec_command_end(ev),
            EventMsg::ViewImageToolCall(ev) => self.on_view_image_tool_call(ev),
            EventMsg::McpToolCallBegin(ev) => self.on_mcp_tool_call_begin(ev),
            EventMsg::McpToolCallEnd(ev) => self.on_mcp_tool_call_end(ev),
            EventMsg::WebSearchBegin(ev) => self.on_web_search_begin(ev),
            EventMsg::WebSearchEnd(ev) => self.on_web_search_end(ev),
            EventMsg::GetHistoryEntryResponse(ev) => self.on_get_history_entry_response(ev),
            EventMsg::McpListToolsResponse(ev) => self.on_list_mcp_tools(ev),
            EventMsg::ListCustomPromptsResponse(ev) => self.on_list_custom_prompts(ev),
            EventMsg::ListSkillsResponse(ev) => self.on_list_skills(ev),
            EventMsg::SkillsUpdateAvailable => {
                self.submit_op(Op::ListSkills {
                    cwds: Vec::new(),
                    force_reload: true,
                });
            }
            EventMsg::ShutdownComplete => self.on_shutdown_complete(),
            EventMsg::TurnDiff(TurnDiffEvent { unified_diff }) => self.on_turn_diff(unified_diff),
            EventMsg::DeprecationNotice(ev) => self.on_deprecation_notice(ev),
            EventMsg::BackgroundEvent(BackgroundEventEvent { message }) => {
                self.on_background_event(message)
            }
            EventMsg::UndoStarted(ev) => self.on_undo_started(ev),
            EventMsg::UndoCompleted(ev) => self.on_undo_completed(ev),
            EventMsg::StreamError(StreamErrorEvent {
                message,
                additional_details,
                ..
            }) => self.on_stream_error(message, additional_details),
            EventMsg::UserMessage(ev) => {
                if from_replay {
                    self.on_user_message_event(ev);
                }
            }
            EventMsg::EnteredReviewMode(review_request) => {
                self.on_entered_review_mode(review_request)
            }
            EventMsg::ExitedReviewMode(review) => self.on_exited_review_mode(review),
            EventMsg::ContextCompacted(_) => self.on_agent_message("Context compacted".to_owned()),
            EventMsg::RawResponseItem(_)
            | EventMsg::ItemStarted(_)
            | EventMsg::ItemCompleted(_)
            | EventMsg::AgentMessageContentDelta(_)
            | EventMsg::ReasoningContentDelta(_)
            | EventMsg::ReasoningRawContentDelta(_) => {}
        }
    }

    fn on_entered_review_mode(&mut self, review: ReviewRequest) {
        // Enter review mode and emit a concise banner
        if self.pre_review_token_info.is_none() {
            self.pre_review_token_info = Some(self.token_info.clone());
        }
        self.is_review_mode = true;
        let hint = review
            .user_facing_hint
            .unwrap_or_else(|| codex_core::review_prompts::user_facing_hint(&review.target));
        let banner = format!(">> Code review started: {hint} <<");
        self.add_to_history(history_cell::new_review_status_line(banner));
        self.request_redraw();
    }

    fn on_exited_review_mode(&mut self, review: ExitedReviewModeEvent) {
        // Leave review mode; if output is present, flush pending stream + show results.
        if let Some(output) = review.review_output {
            self.flush_answer_stream_with_separator();
            self.flush_interrupt_queue();
            self.flush_active_cell();

            if output.findings.is_empty() {
                let explanation = output.overall_explanation.trim().to_string();
                if explanation.is_empty() {
                    tracing::error!("Reviewer failed to output a response.");
                    self.add_to_history(history_cell::new_error_event(
                        "Reviewer failed to output a response.".to_owned(),
                    ));
                } else {
                    // Show explanation when there are no structured findings.
                    let mut rendered: Vec<ratatui::text::Line<'static>> = vec!["".into()];
                    append_markdown(&explanation, None, &mut rendered);
                    let body_cell = AgentMessageCell::new(rendered, false);
                    self.app_event_tx
                        .send(AppEvent::InsertHistoryCell(Box::new(body_cell)));
                }
            }
            // Final message is rendered as part of the AgentMessage.
        }

        self.is_review_mode = false;
        self.restore_pre_review_token_info();
        // Append a finishing banner at the end of this turn.
        self.add_to_history(history_cell::new_review_status_line(
            "<< Code review finished >>".to_string(),
        ));
        self.request_redraw();
    }

    fn on_user_message_event(&mut self, event: UserMessageEvent) {
        let message = event.message.trim();
        if !message.is_empty() {
            self.add_to_history(history_cell::new_user_prompt(
                message.to_string(),
                Vec::new(),
            ));
        }
    }

    fn request_exit(&self) {
        self.app_event_tx.send(AppEvent::ExitRequest);
    }

    fn request_redraw(&mut self) {
        self.frame_requester.schedule_frame();
    }

    fn notify(&mut self, notification: Notification) {
        if !notification.allowed_for(&self.config.tui_notifications) {
            return;
        }
        self.pending_notification = Some(notification);
        self.request_redraw();
    }

    pub(crate) fn maybe_post_pending_notification(&mut self, tui: &mut crate::tui::Tui) {
        if let Some(notif) = self.pending_notification.take() {
            tui.notify(notif.display());
        }
    }

    /// Mark the active cell as failed (✗) and flush it into history.
    fn finalize_active_cell_as_failed(&mut self) {
        if let Some(mut cell) = self.active_cell.take() {
            // Insert finalized cell into history and keep grouping consistent.
            if let Some(exec) = cell.as_any_mut().downcast_mut::<ExecCell>() {
                exec.mark_failed();
            } else if let Some(tool) = cell.as_any_mut().downcast_mut::<McpToolCallCell>() {
                tool.mark_failed();
            }
            self.add_boxed_history(cell);
        }
    }

    // If idle and there are queued inputs, submit exactly one to start the next turn.
    fn maybe_send_next_queued_input(&mut self) {
        if !self.queued_auto_send_pending {
            return;
        }
        if self.stream_paused {
            return;
        }
        if self.bottom_pane.is_task_running() {
            return;
        }
        if self.queued_edit_state.is_some() {
            return;
        }
        if self.bottom_pane.has_active_view() || self.bottom_pane.composer_popup_active() {
            return;
        }

        if let Some(queued) = self.queued_user_messages.pop_front() {
            self.submit_queued_user_message(queued);
        }
        self.queued_auto_send_pending = false;
        // Update the list to reflect the remaining queued messages (if any).
        self.refresh_queued_user_messages();
    }

    fn send_next_queued_user_message(&mut self) {
        if self.stream_paused {
            return;
        }
        if self.bottom_pane.is_task_running() {
            return;
        }
        if self.queued_edit_state.is_some() {
            return;
        }
        if self.bottom_pane.has_active_view() || self.bottom_pane.composer_popup_active() {
            return;
        }

        if let Some(queued) = self.queued_user_messages.pop_front() {
            self.submit_queued_user_message(queued);
        }

        self.refresh_queued_user_messages();
    }

    /// Rebuild and update the queued user messages from the current queue.
    fn refresh_queued_user_messages(&mut self) {
        let session = self.session_turn_context();
        let editing_id = self
            .queued_edit_state
            .as_ref()
            .map(|state| state.selected_id);
        let messages: Vec<String> = self
            .queued_user_messages
            .iter()
            .map(|message| {
                let effective_model = message
                    .model_override
                    .as_deref()
                    .unwrap_or(session.model.as_str());
                let effective_effort = match message.effort_override {
                    Some(effort) => effort,
                    None => session.reasoning_effort,
                };

                let tag = if message.model_override.is_some() || message.effort_override.is_some() {
                    let mut tag = format!("[{effective_model}");
                    if let Some(label) = Self::thinking_label_for(effective_model, effective_effort)
                        .or_else(|| {
                            (!effective_model.starts_with("codex-auto-")
                                && effective_effort.is_none())
                            .then_some("default")
                        })
                    {
                        tag.push_str(&format!(" · reasoning {label}"));
                    }
                    tag.push_str("] ");
                    tag
                } else {
                    String::new()
                };

                if Some(message.id) == editing_id {
                    format!("✎ {tag}{}", message.text)
                } else {
                    format!("{tag}{}", message.text)
                }
            })
            .collect();
        self.bottom_pane.set_queued_user_messages(messages);
    }

    pub(crate) fn queue_snapshot(&self) -> Option<QueueSnapshot> {
        if self.queued_user_messages.is_empty() {
            return None;
        }

        Some(QueueSnapshot {
            messages: self.queued_user_messages.clone(),
            next_id: self.next_queued_user_message_id,
        })
    }

    pub(crate) fn apply_queue_snapshot(&mut self, snapshot: QueueSnapshot) {
        self.queued_user_messages = snapshot.messages;
        self.next_queued_user_message_id = snapshot.next_id;
        self.queued_edit_state = None;
        self.queued_auto_send_pending = false;
        self.refresh_queued_user_messages();
    }

    pub(crate) fn add_diff_in_progress(&mut self) {
        self.request_redraw();
    }

    pub(crate) fn on_diff_complete(&mut self) {
        self.request_redraw();
    }

    pub(crate) fn add_status_output(&mut self) {
        let default_usage = TokenUsage::default();
        let (total_usage, context_usage) = if let Some(ti) = &self.token_info {
            (&ti.total_token_usage, Some(&ti.last_token_usage))
        } else {
            (&default_usage, Some(&default_usage))
        };

        if self.bottom_pane.is_task_running()
            && let Some(active) = self.active_turn_context.as_ref()
        {
            let mut status_config = self.config.clone();
            status_config.model_reasoning_effort = active.reasoning_effort;

            self.add_to_history(crate::status::new_status_output(
                &status_config,
                self.auth_manager.as_ref(),
                &active.model_family,
                total_usage,
                context_usage,
                &self.conversation_id,
                self.rate_limit_snapshot.as_ref(),
                self.plan_type,
                Local::now(),
                &active.model,
            ));

            return;
        }

        let model_slug = self
            .config
            .model
            .as_deref()
            .unwrap_or_else(|| self.model_family.get_model_slug());
        self.add_to_history(crate::status::new_status_output(
            &self.config,
            self.auth_manager.as_ref(),
            &self.model_family,
            total_usage,
            context_usage,
            &self.conversation_id,
            self.rate_limit_snapshot.as_ref(),
            self.plan_type,
            Local::now(),
            model_slug,
        ));
    }

    pub(crate) fn add_ps_output(&mut self) {
        let sessions = self
            .unified_exec_sessions
            .iter()
            .map(|session| session.command_display.clone())
            .collect();
        self.add_to_history(history_cell::new_unified_exec_sessions_output(sessions));
    }
    fn stop_rate_limit_poller(&mut self) {
        if let Some(handle) = self.rate_limit_poller.take() {
            handle.abort();
        }
    }

    fn prefetch_rate_limits(&mut self) {
        self.stop_rate_limit_poller();

        let Some(auth) = self.auth_manager.auth() else {
            return;
        };
        if auth.mode != AuthMode::ChatGPT {
            return;
        }

        let base_url = self.config.chatgpt_base_url.clone();
        let app_event_tx = self.app_event_tx.clone();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));

            loop {
                if let Some(snapshot) = fetch_rate_limits(base_url.clone(), auth.clone()).await {
                    app_event_tx.send(AppEvent::RateLimitSnapshotFetched(snapshot));
                }
                interval.tick().await;
            }
        });

        self.rate_limit_poller = Some(handle);
    }

    fn lower_cost_preset(&self) -> Option<ModelPreset> {
        let models = self.models_manager.try_list_models(&self.config).ok()?;
        models
            .iter()
            .find(|preset| preset.model == NUDGE_MODEL_SLUG)
            .cloned()
    }

    fn rate_limit_switch_prompt_hidden(&self) -> bool {
        self.config
            .notices
            .hide_rate_limit_model_nudge
            .unwrap_or(false)
    }

    fn maybe_show_pending_rate_limit_prompt(&mut self) {
        if self.rate_limit_switch_prompt_hidden() {
            self.rate_limit_switch_prompt = RateLimitSwitchPromptState::Idle;
            return;
        }
        if !matches!(
            self.rate_limit_switch_prompt,
            RateLimitSwitchPromptState::Pending
        ) {
            return;
        }
        if let Some(preset) = self.lower_cost_preset() {
            self.open_rate_limit_switch_prompt(preset);
            self.rate_limit_switch_prompt = RateLimitSwitchPromptState::Shown;
        } else {
            self.rate_limit_switch_prompt = RateLimitSwitchPromptState::Idle;
        }
    }

    fn open_rate_limit_switch_prompt(&mut self, preset: ModelPreset) {
        let switch_model = preset.model.to_string();
        let display_name = preset.display_name.to_string();
        let default_effort: ReasoningEffortConfig = preset.default_reasoning_effort;

        let switch_actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
            tx.send(AppEvent::CodexOp(Op::OverrideTurnContext {
                cwd: None,
                approval_policy: None,
                sandbox_policy: None,
                model: Some(switch_model.clone()),
                effort: Some(Some(default_effort)),
                summary: None,
                mode: None,
            }));
        })];

        let keep_actions: Vec<SelectionAction> = Vec::new();
        let never_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::UpdateRateLimitSwitchPromptHidden(true));
            tx.send(AppEvent::PersistRateLimitSwitchPromptHidden);
        })];
        let description = if preset.description.is_empty() {
            Some("Uses fewer credits for upcoming turns.".to_string())
        } else {
            Some(preset.description)
        };

        let items = vec![
            SelectionItem {
                name: format!("Switch to {display_name}"),
                description,
                selected_description: None,
                is_current: false,
                actions: switch_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Keep current model".to_string(),
                description: None,
                selected_description: None,
                is_current: false,
                actions: keep_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Keep current model (never show again)".to_string(),
                description: Some(
                    "Hide future rate limit reminders about switching models.".to_string(),
                ),
                selected_description: None,
                is_current: false,
                actions: never_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Approaching rate limits".to_string()),
            subtitle: Some(format!("Switch to {display_name} for lower credit usage?")),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            ..Default::default()
        });
    }

    /// Open a popup to choose a quick auto model. Selecting "All models"
    /// opens the full picker with every available preset.
    pub(crate) fn open_model_popup(&mut self) {
        let current_model = self.model_family.get_model_slug().to_string();
        let presets: Vec<ModelPreset> =
            // todo(aibrahim): make this async function
            match self.models_manager.try_list_models(&self.config) {
                Ok(models) => models,
                Err(_) => {
                    self.add_info_message(
                        "Models are being updated; please try /model again in a moment."
                            .to_string(),
                        None,
                    );
                    return;
                }
            };

        let current_label = presets
            .iter()
            .find(|preset| preset.model == current_model)
            .map(|preset| preset.display_name.to_string())
            .unwrap_or_else(|| current_model.clone());

        let (mut auto_presets, other_presets): (Vec<ModelPreset>, Vec<ModelPreset>) = presets
            .into_iter()
            .partition(|preset| Self::is_auto_model(&preset.model));

        if auto_presets.is_empty() {
            self.open_all_models_popup(other_presets);
            return;
        }

        auto_presets.sort_by_key(|preset| Self::auto_model_order(&preset.model));

        let mut items: Vec<SelectionItem> = auto_presets
            .into_iter()
            .map(|preset| {
                let description =
                    (!preset.description.is_empty()).then_some(preset.description.clone());
                let model = preset.model.clone();
                let actions = Self::model_selection_actions(
                    model.clone(),
                    Some(preset.default_reasoning_effort),
                );
                SelectionItem {
                    name: preset.display_name.clone(),
                    description,
                    is_current: model == current_model,
                    is_default: preset.is_default,
                    actions,
                    dismiss_on_select: true,
                    ..Default::default()
                }
            })
            .collect();

        if !other_presets.is_empty() {
            let all_models = other_presets;
            let actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
                tx.send(AppEvent::OpenAllModelsPopup {
                    models: all_models.clone(),
                });
            })];

            let is_current = !items.iter().any(|item| item.is_current);
            let description = Some(format!(
                "Choose a specific model and reasoning level (current: {current_label})"
            ));

            items.push(SelectionItem {
                name: "All models".to_string(),
                description,
                is_current,
                actions,
                dismiss_on_select: true,
                ..Default::default()
            });
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select Model".to_string()),
            subtitle: Some("Pick a quick auto mode or browse all models.".to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            ..Default::default()
        });
    }

    #[allow(dead_code)]
    fn open_thinking_popup(&mut self) {
        let model_slug = self.model_family.get_model_slug().to_string();
        let current_effort = self.config.model_reasoning_effort;
        let presets = match self.models_manager.try_list_models(&self.config) {
            Ok(presets) => presets,
            Err(_) => {
                self.add_info_message(
                    "Models are being updated; please try again in a moment.".to_string(),
                    None,
                );
                return;
            }
        };

        let Some(preset) = presets
            .into_iter()
            .find(|preset| preset.model == model_slug)
        else {
            self.add_info_message(
                format!("Model '{model_slug}' is not available right now."),
                None,
            );
            return;
        };

        let preset_display_name = preset.display_name.clone();
        let default_effort = preset.default_reasoning_effort;
        let mut items: Vec<SelectionItem> = Vec::new();

        items.push(SelectionItem {
            name: format!("Default ({})", Self::reasoning_effort_label(default_effort)),
            description: Some("Use the model's default reasoning level.".to_string()),
            is_current: current_effort.is_none(),
            actions: Self::model_selection_actions(model_slug.clone(), None),
            dismiss_on_select: true,
            ..Default::default()
        });

        for option in preset.supported_reasoning_efforts {
            let effort = option.effort;
            let mut label = Self::reasoning_effort_label(effort).to_string();
            if effort == default_effort {
                label.push_str(" (default)");
            }

            let description = (!option.description.is_empty()).then_some(option.description);
            let actions = Self::model_selection_actions(model_slug.clone(), Some(effort));
            items.push(SelectionItem {
                name: label,
                description,
                is_current: current_effort == Some(effort),
                actions,
                dismiss_on_select: true,
                ..Default::default()
            });
        }

        let initial_selected_idx = items.iter().position(|item| item.is_current).or(Some(0));

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select Thinking".to_string()),
            subtitle: Some(format!("Model: {preset_display_name}")),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            initial_selected_idx,
            ..Default::default()
        });
    }

    fn is_auto_model(model: &str) -> bool {
        model.starts_with("codex-auto-")
    }

    fn auto_model_order(model: &str) -> usize {
        match model {
            "codex-auto-fast" => 0,
            "codex-auto-balanced" => 1,
            "codex-auto-thorough" => 2,
            _ => 3,
        }
    }

    pub(crate) fn open_all_models_popup(&mut self, presets: Vec<ModelPreset>) {
        if presets.is_empty() {
            self.add_info_message(
                "No additional models are available right now.".to_string(),
                None,
            );
            return;
        }

        let current_model = self.model_family.get_model_slug().to_string();
        let mut items: Vec<SelectionItem> = Vec::new();
        for preset in presets.into_iter() {
            let description =
                (!preset.description.is_empty()).then_some(preset.description.to_string());
            let is_current = preset.model == current_model;
            let single_supported_effort = preset.supported_reasoning_efforts.len() == 1;
            let preset_for_action = preset.clone();
            let actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
                let preset_for_event = preset_for_action.clone();
                tx.send(AppEvent::OpenReasoningPopup {
                    model: preset_for_event,
                });
            })];
            items.push(SelectionItem {
                name: preset.display_name.clone(),
                description,
                is_current,
                is_default: preset.is_default,
                actions,
                dismiss_on_select: single_supported_effort,
                ..Default::default()
            });
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select Model and Effort".to_string()),
            subtitle: Some(
                "Access legacy models by running codex -m <model_name> or in your config.toml"
                    .to_string(),
            ),
            footer_hint: Some("Press enter to select reasoning effort, or esc to dismiss.".into()),
            items,
            ..Default::default()
        });
    }

    fn model_selection_actions(
        model_for_action: String,
        effort_for_action: Option<ReasoningEffortConfig>,
    ) -> Vec<SelectionAction> {
        vec![Box::new(move |tx| {
            let effort_label = effort_for_action
                .map(|effort| effort.to_string())
                .unwrap_or_else(|| "default".to_string());
            tx.send(AppEvent::CodexOp(Op::OverrideTurnContext {
                cwd: None,
                approval_policy: None,
                sandbox_policy: None,
                model: Some(model_for_action.clone()),
                effort: Some(effort_for_action),
                summary: None,
                mode: None,
            }));
            tx.send(AppEvent::PersistModelSelection {
                model: model_for_action.clone(),
                effort: effort_for_action,
            });
            tracing::info!(
                "Selected model: {}, Selected effort: {}",
                model_for_action,
                effort_label
            );
        })]
    }

    /// Open a popup to choose the reasoning effort (stage 2) for the given model.
    pub(crate) fn open_reasoning_popup(&mut self, preset: ModelPreset) {
        let default_effort: ReasoningEffortConfig = preset.default_reasoning_effort;
        let supported = preset.supported_reasoning_efforts;

        let warn_effort = if supported
            .iter()
            .any(|option| option.effort == ReasoningEffortConfig::XHigh)
        {
            Some(ReasoningEffortConfig::XHigh)
        } else if supported
            .iter()
            .any(|option| option.effort == ReasoningEffortConfig::High)
        {
            Some(ReasoningEffortConfig::High)
        } else {
            None
        };
        let warning_text = warn_effort.map(|effort| {
            let effort_label = Self::reasoning_effort_label(effort);
            format!("⚠ {effort_label} reasoning effort can quickly consume Plus plan rate limits.")
        });
        let warn_for_model = preset.model.starts_with("gpt-5.1-codex")
            || preset.model.starts_with("gpt-5.1-codex-max")
            || preset.model.starts_with("gpt-5.2");

        struct EffortChoice {
            stored: Option<ReasoningEffortConfig>,
            display: ReasoningEffortConfig,
        }
        let mut choices: Vec<EffortChoice> = Vec::new();
        for effort in ReasoningEffortConfig::iter() {
            if supported.iter().any(|option| option.effort == effort) {
                choices.push(EffortChoice {
                    stored: Some(effort),
                    display: effort,
                });
            }
        }
        if choices.is_empty() {
            choices.push(EffortChoice {
                stored: Some(default_effort),
                display: default_effort,
            });
        }

        if choices.len() == 1 {
            if let Some(effort) = choices.first().and_then(|c| c.stored) {
                self.apply_model_and_effort(preset.model, Some(effort));
            } else {
                self.apply_model_and_effort(preset.model, None);
            }
            return;
        }

        let default_choice: Option<ReasoningEffortConfig> = choices
            .iter()
            .any(|choice| choice.stored == Some(default_effort))
            .then_some(Some(default_effort))
            .flatten()
            .or_else(|| choices.iter().find_map(|choice| choice.stored))
            .or(Some(default_effort));

        let model_slug = preset.model.to_string();
        let is_current_model = self.model_family.get_model_slug() == preset.model;
        let highlight_choice = if is_current_model {
            self.config.model_reasoning_effort
        } else {
            default_choice
        };
        let selection_choice = highlight_choice.or(default_choice);
        let initial_selected_idx = choices
            .iter()
            .position(|choice| choice.stored == selection_choice)
            .or_else(|| {
                selection_choice
                    .and_then(|effort| choices.iter().position(|choice| choice.display == effort))
            });
        let mut items: Vec<SelectionItem> = Vec::new();
        for choice in choices.iter() {
            let effort = choice.display;
            let mut effort_label = Self::reasoning_effort_label(effort).to_string();
            if choice.stored == default_choice {
                effort_label.push_str(" (default)");
            }

            let description = choice
                .stored
                .and_then(|effort| {
                    supported
                        .iter()
                        .find(|option| option.effort == effort)
                        .map(|option| option.description.to_string())
                })
                .filter(|text| !text.is_empty());

            let show_warning = warn_for_model && warn_effort == Some(effort);
            let selected_description = if show_warning {
                warning_text.as_ref().map(|warning_message| {
                    description.as_ref().map_or_else(
                        || warning_message.clone(),
                        |d| format!("{d}\n{warning_message}"),
                    )
                })
            } else {
                None
            };

            let model_for_action = model_slug.clone();
            let actions = Self::model_selection_actions(model_for_action, choice.stored);

            items.push(SelectionItem {
                name: effort_label,
                description,
                selected_description,
                is_current: is_current_model && choice.stored == highlight_choice,
                actions,
                dismiss_on_select: true,
                ..Default::default()
            });
        }

        let mut header = ColumnRenderable::new();
        header.push(Line::from(
            format!("Select Reasoning Level for {model_slug}").bold(),
        ));

        self.bottom_pane.show_selection_view(SelectionViewParams {
            header: Box::new(header),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            initial_selected_idx,
            ..Default::default()
        });
    }

    fn reasoning_effort_label(effort: ReasoningEffortConfig) -> &'static str {
        match effort {
            ReasoningEffortConfig::None => "None",
            ReasoningEffortConfig::Minimal => "Minimal",
            ReasoningEffortConfig::Low => "Low",
            ReasoningEffortConfig::Medium => "Medium",
            ReasoningEffortConfig::High => "High",
            ReasoningEffortConfig::XHigh => "Extra high",
        }
    }

    fn queued_override_info_message(
        &self,
        model: &str,
        reasoning_effort: Option<ReasoningEffortConfig>,
    ) -> String {
        let mut message = format!("Model changed to {model}");

        if !model.starts_with("codex-auto-") {
            let label = match reasoning_effort {
                Some(ReasoningEffortConfig::Minimal) => "minimal",
                Some(ReasoningEffortConfig::Low) => "low",
                Some(ReasoningEffortConfig::Medium) => "medium",
                Some(ReasoningEffortConfig::High) => "high",
                Some(ReasoningEffortConfig::XHigh) => "xhigh",
                None | Some(ReasoningEffortConfig::None) => "default",
            };

            message.push(' ');
            message.push_str(label);
        }

        message.push_str(" (queued message)");

        message
    }

    fn thinking_label_for(
        model: &str,
        effort: Option<ReasoningEffortConfig>,
    ) -> Option<&'static str> {
        if model.starts_with("codex-auto-") {
            return None;
        }

        effort.map(Self::thinking_label)
    }

    fn thinking_label(effort: ReasoningEffortConfig) -> &'static str {
        match effort {
            ReasoningEffortConfig::Minimal => "minimal",
            ReasoningEffortConfig::Low => "low",
            ReasoningEffortConfig::Medium => "medium",
            ReasoningEffortConfig::High => "high",
            ReasoningEffortConfig::XHigh => "xhigh",
            ReasoningEffortConfig::None => "none",
        }
    }

    fn apply_model_and_effort(&self, model: String, effort: Option<ReasoningEffortConfig>) {
        self.app_event_tx
            .send(AppEvent::CodexOp(Op::OverrideTurnContext {
                cwd: None,
                approval_policy: None,
                sandbox_policy: None,
                model: Some(model.clone()),
                effort: Some(effort),
                summary: None,
                mode: None,
            }));
        self.app_event_tx.send(AppEvent::PersistModelSelection {
            model: model.clone(),
            effort,
        });
        tracing::info!(
            "Selected model: {}, Selected effort: {}",
            model,
            effort
                .map(|e| e.to_string())
                .unwrap_or_else(|| "default".to_string())
        );
    }

    /// Open a popup to choose the approvals mode (ask for approval policy + sandbox policy).
    pub(crate) fn open_approvals_popup(&mut self) {
        let current_approval = self.config.approval_policy.value();
        let current_sandbox = self.config.sandbox_policy.get().clone();
        let mut items: Vec<SelectionItem> = Vec::new();
        let presets: Vec<ApprovalPreset> = builtin_approval_presets();
        for preset in presets.into_iter() {
            let is_current =
                Self::preset_matches_current(current_approval, &current_sandbox, &preset);
            let name = preset.label.to_string();
            let description = Some(preset.description.to_string());
            let disabled_reason = match self.config.approval_policy.can_set(&preset.approval) {
                Ok(()) => None,
                Err(err) => Some(err.to_string()),
            };
            let requires_confirmation = preset.id == "full-access"
                && !self
                    .config
                    .notices
                    .hide_full_access_warning
                    .unwrap_or(false);
            let actions: Vec<SelectionAction> = if requires_confirmation {
                let preset_clone = preset.clone();
                vec![Box::new(move |tx| {
                    tx.send(AppEvent::OpenFullAccessConfirmation {
                        preset: preset_clone.clone(),
                    });
                })]
            } else if preset.id == "auto" {
                #[cfg(target_os = "windows")]
                {
                    if codex_core::get_platform_sandbox().is_none() {
                        let preset_clone = preset.clone();
                        vec![Box::new(move |tx| {
                            tx.send(AppEvent::OpenWindowsSandboxEnablePrompt {
                                preset: preset_clone.clone(),
                            });
                        })]
                    } else if let Some((sample_paths, extra_count, failed_scan)) =
                        self.world_writable_warning_details()
                    {
                        let preset_clone = preset.clone();
                        vec![Box::new(move |tx| {
                            tx.send(AppEvent::OpenWorldWritableWarningConfirmation {
                                preset: Some(preset_clone.clone()),
                                sample_paths: sample_paths.clone(),
                                extra_count,
                                failed_scan,
                            });
                        })]
                    } else {
                        Self::approval_preset_actions(preset.approval, preset.sandbox.clone())
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    Self::approval_preset_actions(preset.approval, preset.sandbox.clone())
                }
            } else {
                Self::approval_preset_actions(preset.approval, preset.sandbox.clone())
            };
            items.push(SelectionItem {
                name,
                description,
                is_current,
                actions,
                dismiss_on_select: true,
                disabled_reason,
                ..Default::default()
            });
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select Approval Mode".to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            header: Box::new(()),
            ..Default::default()
        });
    }

    pub(crate) fn open_experimental_popup(&mut self) {
        let features: Vec<BetaFeatureItem> = FEATURES
            .iter()
            .filter_map(|spec| {
                let name = spec.stage.beta_menu_name()?;
                let description = spec.stage.beta_menu_description()?;
                Some(BetaFeatureItem {
                    feature: spec.id,
                    name: name.to_string(),
                    description: description.to_string(),
                    enabled: self.config.features.enabled(spec.id),
                })
            })
            .collect();

        let view = ExperimentalFeaturesView::new(features, self.app_event_tx.clone());
        self.bottom_pane.show_view(Box::new(view));
    }

    fn approval_preset_actions(
        approval: AskForApproval,
        sandbox: SandboxPolicy,
    ) -> Vec<SelectionAction> {
        vec![Box::new(move |tx| {
            let sandbox_clone = sandbox.clone();
            tx.send(AppEvent::CodexOp(Op::OverrideTurnContext {
                cwd: None,
                approval_policy: Some(approval),
                sandbox_policy: Some(sandbox_clone.clone()),
                model: None,
                effort: None,
                summary: None,
                mode: None,
            }));
            tx.send(AppEvent::UpdateAskForApprovalPolicy(approval));
            tx.send(AppEvent::UpdateSandboxPolicy(sandbox_clone));
        })]
    }

    fn preset_matches_current(
        current_approval: AskForApproval,
        current_sandbox: &SandboxPolicy,
        preset: &ApprovalPreset,
    ) -> bool {
        if current_approval != preset.approval {
            return false;
        }
        matches!(
            (&preset.sandbox, current_sandbox),
            (SandboxPolicy::ReadOnly, SandboxPolicy::ReadOnly)
                | (
                    SandboxPolicy::DangerFullAccess,
                    SandboxPolicy::DangerFullAccess
                )
                | (
                    SandboxPolicy::WorkspaceWrite { .. },
                    SandboxPolicy::WorkspaceWrite { .. }
                )
        )
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn world_writable_warning_details(&self) -> Option<(Vec<String>, usize, bool)> {
        if self
            .config
            .notices
            .hide_world_writable_warning
            .unwrap_or(false)
        {
            return None;
        }
        let cwd = self.config.cwd.clone();
        let env_map: std::collections::HashMap<String, String> = std::env::vars().collect();
        match codex_windows_sandbox::apply_world_writable_scan_and_denies(
            self.config.codex_home.as_path(),
            cwd.as_path(),
            &env_map,
            self.config.sandbox_policy.get(),
            Some(self.config.codex_home.as_path()),
        ) {
            Ok(_) => None,
            Err(_) => Some((Vec::new(), 0, true)),
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[allow(dead_code)]
    pub(crate) fn world_writable_warning_details(&self) -> Option<(Vec<String>, usize, bool)> {
        None
    }

    pub(crate) fn open_full_access_confirmation(&mut self, preset: ApprovalPreset) {
        let approval = preset.approval;
        let sandbox = preset.sandbox;
        let mut header_children: Vec<Box<dyn Renderable>> = Vec::new();
        let title_line = Line::from("Enable full access?").bold();
        let info_line = Line::from(vec![
            "When Codex runs with full access, it can edit any file on your computer and run commands with network, without your approval. "
                .into(),
            "Exercise caution when enabling full access. This significantly increases the risk of data loss, leaks, or unexpected behavior."
                .fg(Color::Red),
        ]);
        header_children.push(Box::new(title_line));
        header_children.push(Box::new(
            Paragraph::new(vec![info_line]).wrap(Wrap { trim: false }),
        ));
        let header = ColumnRenderable::with(header_children);

        let mut accept_actions = Self::approval_preset_actions(approval, sandbox.clone());
        accept_actions.push(Box::new(|tx| {
            tx.send(AppEvent::UpdateFullAccessWarningAcknowledged(true));
        }));

        let mut accept_and_remember_actions = Self::approval_preset_actions(approval, sandbox);
        accept_and_remember_actions.push(Box::new(|tx| {
            tx.send(AppEvent::UpdateFullAccessWarningAcknowledged(true));
            tx.send(AppEvent::PersistFullAccessWarningAcknowledged);
        }));

        let deny_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::OpenApprovalsPopup);
        })];

        let items = vec![
            SelectionItem {
                name: "Yes, continue anyway".to_string(),
                description: Some("Apply full access for this session".to_string()),
                actions: accept_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Yes, and don't ask again".to_string(),
                description: Some("Enable full access and remember this choice".to_string()),
                actions: accept_and_remember_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Cancel".to_string(),
                description: Some("Go back without enabling full access".to_string()),
                actions: deny_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            footer_hint: Some(standard_popup_hint_line()),
            items,
            header: Box::new(header),
            ..Default::default()
        });
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn open_world_writable_warning_confirmation(
        &mut self,
        preset: Option<ApprovalPreset>,
        sample_paths: Vec<String>,
        extra_count: usize,
        failed_scan: bool,
    ) {
        let (approval, sandbox) = match &preset {
            Some(p) => (Some(p.approval), Some(p.sandbox.clone())),
            None => (None, None),
        };
        let mut header_children: Vec<Box<dyn Renderable>> = Vec::new();
        let describe_policy = |policy: &SandboxPolicy| match policy {
            SandboxPolicy::WorkspaceWrite { .. } => "Agent mode",
            SandboxPolicy::ReadOnly => "Read-Only mode",
            _ => "Agent mode",
        };
        let mode_label = preset
            .as_ref()
            .map(|p| describe_policy(&p.sandbox))
            .unwrap_or_else(|| describe_policy(self.config.sandbox_policy.get()));
        let info_line = if failed_scan {
            Line::from(vec![
                "We couldn't complete the world-writable scan, so protections cannot be verified. "
                    .into(),
                format!("The Windows sandbox cannot guarantee protection in {mode_label}.")
                    .fg(Color::Red),
            ])
        } else {
            Line::from(vec![
                "The Windows sandbox cannot protect writes to folders that are writable by Everyone.".into(),
                " Consider removing write access for Everyone from the following folders:".into(),
            ])
        };
        header_children.push(Box::new(
            Paragraph::new(vec![info_line]).wrap(Wrap { trim: false }),
        ));

        if !sample_paths.is_empty() {
            // Show up to three examples and optionally an "and X more" line.
            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(""));
            for p in &sample_paths {
                lines.push(Line::from(format!("  - {p}")));
            }
            if extra_count > 0 {
                lines.push(Line::from(format!("and {extra_count} more")));
            }
            header_children.push(Box::new(Paragraph::new(lines).wrap(Wrap { trim: false })));
        }
        let header = ColumnRenderable::with(header_children);

        // Build actions ensuring acknowledgement happens before applying the new sandbox policy,
        // so downstream policy-change hooks don't re-trigger the warning.
        let mut accept_actions: Vec<SelectionAction> = Vec::new();
        // Suppress the immediate re-scan only when a preset will be applied (i.e., via /approvals),
        // to avoid duplicate warnings from the ensuing policy change.
        if preset.is_some() {
            accept_actions.push(Box::new(|tx| {
                tx.send(AppEvent::SkipNextWorldWritableScan);
            }));
        }
        if let (Some(approval), Some(sandbox)) = (approval, sandbox.clone()) {
            accept_actions.extend(Self::approval_preset_actions(approval, sandbox));
        }

        let mut accept_and_remember_actions: Vec<SelectionAction> = Vec::new();
        accept_and_remember_actions.push(Box::new(|tx| {
            tx.send(AppEvent::UpdateWorldWritableWarningAcknowledged(true));
            tx.send(AppEvent::PersistWorldWritableWarningAcknowledged);
        }));
        if let (Some(approval), Some(sandbox)) = (approval, sandbox) {
            accept_and_remember_actions.extend(Self::approval_preset_actions(approval, sandbox));
        }

        let items = vec![
            SelectionItem {
                name: "Continue".to_string(),
                description: Some(format!("Apply {mode_label} for this session")),
                actions: accept_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Continue and don't warn again".to_string(),
                description: Some(format!("Enable {mode_label} and remember this choice")),
                actions: accept_and_remember_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            footer_hint: Some(standard_popup_hint_line()),
            items,
            header: Box::new(header),
            ..Default::default()
        });
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn open_world_writable_warning_confirmation(
        &mut self,
        _preset: Option<ApprovalPreset>,
        _sample_paths: Vec<String>,
        _extra_count: usize,
        _failed_scan: bool,
    ) {
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn open_windows_sandbox_enable_prompt(&mut self, preset: ApprovalPreset) {
        use ratatui_macros::line;

        let mut header = ColumnRenderable::new();
        header.push(*Box::new(
            Paragraph::new(vec![
                line!["Agent mode on Windows uses an experimental sandbox to limit network and filesystem access.".bold()],
                line![
                    "Learn more: https://developers.openai.com/codex/windows"
                ],
            ])
            .wrap(Wrap { trim: false }),
        ));

        let preset_clone = preset;
        let items = vec![
            SelectionItem {
                name: "Enable experimental sandbox".to_string(),
                description: None,
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::EnableWindowsSandboxForAgentMode {
                        preset: preset_clone.clone(),
                    });
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Go back".to_string(),
                description: None,
                actions: vec![Box::new(|tx| {
                    tx.send(AppEvent::OpenApprovalsPopup);
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: None,
            footer_hint: Some(standard_popup_hint_line()),
            items,
            header: Box::new(header),
            ..Default::default()
        });
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn open_windows_sandbox_enable_prompt(&mut self, _preset: ApprovalPreset) {}

    #[cfg(target_os = "windows")]
    pub(crate) fn maybe_prompt_windows_sandbox_enable(&mut self) {
        if self.config.forced_auto_mode_downgraded_on_windows
            && codex_core::get_platform_sandbox().is_none()
            && let Some(preset) = builtin_approval_presets()
                .into_iter()
                .find(|preset| preset.id == "auto")
        {
            self.open_windows_sandbox_enable_prompt(preset);
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn maybe_prompt_windows_sandbox_enable(&mut self) {}

    #[cfg(target_os = "windows")]
    pub(crate) fn clear_forced_auto_mode_downgrade(&mut self) {
        self.config.forced_auto_mode_downgraded_on_windows = false;
    }

    #[cfg(not(target_os = "windows"))]
    #[allow(dead_code)]
    pub(crate) fn clear_forced_auto_mode_downgrade(&mut self) {}

    /// Set the approval policy in the widget's config copy.
    pub(crate) fn set_approval_policy(&mut self, policy: AskForApproval) {
        if let Err(err) = self.config.approval_policy.set(policy) {
            tracing::warn!(%err, "failed to set approval_policy on chat config");
        }
    }

    /// Set the sandbox policy in the widget's config copy.
    pub(crate) fn set_sandbox_policy(&mut self, policy: SandboxPolicy) {
        #[cfg(target_os = "windows")]
        let should_clear_downgrade = !matches!(policy, SandboxPolicy::ReadOnly)
            || codex_core::get_platform_sandbox().is_some();

        if let Err(err) = self.config.sandbox_policy.set(policy) {
            tracing::warn!(%err, "failed to set sandbox_policy on chat config");
        }

        #[cfg(target_os = "windows")]
        if should_clear_downgrade {
            self.config.forced_auto_mode_downgraded_on_windows = false;
        }
    }

    pub(crate) fn set_feature_enabled(&mut self, feature: Feature, enabled: bool) {
        if enabled {
            self.config.features.enable(feature);
        } else {
            self.config.features.disable(feature);
        }
    }

    pub(crate) fn set_full_access_warning_acknowledged(&mut self, acknowledged: bool) {
        self.config.notices.hide_full_access_warning = Some(acknowledged);
    }

    pub(crate) fn set_world_writable_warning_acknowledged(&mut self, acknowledged: bool) {
        self.config.notices.hide_world_writable_warning = Some(acknowledged);
    }

    pub(crate) fn set_rate_limit_switch_prompt_hidden(&mut self, hidden: bool) {
        self.config.notices.hide_rate_limit_model_nudge = Some(hidden);
        if hidden {
            self.rate_limit_switch_prompt = RateLimitSwitchPromptState::Idle;
        }
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn world_writable_warning_hidden(&self) -> bool {
        self.config
            .notices
            .hide_world_writable_warning
            .unwrap_or(false)
    }

    fn effective_reasoning_effort(&self) -> Option<ReasoningEffortConfig> {
        self.config
            .model_reasoning_effort
            .or(self.model_family.default_reasoning_effort)
    }

    /// Set the reasoning effort in the widget's config copy.
    pub(crate) fn set_reasoning_effort(&mut self, effort: Option<ReasoningEffortConfig>) {
        self.config.model_reasoning_effort = effort;
        self.bottom_pane
            .set_session_reasoning_effort(self.effective_reasoning_effort());

        self.request_redraw();
    }

    /// Set the model in the widget's config copy.
    pub(crate) fn set_model(&mut self, model: &str, model_family: ModelFamily) {
        self.config.model = Some(model.to_string());
        self.session_header.set_model(model);
        self.model_family = model_family;
        self.bottom_pane.set_session_model(model.to_string());
        self.bottom_pane
            .set_session_reasoning_effort(self.effective_reasoning_effort());

        self.request_redraw();
    }

    pub(crate) fn set_status_line_git_branch(&mut self, branch: Option<String>) {
        self.bottom_pane.set_status_line_git_branch(branch);
    }

    pub(crate) fn add_info_message(&mut self, message: String, hint: Option<String>) {
        self.add_to_history(history_cell::new_info_event(message, hint));
        self.request_redraw();
    }

    pub(crate) fn add_plain_history_lines(&mut self, lines: Vec<Line<'static>>) {
        self.add_boxed_history(Box::new(PlainHistoryCell::new(lines)));
        self.request_redraw();
    }

    pub(crate) fn add_error_message(&mut self, message: String) {
        self.add_to_history(history_cell::new_error_event(message));
        self.request_redraw();
    }

    pub(crate) fn add_mcp_output(&mut self) {
        if self.config.mcp_servers.is_empty() {
            self.add_to_history(history_cell::empty_mcp_output());
        } else {
            self.submit_op(Op::ListMcpTools);
        }
    }

    /// Forward file-search results to the bottom pane.
    pub(crate) fn apply_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        self.bottom_pane.on_file_search_result(query, matches);
    }

    /// Handle Ctrl-C key press.
    fn on_ctrl_c(&mut self) {
        if self.bottom_pane.on_ctrl_c() == CancellationEvent::Handled {
            self.maybe_send_next_queued_input();
            return;
        }

        if self.bottom_pane.is_task_running() {
            self.bottom_pane.show_ctrl_c_quit_hint();
            self.submit_op(Op::Interrupt);
            return;
        }

        self.submit_op(Op::Shutdown);
    }

    pub(crate) fn composer_is_empty(&self) -> bool {
        self.bottom_pane.composer_is_empty()
    }

    /// True when the UI is in the regular composer state with no running task,
    /// no modal overlay (e.g. approvals or status indicator), and no composer popups.
    /// In this state Esc-Esc backtracking is enabled.
    pub(crate) fn is_normal_backtrack_mode(&self) -> bool {
        self.bottom_pane.is_normal_backtrack_mode()
    }

    pub(crate) fn insert_str(&mut self, text: &str) {
        self.bottom_pane.insert_str(text);
    }

    /// Replace the composer content with the provided text and reset cursor.
    pub(crate) fn set_composer_text(&mut self, text: String) {
        self.bottom_pane.set_composer_text(text);
    }

    pub(crate) fn set_composer_text_with_attachments(
        &mut self,
        text: String,
        attachments: Vec<ComposerAttachment>,
    ) {
        self.bottom_pane
            .set_composer_text_with_attachments(text, attachments);
    }

    pub(crate) fn show_esc_backtrack_hint(&mut self) {
        self.bottom_pane.show_esc_backtrack_hint();
    }

    pub(crate) fn clear_esc_backtrack_hint(&mut self) {
        self.bottom_pane.clear_esc_backtrack_hint();
    }
    /// Forward an `Op` directly to codex.
    pub(crate) fn submit_op(&self, op: Op) {
        // Record outbound operation for session replay fidelity.
        crate::session_log::log_outbound_op(&op);
        if let Err(e) = self.codex_op_tx.send(op) {
            tracing::error!("failed to submit op: {e}");
        }
    }

    fn on_list_mcp_tools(&mut self, ev: McpListToolsResponseEvent) {
        self.add_to_history(history_cell::new_mcp_tools_output(
            &self.config,
            ev.tools,
            ev.resources,
            ev.resource_templates,
            &ev.auth_statuses,
        ));
    }

    fn on_list_custom_prompts(&mut self, ev: ListCustomPromptsResponseEvent) {
        let len = ev.custom_prompts.len();
        debug!("received {len} custom prompts");
        // Forward to bottom pane so the slash popup can show them now.
        self.bottom_pane.set_custom_prompts(ev.custom_prompts);
    }

    fn on_list_skills(&mut self, ev: ListSkillsResponseEvent) {
        self.set_skills_from_response(&ev);
    }

    fn mode_label(mode: SessionMode) -> &'static str {
        match mode {
            SessionMode::Normal => "Normal",
            SessionMode::Plan => "Plan",
            SessionMode::Ask => "Ask",
        }
    }

    fn set_session_mode(&mut self, mode: SessionMode) {
        if self.session_mode == mode {
            let label = Self::mode_label(mode);
            self.add_info_message(format!("{label} mode is already active."), None);
            return;
        }
        self.session_mode = mode;
        self.bottom_pane.set_session_mode(mode);
        self.submit_op(Op::OverrideTurnContext {
            cwd: None,
            approval_policy: None,
            sandbox_policy: None,
            model: None,
            effort: None,
            summary: None,
            mode: Some(mode),
        });
        let message = match mode {
            SessionMode::Normal => "Switched to normal mode (edits allowed).".to_string(),
            SessionMode::Plan => "Switched to plan mode (no edits).".to_string(),
            SessionMode::Ask => "Switched to ask mode (no edits).".to_string(),
        };
        self.add_info_message(message, None);
    }

    pub(crate) fn open_review_popup(&mut self) {
        let mut items: Vec<SelectionItem> = Vec::new();

        items.push(SelectionItem {
            name: "Review against a base branch".to_string(),
            description: Some("(PR Style)".into()),
            actions: vec![Box::new({
                let cwd = self.config.cwd.clone();
                move |tx| {
                    tx.send(AppEvent::OpenReviewBranchPicker(cwd.clone()));
                }
            })],
            dismiss_on_select: false,
            ..Default::default()
        });

        items.push(SelectionItem {
            name: "Review uncommitted changes".to_string(),
            actions: vec![Box::new(move |tx: &AppEventSender| {
                tx.send(AppEvent::CodexOp(Op::Review {
                    review_request: ReviewRequest {
                        target: ReviewTarget::UncommittedChanges,
                        user_facing_hint: None,
                    },
                }));
            })],
            dismiss_on_select: true,
            ..Default::default()
        });

        // New: Review a specific commit (opens commit picker)
        items.push(SelectionItem {
            name: "Review a commit".to_string(),
            actions: vec![Box::new({
                let cwd = self.config.cwd.clone();
                move |tx| {
                    tx.send(AppEvent::OpenReviewCommitPicker(cwd.clone()));
                }
            })],
            dismiss_on_select: false,
            ..Default::default()
        });

        items.push(SelectionItem {
            name: "Custom review instructions".to_string(),
            actions: vec![Box::new(move |tx| {
                tx.send(AppEvent::OpenReviewCustomPrompt);
            })],
            dismiss_on_select: false,
            ..Default::default()
        });

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select a review preset".into()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            ..Default::default()
        });
    }

    pub(crate) fn open_backtrack_action_picker(&mut self) {
        let mut items: Vec<SelectionItem> = Vec::new();
        let cancel_action: SelectionAction = Box::new(|tx| {
            tx.send(AppEvent::BacktrackActionCanceled);
        });

        items.push(SelectionItem {
            name: "Edit in place".to_string(),
            description: Some("Keep this chat and add edit versions.".into()),
            actions: vec![Box::new(|tx| {
                tx.send(AppEvent::BacktrackActionSelected {
                    action: BacktrackActionRequest::EditInPlace,
                });
            })],
            dismiss_on_select: true,
            is_default: true,
            ..Default::default()
        });

        items.push(SelectionItem {
            name: "Retry same message".to_string(),
            description: Some("Keep this chat and resend immediately.".into()),
            actions: vec![Box::new(|tx| {
                tx.send(AppEvent::BacktrackActionSelected {
                    action: BacktrackActionRequest::RetrySame,
                });
            })],
            dismiss_on_select: true,
            ..Default::default()
        });

        items.push(SelectionItem {
            name: "Resend with different model/thinking".to_string(),
            description: Some("Choose a model and reasoning level, then resend.".into()),
            actions: vec![Box::new(|tx| {
                tx.send(AppEvent::BacktrackActionSelected {
                    action: BacktrackActionRequest::ResendWithDifferentModel,
                });
            })],
            dismiss_on_select: true,
            ..Default::default()
        });

        items.push(SelectionItem {
            name: "Branch to new chat".to_string(),
            description: Some("Start a new chat from this message.".into()),
            actions: vec![Box::new(|tx| {
                tx.send(AppEvent::BacktrackActionSelected {
                    action: BacktrackActionRequest::Branch,
                });
            })],
            dismiss_on_select: true,
            ..Default::default()
        });

        items.push(SelectionItem {
            name: "Back".to_string(),
            description: Some("Return to message navigation.".into()),
            actions: vec![Box::new(|tx| {
                tx.send(AppEvent::BacktrackActionCanceled);
            })],
            dismiss_on_select: true,
            ..Default::default()
        });

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Edit previous message".into()),
            subtitle: Some("Choose how to edit or resend.".into()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            cancel_action: Some(cancel_action),
            ..Default::default()
        });
    }

    pub(crate) fn open_backtrack_resend_model_picker(&mut self) {
        let current_model = self.model_family.get_model_slug().to_string();
        let mut presets = match self.models_manager.try_list_models(&self.config) {
            Ok(presets) => presets,
            Err(_) => {
                self.add_info_message(
                    "Models are being updated; please try again in a moment.".to_string(),
                    None,
                );
                return;
            }
        };
        presets.sort_by(|a, b| a.display_name.cmp(&b.display_name));

        let items: Vec<SelectionItem> = presets
            .into_iter()
            .map(|preset| {
                let description =
                    (!preset.description.is_empty()).then_some(preset.description.clone());
                let is_current = preset.model == current_model;
                let preset_for_action = preset.clone();
                let actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
                    tx.send(AppEvent::OpenBacktrackResendThinkingPicker {
                        preset: preset_for_action.clone(),
                    });
                })];
                SelectionItem {
                    name: preset.display_name,
                    description,
                    is_current,
                    actions,
                    dismiss_on_select: true,
                    ..Default::default()
                }
            })
            .collect();

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select model for resend".to_string()),
            subtitle: Some("Choose a model and reasoning level for this retry.".to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            ..Default::default()
        });
    }

    pub(crate) fn open_backtrack_resend_thinking_picker(&mut self, preset: ModelPreset) {
        let model_slug = preset.model.clone();
        let default_effort = preset.default_reasoning_effort;
        let has_alternative = preset
            .supported_reasoning_efforts
            .iter()
            .any(|option| option.effort != default_effort);

        if !has_alternative {
            self.app_event_tx.send(AppEvent::BacktrackActionSelected {
                action: BacktrackActionRequest::ResendWithOverrides(ResendOverrides {
                    model: model_slug,
                    effort: None,
                }),
            });
            return;
        }

        let mut items: Vec<SelectionItem> = Vec::new();
        let model_for_default = model_slug.clone();
        let default_actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
            tx.send(AppEvent::BacktrackActionSelected {
                action: BacktrackActionRequest::ResendWithOverrides(ResendOverrides {
                    model: model_for_default.clone(),
                    effort: None,
                }),
            });
        })];
        items.push(SelectionItem {
            name: format!("Default ({})", Self::reasoning_effort_label(default_effort)),
            description: Some("Use the model's default reasoning level.".to_string()),
            actions: default_actions,
            dismiss_on_select: true,
            ..Default::default()
        });

        for option in preset.supported_reasoning_efforts {
            let effort = option.effort;
            let mut label = Self::reasoning_effort_label(effort).to_string();
            if effort == default_effort {
                label.push_str(" (default)");
            }
            let description = (!option.description.is_empty()).then_some(option.description);
            let model_for_action = model_slug.clone();
            let actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
                tx.send(AppEvent::BacktrackActionSelected {
                    action: BacktrackActionRequest::ResendWithOverrides(ResendOverrides {
                        model: model_for_action.clone(),
                        effort: Some(effort),
                    }),
                });
            })];
            items.push(SelectionItem {
                name: label,
                description,
                actions,
                dismiss_on_select: true,
                ..Default::default()
            });
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select thinking for resend".to_string()),
            subtitle: Some(format!("Model: {model_slug}")),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            ..Default::default()
        });
    }

    pub(crate) async fn show_review_branch_picker(&mut self, cwd: &Path) {
        let branches = local_git_branches(cwd).await;
        let current_branch = current_branch_name(cwd)
            .await
            .unwrap_or_else(|| "(detached HEAD)".to_string());
        let mut items: Vec<SelectionItem> = Vec::with_capacity(branches.len());

        for option in branches {
            let branch = option.clone();
            items.push(SelectionItem {
                name: format!("{current_branch} -> {branch}"),
                actions: vec![Box::new(move |tx3: &AppEventSender| {
                    tx3.send(AppEvent::CodexOp(Op::Review {
                        review_request: ReviewRequest {
                            target: ReviewTarget::BaseBranch {
                                branch: branch.clone(),
                            },
                            user_facing_hint: None,
                        },
                    }));
                })],
                dismiss_on_select: true,
                search_value: Some(option),
                ..Default::default()
            });
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select a base branch".to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            is_searchable: true,
            search_placeholder: Some("Type to search branches".to_string()),
            ..Default::default()
        });
    }

    pub(crate) async fn show_review_commit_picker(&mut self, cwd: &Path) {
        let commits = codex_core::git_info::recent_commits(cwd, 100).await;

        let mut items: Vec<SelectionItem> = Vec::with_capacity(commits.len());
        for entry in commits {
            let subject = entry.subject.clone();
            let sha = entry.sha.clone();
            let search_val = format!("{subject} {sha}");

            items.push(SelectionItem {
                name: subject.clone(),
                actions: vec![Box::new(move |tx3: &AppEventSender| {
                    tx3.send(AppEvent::CodexOp(Op::Review {
                        review_request: ReviewRequest {
                            target: ReviewTarget::Commit {
                                sha: sha.clone(),
                                title: Some(subject.clone()),
                            },
                            user_facing_hint: None,
                        },
                    }));
                })],
                dismiss_on_select: true,
                search_value: Some(search_val),
                ..Default::default()
            });
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select a commit to review".to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            is_searchable: true,
            search_placeholder: Some("Type to search commits".to_string()),
            ..Default::default()
        });
    }

    pub(crate) fn show_review_custom_prompt(&mut self) {
        let tx = self.app_event_tx.clone();
        let view = CustomPromptView::new(
            "Custom review instructions".to_string(),
            "Type instructions and press Enter".to_string(),
            None,
            Box::new(move |prompt: String| {
                let trimmed = prompt.trim().to_string();
                if trimmed.is_empty() {
                    return;
                }
                tx.send(AppEvent::CodexOp(Op::Review {
                    review_request: ReviewRequest {
                        target: ReviewTarget::Custom {
                            instructions: trimmed,
                        },
                        user_facing_hint: None,
                    },
                }));
            }),
        );
        self.bottom_pane.show_view(Box::new(view));
    }

    pub(crate) fn token_usage(&self) -> TokenUsage {
        self.token_info
            .as_ref()
            .map(|ti| ti.total_token_usage.clone())
            .unwrap_or_default()
    }

    pub(crate) fn conversation_id(&self) -> Option<ConversationId> {
        self.conversation_id
    }

    pub(crate) fn rollout_path(&self) -> Option<PathBuf> {
        self.current_rollout_path.clone()
    }

    /// Return a reference to the widget's current config (includes any
    /// runtime overrides applied via TUI, e.g., model or approval policy).
    pub(crate) fn config_ref(&self) -> &Config {
        &self.config
    }

    pub(crate) fn clear_token_usage(&mut self) {
        self.token_info = None;
    }

    pub(crate) fn update_layout_for_screen(&mut self, width: u16, height: u16) {
        if self.queued_edit_state.is_some() {
            let cap = self.bottom_pane.composer_height_cap_for_screen(
                width,
                height,
                MIN_ACTIVE_CELL_HEIGHT,
            );
            self.bottom_pane.set_composer_height_cap(Some(cap));
        } else {
            self.bottom_pane.set_composer_height_cap(None);
        }
    }

    fn as_renderable(&self) -> RenderableItem<'_> {
        let active_cell_renderable = match &self.active_cell {
            Some(cell) => RenderableItem::Borrowed(cell).inset(Insets::tlbr(1, 0, 0, 0)),
            None => RenderableItem::Owned(Box::new(())),
        };
        let mut flex = FlexRenderable::new();
        flex.push(1, active_cell_renderable);
        flex.push(
            0,
            RenderableItem::Borrowed(&self.bottom_pane).inset(Insets::tlbr(1, 0, 0, 0)),
        );
        RenderableItem::Owned(Box::new(flex))
    }
}

impl Drop for ChatWidget {
    fn drop(&mut self) {
        self.stop_rate_limit_poller();
    }
}

impl Renderable for ChatWidget {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.as_renderable().render(area, buf);
        self.last_rendered_width.set(Some(area.width as usize));
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.as_renderable().desired_height(width)
    }

    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.as_renderable().cursor_pos(area)
    }
}

enum Notification {
    AgentTurnComplete { response: String },
    ExecApprovalRequested { command: String },
    EditApprovalRequested { cwd: PathBuf, changes: Vec<PathBuf> },
    ElicitationRequested { server_name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NotificationFocusAction {
    Enable,
    Disable,
    Toggle,
    Reset,
    Status,
}

impl Notification {
    fn display(&self) -> String {
        match self {
            Notification::AgentTurnComplete { response } => {
                Notification::agent_turn_preview(response)
                    .unwrap_or_else(|| "Agent turn complete".to_string())
            }
            Notification::ExecApprovalRequested { command } => {
                format!("Approval requested: {}", truncate_text(command, 30))
            }
            Notification::EditApprovalRequested { cwd, changes } => {
                format!(
                    "Codex wants to edit {}",
                    if changes.len() == 1 {
                        #[allow(clippy::unwrap_used)]
                        display_path_for(changes.first().unwrap(), cwd)
                    } else {
                        format!("{} files", changes.len())
                    }
                )
            }
            Notification::ElicitationRequested { server_name } => {
                format!("Approval requested by {server_name}")
            }
        }
    }

    fn type_name(&self) -> &str {
        match self {
            Notification::AgentTurnComplete { .. } => "agent-turn-complete",
            Notification::ExecApprovalRequested { .. }
            | Notification::EditApprovalRequested { .. }
            | Notification::ElicitationRequested { .. } => "approval-requested",
        }
    }

    fn allowed_for(&self, settings: &Notifications) -> bool {
        match settings {
            Notifications::Enabled(enabled) => *enabled,
            Notifications::Custom(allowed) => allowed.iter().any(|a| a == self.type_name()),
        }
    }

    fn agent_turn_preview(response: &str) -> Option<String> {
        let mut normalized = String::new();
        for part in response.split_whitespace() {
            if !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push_str(part);
        }
        let trimmed = normalized.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(truncate_text(trimmed, AGENT_NOTIFICATION_PREVIEW_GRAPHEMES))
        }
    }
}

fn parse_notification_focus_action(input: Option<&str>) -> Result<NotificationFocusAction, String> {
    let tokens = input
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>();
    let action = match tokens.as_slice() {
        [] => Some(NotificationFocusAction::Toggle),
        ["focus"] => Some(NotificationFocusAction::Toggle),
        ["focus", action] | [action] => parse_notification_focus_action_token(action),
        _ => None,
    };
    action.ok_or_else(|| "Usage: /notifications [focus] <on|off|toggle|reset|status>".to_string())
}

fn parse_notification_focus_action_token(token: &str) -> Option<NotificationFocusAction> {
    match token.to_ascii_lowercase().as_str() {
        "on" | "enable" => Some(NotificationFocusAction::Enable),
        "off" | "disable" => Some(NotificationFocusAction::Disable),
        "toggle" => Some(NotificationFocusAction::Toggle),
        "reset" => Some(NotificationFocusAction::Reset),
        "status" => Some(NotificationFocusAction::Status),
        _ => None,
    }
}

const AGENT_NOTIFICATION_PREVIEW_GRAPHEMES: usize = 200;

const EXAMPLE_PROMPTS: [&str; 6] = [
    "Explain this codebase",
    "Summarize recent commits",
    "Implement {feature}",
    "Find and fix a bug in @filename",
    "Write tests for @filename",
    "Improve documentation in @filename",
];

// Extract the first bold (Markdown) element in the form **...** from `s`.
// Returns the inner text if found; otherwise `None`.
fn extract_first_bold(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'*' {
            let start = i + 2;
            let mut j = start;
            while j + 1 < bytes.len() {
                if bytes[j] == b'*' && bytes[j + 1] == b'*' {
                    // Found closing **
                    let inner = &s[start..j];
                    let trimmed = inner.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    } else {
                        return None;
                    }
                }
                j += 1;
            }
            // No closing; stop searching (wait for more deltas)
            return None;
        }
        i += 1;
    }
    None
}

async fn fetch_rate_limits(base_url: String, auth: CodexAuth) -> Option<RateLimitSnapshot> {
    match BackendClient::from_auth(base_url, &auth).await {
        Ok(client) => match client.get_rate_limits().await {
            Ok(snapshot) => Some(snapshot),
            Err(err) => {
                debug!(error = ?err, "failed to fetch rate limits from /usage");
                None
            }
        },
        Err(err) => {
            debug!(error = ?err, "failed to construct backend client for rate limits");
            None
        }
    }
}

#[cfg(test)]
pub(crate) fn show_review_commit_picker_with_entries(
    chat: &mut ChatWidget,
    entries: Vec<codex_core::git_info::CommitLogEntry>,
) {
    let mut items: Vec<SelectionItem> = Vec::with_capacity(entries.len());
    for entry in entries {
        let subject = entry.subject.clone();
        let sha = entry.sha.clone();
        let search_val = format!("{subject} {sha}");

        items.push(SelectionItem {
            name: subject.clone(),
            actions: vec![Box::new(move |tx3: &AppEventSender| {
                tx3.send(AppEvent::CodexOp(Op::Review {
                    review_request: ReviewRequest {
                        target: ReviewTarget::Commit {
                            sha: sha.clone(),
                            title: Some(subject.clone()),
                        },
                        user_facing_hint: None,
                    },
                }));
            })],
            dismiss_on_select: true,
            search_value: Some(search_val),
            ..Default::default()
        });
    }

    chat.bottom_pane.show_selection_view(SelectionViewParams {
        title: Some("Select a commit to review".to_string()),
        footer_hint: Some(standard_popup_hint_line()),
        items,
        is_searchable: true,
        search_placeholder: Some("Type to search commits".to_string()),
        ..Default::default()
    });
}

fn skills_for_cwd(cwd: &Path, skills_entries: &[SkillsListEntry]) -> Vec<SkillMetadata> {
    skills_entries
        .iter()
        .find(|entry| entry.cwd.as_path() == cwd)
        .map(|entry| {
            entry
                .skills
                .iter()
                .map(|skill| SkillMetadata {
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    short_description: skill.short_description.clone(),
                    path: skill.path.clone(),
                    scope: skill.scope,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn find_skill_mentions(text: &str, skills: &[SkillMetadata]) -> Vec<SkillMetadata> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut matches: Vec<SkillMetadata> = Vec::new();
    for skill in skills {
        if seen.contains(&skill.name) {
            continue;
        }
        let needle = format!("${}", skill.name);
        if text.contains(&needle) {
            seen.insert(skill.name.clone());
            matches.push(skill.clone());
        }
    }
    matches
}

#[derive(Debug, Default)]
struct ParsedExportArgs {
    format: Option<ChatExportFormat>,
    overrides: ExportOverrides,
}

#[derive(Debug, Clone)]
struct ExportDefaults {
    cwd: PathBuf,
    export_dir: Option<PathBuf>,
    export_name: Option<String>,
    export_format: Option<ChatExportFormat>,
}

impl ExportDefaults {
    fn from_config(config: &Config) -> Self {
        Self {
            cwd: config.cwd.clone(),
            export_dir: config.tui_export_dir.clone(),
            export_name: config.tui_export_name.clone(),
            export_format: config.tui_export_format.map(|format| match format {
                ExportFormat::Markdown => ChatExportFormat::Markdown,
                ExportFormat::Json => ChatExportFormat::Json,
            }),
        }
    }
}

#[derive(Debug)]
struct ExportDestination {
    path: PathBuf,
    format: ChatExportFormat,
}

#[derive(Debug)]
enum ExportPathSpec {
    File(PathBuf),
    Dir(PathBuf),
}

fn parse_export_args(input: Option<&str>, cwd: &Path) -> Result<Option<ParsedExportArgs>, String> {
    let Some(input) = input else {
        return Ok(None);
    };
    let Some((name, rest)) = parse_slash_name(input) else {
        return Ok(None);
    };
    if name != SlashCommand::Export.command() {
        return Ok(None);
    }

    let args = parse_positional_args(rest);
    if args.is_empty() {
        return Ok(None);
    }

    let mut parsed = ParsedExportArgs::default();
    let mut positional: Option<String> = None;
    let mut args_iter = args.into_iter().peekable();
    while let Some(arg) = args_iter.next() {
        if arg == "--" {
            let remaining: Vec<String> = args_iter.collect();
            if remaining.is_empty() {
                return Err("Expected a path after --.".to_string());
            }
            if remaining.len() > 1 {
                return Err(format!(
                    "Unexpected /export arguments: {}",
                    remaining.join(" ")
                ));
            }
            positional = Some(remaining[0].clone());
            break;
        }

        if let Some(value) = arg.strip_prefix("--format=") {
            parsed.format = Some(parse_export_format(value)?);
            continue;
        }
        if arg == "--format" {
            let Some(value) = args_iter.next() else {
                return Err("Expected a value after --format.".to_string());
            };
            parsed.format = Some(parse_export_format(&value)?);
            continue;
        }
        if arg == "--json" {
            parsed.format = Some(ChatExportFormat::Json);
            continue;
        }
        if arg == "--markdown" || arg == "--md" {
            parsed.format = Some(ChatExportFormat::Markdown);
            continue;
        }
        if let Some(value) = arg.strip_prefix("--name=") {
            parsed.overrides.name = Some(value.to_string());
            continue;
        }
        if arg == "--name" {
            let Some(value) = args_iter.next() else {
                return Err("Expected a value after --name.".to_string());
            };
            parsed.overrides.name = Some(value);
            continue;
        }
        if arg == "-C" {
            let Some(value) = args_iter.next() else {
                return Err("Expected a directory after -C.".to_string());
            };
            parsed.overrides.output_dir = Some(PathBuf::from(value));
            continue;
        }
        if let Some(value) = arg.strip_prefix("--output=") {
            if value.ends_with('/') || value.ends_with('\\') {
                return Err("Expected a file path after --output.".to_string());
            }
            parsed.overrides.output_path = Some(PathBuf::from(value));
            continue;
        }
        if arg == "-o" || arg == "--output" {
            let Some(value) = args_iter.next() else {
                return Err("Expected a path after -o/--output.".to_string());
            };
            if value.ends_with('/') || value.ends_with('\\') {
                return Err("Expected a file path after -o/--output.".to_string());
            }
            parsed.overrides.output_path = Some(PathBuf::from(value));
            continue;
        }
        if arg.starts_with('-') {
            return Err(format!("Unknown /export flag: {arg}"));
        }
        if positional.is_some() {
            return Err(format!("Unexpected /export argument: {arg}"));
        }
        positional = Some(arg);
    }

    if let Some(positional) = positional {
        if parsed.overrides.output_path.is_some() || parsed.overrides.output_dir.is_some() {
            return Err(
                "Provide only one export path (use -o, -C, or a single positional path)."
                    .to_string(),
            );
        }
        match classify_export_path(&positional, cwd) {
            ExportPathSpec::File(path) => parsed.overrides.output_path = Some(path),
            ExportPathSpec::Dir(path) => parsed.overrides.output_dir = Some(path),
        }
    }

    Ok(Some(parsed))
}

fn parse_export_format(value: &str) -> Result<ChatExportFormat, String> {
    match value.to_ascii_lowercase().as_str() {
        "md" | "markdown" => Ok(ChatExportFormat::Markdown),
        "json" => Ok(ChatExportFormat::Json),
        _ => Err(format!(
            "Unknown export format: {value} (expected md or json)."
        )),
    }
}

fn export_overrides_from_path_input(value: &str, cwd: &Path) -> ExportOverrides {
    match classify_export_path(value, cwd) {
        ExportPathSpec::File(path) => ExportOverrides {
            output_path: Some(path),
            ..Default::default()
        },
        ExportPathSpec::Dir(path) => ExportOverrides {
            output_dir: Some(path),
            ..Default::default()
        },
    }
}

fn classify_export_path(value: &str, cwd: &Path) -> ExportPathSpec {
    let ends_with_separator = value.ends_with('/') || value.ends_with('\\');
    let path = PathBuf::from(value);
    if ends_with_separator {
        return ExportPathSpec::Dir(path);
    }

    let resolved = resolve_relative_path(cwd, &path);
    if resolved.is_dir() {
        ExportPathSpec::Dir(path)
    } else {
        ExportPathSpec::File(path)
    }
}

fn resolve_export_destination(
    rollout_path: &Path,
    defaults: &ExportDefaults,
    format_override: Option<ChatExportFormat>,
    overrides: &ExportOverrides,
) -> Result<ExportDestination, String> {
    let output_path = overrides
        .output_path
        .as_ref()
        .map(|path| resolve_relative_path(&defaults.cwd, path));
    let output_dir = overrides
        .output_dir
        .as_ref()
        .map(|path| resolve_relative_path(&defaults.cwd, path))
        .or_else(|| {
            defaults
                .export_dir
                .as_ref()
                .map(|path| resolve_relative_path(&defaults.cwd, path))
        });

    let format = format_override
        .or_else(|| {
            output_path
                .as_ref()
                .and_then(|path| format_from_extension(path))
        })
        .or(defaults.export_format)
        .unwrap_or(ChatExportFormat::Markdown);

    if let Some(mut output_path) = output_path {
        if output_path.is_dir() {
            return Err(format!(
                "Export path is a directory: {}",
                output_path.display()
            ));
        }
        if output_path.extension().is_none() {
            output_path.set_extension(format.extension());
        }
        return Ok(ExportDestination {
            path: output_path,
            format,
        });
    }

    let default_dir = rollout_path.parent().unwrap_or_else(|| Path::new("."));
    let output_dir = output_dir.unwrap_or_else(|| default_dir.to_path_buf());
    if output_dir.is_file() {
        return Err(format!(
            "Export directory is a file: {}",
            output_dir.display()
        ));
    }

    let name = if let Some(name) = overrides.name.as_deref() {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("Export name cannot be empty.".to_string());
        }
        trimmed.to_string()
    } else if let Some(name) = defaults.export_name.as_deref() {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("Default export name cannot be empty.".to_string());
        }
        trimmed.to_string()
    } else {
        default_export_name(rollout_path)?
    };

    if name.contains('/') || name.contains('\\') {
        return Err("Export name must not contain path separators.".to_string());
    }

    let mut path = output_dir.join(name);
    path.set_extension(format.extension());
    Ok(ExportDestination { path, format })
}

fn default_export_name(rollout_path: &Path) -> Result<String, String> {
    let Some(stem) = rollout_path.file_stem().and_then(|stem| stem.to_str()) else {
        return Err("Failed to derive export name from rollout path.".to_string());
    };
    if stem.is_empty() {
        return Err("Failed to derive export name from rollout path.".to_string());
    }
    Ok(stem.to_string())
}

fn format_from_extension(path: &Path) -> Option<ChatExportFormat> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" => Some(ChatExportFormat::Markdown),
        "json" => Some(ChatExportFormat::Json),
        _ => None,
    }
}

fn resolve_relative_path(base: &Path, value: &Path) -> PathBuf {
    if value.is_absolute() {
        value.to_path_buf()
    } else {
        base.join(value)
    }
}

fn diff_view_override(input: Option<&str>, default_view: DiffView) -> Result<DiffView, String> {
    let Some(input) = input else {
        return Ok(default_view);
    };
    let Some((name, rest)) = parse_slash_name(input) else {
        return Ok(default_view);
    };
    if name != SlashCommand::Diff.command() {
        return Ok(default_view);
    }

    let args = parse_positional_args(rest);
    if args.is_empty() {
        return Ok(default_view);
    }

    let mut view_override = None;
    let mut args_iter = args.iter().peekable();
    while let Some(arg) = args_iter.next() {
        if let Some(value) = arg.strip_prefix("--view=") {
            view_override = Some(parse_diff_view_value(value)?);
            continue;
        }
        match arg.as_str() {
            "--inline" => view_override = Some(DiffView::Inline),
            "--line" => view_override = Some(DiffView::Line),
            "--side-by-side" => view_override = Some(DiffView::SideBySide),
            "--view" => {
                let Some(value) = args_iter.next() else {
                    return Err(
                        "Expected a value after --view (line, inline, or side-by-side)."
                            .to_string(),
                    );
                };
                view_override = Some(parse_diff_view_value(value)?);
            }
            _ if arg.starts_with('-') => {
                return Err(format!("Unknown /diff flag: {arg}"));
            }
            _ => {
                return Err(format!("Unexpected /diff argument: {arg}"));
            }
        }
    }

    Ok(view_override.unwrap_or(default_view))
}

fn parse_diff_view_value(value: &str) -> Result<DiffView, String> {
    match value {
        "line" => Ok(DiffView::Line),
        "inline" => Ok(DiffView::Inline),
        "side-by-side" | "side_by_side" | "side" => Ok(DiffView::SideBySide),
        _ => Err(format!(
            "Invalid /diff view '{value}'. Use 'line', 'inline', or 'side-by-side'."
        )),
    }
}

#[cfg(test)]
pub(crate) mod tests;
