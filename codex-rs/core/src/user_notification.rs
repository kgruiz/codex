use serde::Serialize;
use std::io;
use std::process::Command;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use tracing::error;
use tracing::warn;

use crate::env::is_wsl;

const BUILTIN_TITLE: &str = "Codex";
const MAX_COMPLETION_PREVIEW_CHARS: usize = 200;
const MAX_APPROVAL_DETAIL_CHARS: usize = 60;

static NOTIFY_SEND_MISSING_WARNED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Default, Clone)]
pub(crate) struct UserNotifier {
    approval_command: Option<Vec<String>>,
    completion_command: Option<Vec<String>>,
    approval_notify: bool,
    completion_notify: bool,
}

impl UserNotifier {
    pub(crate) fn notify(&self, notification: &UserNotification) {
        match notification {
            UserNotification::AgentTurnComplete { .. } => {
                if self.completion_notify {
                    self.notify_builtin(notification);
                }
                if let Some(command) = &self.completion_command {
                    self.invoke_command(command, notification);
                }
            }
            UserNotification::ApprovalRequested { .. } => {
                if self.approval_notify {
                    self.notify_builtin(notification);
                }
                if let Some(command) = &self.approval_command {
                    self.invoke_command(command, notification);
                }
            }
        }
    }

    fn invoke_command(&self, notify_command: &[String], notification: &UserNotification) {
        let Ok(json) = serde_json::to_string(&notification) else {
            error!("failed to serialise notification payload");
            return;
        };

        if notify_command.is_empty() {
            return;
        }

        let mut command = Command::new(&notify_command[0]);
        if notify_command.len() > 1 {
            command.args(&notify_command[1..]);
        }
        command.arg(json);

        // Fire-and-forget – we do not wait for completion.
        if let Err(e) = command.spawn() {
            warn!("failed to spawn notifier '{}': {e}", notify_command[0]);
        }
    }

    pub(crate) fn new(
        approval_command: Option<Vec<String>>,
        completion_command: Option<Vec<String>>,
        approval_notify: bool,
        completion_notify: bool,
    ) -> Self {
        Self {
            approval_command,
            completion_command,
            approval_notify,
            completion_notify,
        }
    }

    fn notify_builtin(&self, notification: &UserNotification) {
        let Some(message) = notification.builtin_message() else {
            return;
        };
        if let Err(err) = notify_builtin(&message) {
            warn!("failed to send built-in notification: {err}");
        }
    }
}

/// User can configure a program that will receive notifications. Each
/// notification is serialized as JSON and passed as an argument to the
/// program.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(crate) enum UserNotification {
    #[serde(rename_all = "kebab-case")]
    AgentTurnComplete {
        thread_id: String,
        turn_id: String,
        cwd: String,

        /// Messages that the user sent to the agent to initiate the turn.
        input_messages: Vec<String>,

        /// The last message sent by the assistant in the turn.
        last_assistant_message: Option<String>,
    },

    #[serde(rename_all = "kebab-case")]
    ApprovalRequested {
        #[serde(flatten)]
        approval: ApprovalNotification,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "approval-type", rename_all = "kebab-case")]
pub(crate) enum ApprovalNotification {
    #[serde(rename_all = "kebab-case")]
    Exec {
        thread_id: String,
        turn_id: String,
        cwd: String,
        command: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    #[serde(rename_all = "kebab-case")]
    ApplyPatch {
        thread_id: String,
        turn_id: String,
        cwd: String,
        files: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        grant_root: Option<String>,
    },
    #[serde(rename_all = "kebab-case")]
    McpElicitation {
        server_name: String,
        request_id: String,
        message: String,
    },
}

impl UserNotification {
    fn builtin_message(&self) -> Option<String> {
        match self {
            UserNotification::AgentTurnComplete {
                last_assistant_message,
                ..
            } => {
                let preview = last_assistant_message
                    .as_ref()
                    .and_then(|msg| normalize_preview(msg, MAX_COMPLETION_PREVIEW_CHARS));
                Some(preview.unwrap_or_else(|| "Turn complete".to_string()))
            }
            UserNotification::ApprovalRequested { approval } => {
                let detail = match approval {
                    ApprovalNotification::Exec { command, .. } => {
                        let command_text = command.join(" ");
                        format!(
                            "Approval requested: {}",
                            truncate_text(&command_text, MAX_APPROVAL_DETAIL_CHARS)
                        )
                    }
                    ApprovalNotification::ApplyPatch { files, .. } => {
                        if files.len() == 1 {
                            format!("Approval requested: edit {}", files[0])
                        } else {
                            format!("Approval requested: edit {} files", files.len())
                        }
                    }
                    ApprovalNotification::McpElicitation { server_name, .. } => {
                        format!("Approval requested by {server_name}")
                    }
                };
                Some(detail)
            }
        }
    }
}

fn normalize_preview(input: &str, max_chars: usize) -> Option<String> {
    let normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(truncate_text(trimmed, max_chars))
}

fn truncate_text(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let Some((end, _)) = input.char_indices().nth(max_chars) else {
        return input.to_string();
    };
    if max_chars >= 3 {
        let truncated = input
            .chars()
            .take(max_chars.saturating_sub(3))
            .collect::<String>();
        format!("{truncated}...")
    } else {
        input[..end].to_string()
    }
}

fn notify_builtin(message: &str) -> io::Result<()> {
    if cfg!(target_os = "macos") {
        return notify_macos(message);
    }
    if cfg!(target_os = "windows") {
        return notify_windows(message);
    }
    if is_wsl() && std::env::var_os("WT_SESSION").is_some() {
        return notify_windows(message);
    }
    notify_linux(message)
}

fn notify_macos(message: &str) -> io::Result<()> {
    let escaped = message.replace('"', "\\\"");
    let script = format!("display notification \"{escaped}\" with title \"{BUILTIN_TITLE}\"");
    let mut command = Command::new("osascript");
    command
        .arg("-e")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "osascript exited with status {status}"
        )))
    }
}

fn notify_linux(message: &str) -> io::Result<()> {
    let mut command = Command::new("notify-send");
    command
        .arg(BUILTIN_TITLE)
        .arg(message)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = match command.status() {
        Ok(status) => status,
        Err(err) => {
            if err.kind() == io::ErrorKind::NotFound {
                if !NOTIFY_SEND_MISSING_WARNED.swap(true, Ordering::SeqCst) {
                    warn!("notify-send not found; built-in notifications are disabled");
                }
                return Ok(());
            }
            return Err(err);
        }
    };
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "notify-send exited with status {status}"
        )))
    }
}

fn notify_windows(message: &str) -> io::Result<()> {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;

    let encoded_title = BASE64.encode(escape_for_xml(BUILTIN_TITLE));
    let encoded_body = BASE64.encode(escape_for_xml(message));
    let script = format!(
        r#"
$encoding = [System.Text.Encoding]::UTF8
$titleText = $encoding.GetString([System.Convert]::FromBase64String("{encoded_title}"))
$bodyText = $encoding.GetString([System.Convert]::FromBase64String("{encoded_body}"))
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
$doc = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02)
$textNodes = $doc.GetElementsByTagName("text")
$textNodes.Item(0).AppendChild($doc.CreateTextNode($titleText)) | Out-Null
$textNodes.Item(1).AppendChild($doc.CreateTextNode($bodyText)) | Out-Null
$toast = [Windows.UI.Notifications.ToastNotification]::new($doc)
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Codex').Show($toast)
"#,
    );
    let encoded_command = encode_script_for_powershell(&script);
    let mut command = Command::new("powershell.exe");
    command
        .arg("-NoProfile")
        .arg("-NoLogo")
        .arg("-EncodedCommand")
        .arg(encoded_command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "powershell.exe exited with status {status}"
        )))
    }
}

fn encode_script_for_powershell(script: &str) -> String {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;

    let mut wide: Vec<u8> = Vec::with_capacity((script.len() + 1) * 2);
    for unit in script.encode_utf16() {
        let bytes = unit.to_le_bytes();
        wide.extend_from_slice(&bytes);
    }
    BASE64.encode(wide)
}

fn escape_for_xml(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_user_notification() -> Result<()> {
        let notification = UserNotification::AgentTurnComplete {
            thread_id: "b5f6c1c2-1111-2222-3333-444455556666".to_string(),
            turn_id: "12345".to_string(),
            cwd: "/Users/example/project".to_string(),
            input_messages: vec!["Rename `foo` to `bar` and update the callsites.".to_string()],
            last_assistant_message: Some(
                "Rename complete and verified `cargo build` succeeds.".to_string(),
            ),
        };
        let serialized = serde_json::to_string(&notification)?;
        assert_eq!(
            serialized,
            r#"{"type":"agent-turn-complete","thread-id":"b5f6c1c2-1111-2222-3333-444455556666","turn-id":"12345","cwd":"/Users/example/project","input-messages":["Rename `foo` to `bar` and update the callsites."],"last-assistant-message":"Rename complete and verified `cargo build` succeeds."}"#
        );
        Ok(())
    }

    #[test]
    fn test_user_notification_approval() -> Result<()> {
        let notification = UserNotification::ApprovalRequested {
            approval: ApprovalNotification::Exec {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                cwd: "/Users/example/project".to_string(),
                command: vec!["git".to_string(), "status".to_string()],
                reason: Some("retry outside sandbox".to_string()),
            },
        };
        let serialized = serde_json::to_string(&notification)?;
        assert_eq!(
            serialized,
            r#"{"type":"approval-requested","approval-type":"exec","thread-id":"thread-1","turn-id":"turn-1","cwd":"/Users/example/project","command":["git","status"],"reason":"retry outside sandbox"}"#
        );
        Ok(())
    }
}
