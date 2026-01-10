use std::path::PathBuf;

use codex_common::approval_presets::ApprovalPreset;
use codex_core::protocol::ConversationPathResponseEvent;
use codex_core::protocol::Event;
use codex_core::protocol::RateLimitSnapshot;
use codex_file_search::FileMatch;
use codex_protocol::openai_models::ModelPreset;

use crate::app_backtrack::BacktrackActionRequest;
use crate::bottom_pane::ApprovalRequest;
use crate::bottom_pane::RenameTarget;
use crate::get_git_diff::GitDiffResult;
use crate::history_cell::HistoryCell;
use crate::session_manager::SessionManagerEntry;
use crate::sessions_picker::SessionView;

use codex_core::features::Feature;
use codex_core::protocol::AskForApproval;
use codex_core::protocol::SandboxPolicy;
use codex_protocol::openai_models::ReasoningEffort;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatExportFormat {
    Markdown,
    Json,
}

impl ChatExportFormat {
    pub(crate) const fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Json => "json",
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown",
            Self::Json => "JSON",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ExportOverrides {
    pub output_path: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub name: Option<String>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(crate) enum AppEvent {
    CodexEvent(Event),

    /// Start a new session.
    NewSession,

    /// Open the sessions picker inside the running TUI session.
    OpenSessionsPicker {
        view: SessionView,
    },

    /// Request to exit the application gracefully.
    ExitRequest,

    /// Forward an `Op` to the Agent. Using an `AppEvent` for this avoids
    /// bubbling channels through layers of widgets.
    CodexOp(codex_core::protocol::Op),

    /// Kick off an asynchronous file search for the given query (text after
    /// the `@`). Previous searches may be cancelled by the app layer so there
    /// is at most one in-flight search.
    StartFileSearch(String),

    /// Result of a completed asynchronous file search. The `query` echoes the
    /// original search term so the UI can decide whether the results are
    /// still relevant.
    FileSearchResult {
        query: String,
        matches: Vec<FileMatch>,
    },

    /// Result of refreshing rate limits
    RateLimitSnapshotFetched(RateLimitSnapshot),

    /// Result of computing a `/diff` command.
    DiffResult(GitDiffResult),

    /// Export the current chat in the selected format.
    ExportChat {
        format: Option<ChatExportFormat>,
        overrides: ExportOverrides,
    },

    /// Prompt for a custom export path.
    OpenExportPathPrompt {
        format: ChatExportFormat,
    },

    /// Result of exporting the current chat.
    ExportResult {
        path: PathBuf,
        messages: usize,
        error: Option<String>,
        format: ChatExportFormat,
    },

    /// Launch the external editor after a normal draw has completed.
    LaunchExternalEditor,

    /// Rename the current session.
    RenameSession {
        title: Option<String>,
    },

    /// Rename a saved session at a specific rollout path.
    RenameSessionPath {
        path: PathBuf,
        title: Option<String>,
    },

    /// Open the rename view for the selected session.
    OpenRenameSessionView {
        target: RenameTarget,
        current_title: Option<String>,
    },

    /// Session manager list loaded in the background.
    SessionManagerLoaded {
        sessions: Vec<SessionManagerEntry>,
    },

    /// Session manager list failed to load.
    SessionManagerLoadFailed {
        message: String,
    },

    /// Switch to a saved session from the session manager.
    SessionManagerSwitch {
        path: PathBuf,
    },

    /// Delete a saved session from the session manager.
    SessionManagerDelete {
        path: PathBuf,
        label: String,
    },

    /// Session manager rename result for a saved session.
    SessionManagerRenameResult {
        path: PathBuf,
        title: Option<String>,
        error: Option<String>,
    },

    /// Session manager delete result for a saved session.
    SessionManagerDeleteResult {
        path: PathBuf,
        label: String,
        error: Option<String>,
    },

    InsertHistoryCell(Box<dyn HistoryCell>),

    StartCommitAnimation,
    StopCommitAnimation,
    CommitTick,

    /// Update the current reasoning effort in the running app and widget.
    UpdateReasoningEffort(Option<ReasoningEffort>),

    /// Update the current model slug in the running app and widget.
    UpdateModel(String),

    /// Update the current git branch name for status line display.
    UpdateStatusLineGitBranch {
        branch: Option<String>,
    },

    /// Persist the selected model and reasoning effort to the appropriate config.
    PersistModelSelection {
        model: String,
        effort: Option<ReasoningEffort>,
    },

    /// Open the reasoning selection popup after picking a model.
    OpenReasoningPopup {
        model: ModelPreset,
    },

    /// Open the full model picker (non-auto models).
    OpenAllModelsPopup {
        models: Vec<ModelPreset>,
    },

    /// Open the confirmation prompt before enabling full access mode.
    OpenFullAccessConfirmation {
        preset: ApprovalPreset,
    },

    /// Open the Windows world-writable directories warning.
    /// If `preset` is `Some`, the confirmation will apply the provided
    /// approval/sandbox configuration on Continue; if `None`, it performs no
    /// policy change and only acknowledges/dismisses the warning.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    OpenWorldWritableWarningConfirmation {
        preset: Option<ApprovalPreset>,
        /// Up to 3 sample world-writable directories to display in the warning.
        sample_paths: Vec<String>,
        /// If there are more than `sample_paths`, this carries the remaining count.
        extra_count: usize,
        /// True when the scan failed (e.g. ACL query error) and protections could not be verified.
        failed_scan: bool,
    },

    /// Prompt to enable the Windows sandbox feature before using Agent mode.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    OpenWindowsSandboxEnablePrompt {
        preset: ApprovalPreset,
    },

    /// Enable the Windows sandbox feature and switch to Agent mode.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    EnableWindowsSandboxForAgentMode {
        preset: ApprovalPreset,
    },

    /// Update the current approval policy in the running app and widget.
    UpdateAskForApprovalPolicy(AskForApproval),

    /// Update the current sandbox policy in the running app and widget.
    UpdateSandboxPolicy(SandboxPolicy),

    /// Update feature flags and persist them to the top-level config.
    UpdateFeatureFlags {
        updates: Vec<(Feature, bool)>,
    },

    /// Update whether the full access warning prompt has been acknowledged.
    UpdateFullAccessWarningAcknowledged(bool),

    /// Update whether the world-writable directories warning has been acknowledged.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    UpdateWorldWritableWarningAcknowledged(bool),

    /// Update whether the rate limit switch prompt has been acknowledged for the session.
    UpdateRateLimitSwitchPromptHidden(bool),

    /// Persist the acknowledgement flag for the full access warning prompt.
    PersistFullAccessWarningAcknowledged,

    /// Persist the acknowledgement flag for the world-writable directories warning.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    PersistWorldWritableWarningAcknowledged,

    /// Persist the acknowledgement flag for the rate limit switch prompt.
    PersistRateLimitSwitchPromptHidden,

    /// Persist the acknowledgement flag for the model migration prompt.
    PersistModelMigrationPromptAcknowledged {
        from_model: String,
        to_model: String,
    },

    /// Skip the next world-writable scan (one-shot) after a user-confirmed continue.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    SkipNextWorldWritableScan,

    /// Re-open the approval presets popup.
    OpenApprovalsPopup,

    /// Forwarded conversation history snapshot from the current conversation.
    ConversationHistory(ConversationPathResponseEvent),

    /// Choose how to apply an Esc backtrack edit.
    BacktrackActionSelected {
        action: BacktrackActionRequest,
    },
    /// Return from the backtrack action picker to message navigation.
    BacktrackActionCanceled,

    /// Open the resend thinking picker for a backtrack retry.
    OpenBacktrackResendThinkingPicker {
        preset: ModelPreset,
    },

    /// Open the branch picker option from the review popup.
    OpenReviewBranchPicker(PathBuf),

    /// Open the commit picker option from the review popup.
    OpenReviewCommitPicker(PathBuf),

    /// Open the custom prompt option from the review popup.
    OpenReviewCustomPrompt,

    /// Open the approval popup.
    FullScreenApprovalRequest(ApprovalRequest),

    /// Open the feedback note entry overlay after the user selects a category.
    OpenFeedbackNote {
        category: FeedbackCategory,
        include_logs: bool,
    },

    /// Open the upload consent popup for feedback after selecting a category.
    OpenFeedbackConsent {
        category: FeedbackCategory,
    },

    /// Begin inline edit mode for a queued user message.
    QueueStartEdit {
        id: u64,
    },

    /// Delete a queued user message.
    QueueDelete {
        id: u64,
    },

    /// Move a queued user message earlier in the queue.
    QueueMoveUp {
        id: u64,
    },

    /// Move a queued user message later in the queue.
    QueueMoveDown {
        id: u64,
    },

    /// Move a queued user message to the front (next to send).
    QueueMoveToFront {
        id: u64,
    },

    /// Open the per-item model override picker for a queued message.
    QueueOpenModelPicker {
        id: u64,
    },

    /// Open the per-item thinking override picker for a queued message.
    QueueOpenThinkingPicker {
        id: u64,
    },

    /// Set (or clear) the per-item model override for a queued message.
    QueueSetModelOverride {
        id: u64,
        model: Option<String>,
    },

    /// Set (or clear) the per-item thinking override for a queued message.
    QueueSetThinkingOverride {
        id: u64,
        effort: Option<Option<ReasoningEffort>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FeedbackCategory {
    BadResult,
    GoodResult,
    Bug,
    Other,
}
