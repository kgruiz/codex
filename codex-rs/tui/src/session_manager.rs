use std::path::Path;
use std::path::PathBuf;

use chrono::DateTime;
use chrono::Utc;
use codex_core::ConversationItem;
use codex_core::Cursor;
use codex_core::INTERACTIVE_SESSION_SOURCES;
use codex_core::RolloutRecorder;
use codex_core::path_utils;
use codex_protocol::items::TurnItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::SessionMetaLine;

const PAGE_SIZE: usize = 100;

#[derive(Clone, Debug)]
pub(crate) struct SessionManagerEntry {
    pub(crate) path: PathBuf,
    pub(crate) title: Option<String>,
    pub(crate) preview: String,
    pub(crate) created_at: Option<DateTime<Utc>>,
    pub(crate) updated_at: Option<DateTime<Utc>>,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) git_branch: Option<String>,
    pub(crate) is_current: bool,
}

impl SessionManagerEntry {
    pub(crate) fn display_title(&self) -> &str {
        self.title.as_deref().unwrap_or(self.preview.as_str())
    }
}

pub(crate) async fn load_session_entries(
    codex_home: &Path,
    default_provider: &str,
    current_session_path: Option<&Path>,
) -> std::io::Result<Vec<SessionManagerEntry>> {
    let provider_filter = vec![default_provider.to_string()];
    let mut cursor: Option<Cursor> = None;
    let mut entries: Vec<SessionManagerEntry> = Vec::new();

    loop {
        let page = RolloutRecorder::list_conversations(
            codex_home,
            PAGE_SIZE,
            cursor.as_ref(),
            INTERACTIVE_SESSION_SOURCES,
            Some(provider_filter.as_slice()),
            default_provider,
        )
        .await?;

        for item in page.items {
            let mut entry = session_entry_from_item(&item);
            if let Some(current_path) = current_session_path
                && paths_match(&entry.path, current_path)
            {
                entry.is_current = true;
            }
            entries.push(entry);
        }

        if page.next_cursor.is_none() {
            break;
        }

        cursor = page.next_cursor;
        if page.reached_scan_cap {
            break;
        }
    }

    Ok(entries)
}

pub(crate) fn paths_match(a: &Path, b: &Path) -> bool {
    if let (Ok(ca), Ok(cb)) = (
        path_utils::normalize_for_path_comparison(a),
        path_utils::normalize_for_path_comparison(b),
    ) {
        return ca == cb;
    }
    a == b
}

fn session_entry_from_item(item: &ConversationItem) -> SessionManagerEntry {
    let created_at = item
        .created_at
        .as_deref()
        .and_then(parse_timestamp_str)
        .or_else(|| item.head.first().and_then(extract_timestamp));
    let updated_at = item
        .updated_at
        .as_deref()
        .and_then(parse_timestamp_str)
        .or(created_at);

    let meta_summary = extract_session_meta_from_head(&item.head);
    let title = meta_summary
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let preview = preview_from_head(&item.head)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| String::from("(no message yet)"));

    SessionManagerEntry {
        path: item.path.clone(),
        title,
        preview,
        created_at,
        updated_at,
        cwd: meta_summary.cwd,
        git_branch: meta_summary.git_branch,
        is_current: false,
    }
}

struct SessionMetaSummary {
    cwd: Option<PathBuf>,
    git_branch: Option<String>,
    title: Option<String>,
}

fn extract_session_meta_from_head(head: &[serde_json::Value]) -> SessionMetaSummary {
    for value in head {
        if let Ok(meta_line) = serde_json::from_value::<SessionMetaLine>(value.clone()) {
            let cwd = Some(meta_line.meta.cwd);
            let git_branch = meta_line.git.and_then(|git| git.branch);
            let title = meta_line.meta.title;
            return SessionMetaSummary {
                cwd,
                git_branch,
                title,
            };
        }
    }
    SessionMetaSummary {
        cwd: None,
        git_branch: None,
        title: None,
    }
}

fn parse_timestamp_str(ts: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

fn extract_timestamp(value: &serde_json::Value) -> Option<DateTime<Utc>> {
    value
        .get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn preview_from_head(head: &[serde_json::Value]) -> Option<String> {
    head.iter()
        .filter_map(|value| serde_json::from_value::<ResponseItem>(value.clone()).ok())
        .find_map(|item| match codex_core::parse_turn_item(&item) {
            Some(TurnItem::UserMessage(user)) => Some(user.message()),
            _ => None,
        })
}
