use serde::Serialize;
use std::io;
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;
use tracing::error;
use tracing::warn;
use wildmatch::WildMatchPattern;

use crate::config::types::NotificationFocusConfig;
use crate::env::is_wsl;

const BUILTIN_TITLE: &str = "Codex";
const MAX_COMPLETION_PREVIEW_CHARS: usize = 200;
const MAX_APPROVAL_DETAIL_CHARS: usize = 60;
const FOCUS_OVERRIDE_USE_CONFIG: u8 = 0;
const FOCUS_OVERRIDE_ENABLED: u8 = 1;
const FOCUS_OVERRIDE_DISABLED: u8 = 2;

static NOTIFY_SEND_MISSING_WARNED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Default)]
struct NotificationFocusFilter {
    process_name_whitelist: Vec<WildMatchPattern<'*', '?'>>,
    process_name_blacklist: Vec<WildMatchPattern<'*', '?'>>,
    app_name_whitelist: Vec<WildMatchPattern<'*', '?'>>,
    app_name_blacklist: Vec<WildMatchPattern<'*', '?'>>,
    bundle_id_whitelist: Vec<WildMatchPattern<'*', '?'>>,
    bundle_id_blacklist: Vec<WildMatchPattern<'*', '?'>>,
}

#[derive(Debug, Default, Clone)]
struct FocusedAppInfo {
    process_name: Option<String>,
    app_name: Option<String>,
    bundle_id: Option<String>,
}

impl FocusedAppInfo {
    fn is_empty(&self) -> bool {
        self.process_name
            .as_ref()
            .is_none_or(|name| name.trim().is_empty())
            && self
                .app_name
                .as_ref()
                .is_none_or(|name| name.trim().is_empty())
            && self
                .bundle_id
                .as_ref()
                .is_none_or(|bundle_id| bundle_id.trim().is_empty())
    }
}

impl NotificationFocusFilter {
    fn from_config(config: &NotificationFocusConfig) -> Self {
        Self {
            process_name_whitelist: Self::compile_patterns(&config.process_name_whitelist),
            process_name_blacklist: Self::compile_patterns(&config.process_name_blacklist),
            app_name_whitelist: Self::compile_patterns(&config.app_name_whitelist),
            app_name_blacklist: Self::compile_patterns(&config.app_name_blacklist),
            bundle_id_whitelist: Self::compile_patterns(&config.bundle_id_whitelist),
            bundle_id_blacklist: Self::compile_patterns(&config.bundle_id_blacklist),
        }
    }

    fn is_configured(&self) -> bool {
        !self.process_name_whitelist.is_empty()
            || !self.process_name_blacklist.is_empty()
            || !self.app_name_whitelist.is_empty()
            || !self.app_name_blacklist.is_empty()
            || !self.bundle_id_whitelist.is_empty()
            || !self.bundle_id_blacklist.is_empty()
    }

    fn allows(
        &self,
        focused_process_name: Option<&str>,
        focused_app_name: Option<&str>,
        focused_bundle_id: Option<&str>,
    ) -> bool {
        let focused_process_name = focused_process_name
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let focused_app_name = focused_app_name
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let focused_bundle_id = focused_bundle_id
            .map(str::trim)
            .filter(|bundle_id| !bundle_id.is_empty());
        if focused_process_name.is_none()
            && focused_app_name.is_none()
            && focused_bundle_id.is_none()
        {
            return true;
        }
        if let Some(name) = focused_process_name
            && self.matches_any(&self.process_name_blacklist, name)
        {
            return false;
        }
        if let Some(name) = focused_app_name
            && self.matches_any(&self.app_name_blacklist, name)
        {
            return false;
        }
        if let Some(bundle_id) = focused_bundle_id
            && self.matches_any(&self.bundle_id_blacklist, bundle_id)
        {
            return false;
        }
        let process_name_whitelist_active =
            focused_process_name.is_some() && !self.process_name_whitelist.is_empty();
        let app_name_whitelist_active =
            focused_app_name.is_some() && !self.app_name_whitelist.is_empty();
        let bundle_whitelist_active =
            focused_bundle_id.is_some() && !self.bundle_id_whitelist.is_empty();
        if !process_name_whitelist_active && !app_name_whitelist_active && !bundle_whitelist_active
        {
            return true;
        }
        let process_name_allowed = focused_process_name.is_some_and(|name| {
            process_name_whitelist_active && self.matches_any(&self.process_name_whitelist, name)
        });
        let app_name_allowed = focused_app_name.is_some_and(|name| {
            app_name_whitelist_active && self.matches_any(&self.app_name_whitelist, name)
        });
        let bundle_allowed = focused_bundle_id.is_some_and(|bundle_id| {
            bundle_whitelist_active && self.matches_any(&self.bundle_id_whitelist, bundle_id)
        });
        process_name_allowed || app_name_allowed || bundle_allowed
    }

    fn matches_any(&self, patterns: &[WildMatchPattern<'*', '?'>], candidate: &str) -> bool {
        patterns.iter().any(|pattern| pattern.matches(candidate))
    }

    fn compile_patterns(patterns: &[String]) -> Vec<WildMatchPattern<'*', '?'>> {
        patterns
            .iter()
            .filter_map(|pattern| {
                let trimmed = pattern.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(WildMatchPattern::new_case_insensitive(trimmed))
                }
            })
            .collect()
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct UserNotifier {
    approval_command: Option<Vec<String>>,
    completion_command: Option<Vec<String>>,
    approval_notify: bool,
    completion_notify: bool,
    focus_filter: NotificationFocusFilter,
    focus_filter_override: Arc<AtomicU8>,
}

impl UserNotifier {
    pub(crate) fn notify(&self, notification: &UserNotification) {
        if !self.has_any_target(notification) || !self.should_send_notification() {
            return;
        }
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
        focus_config: NotificationFocusConfig,
    ) -> Self {
        Self {
            approval_command,
            completion_command,
            approval_notify,
            completion_notify,
            focus_filter: NotificationFocusFilter::from_config(&focus_config),
            focus_filter_override: Arc::new(AtomicU8::new(FOCUS_OVERRIDE_USE_CONFIG)),
        }
    }

    pub(crate) fn set_focus_filter_override(&self, enabled: Option<bool>) {
        let value = match enabled {
            Some(true) => FOCUS_OVERRIDE_ENABLED,
            Some(false) => FOCUS_OVERRIDE_DISABLED,
            None => FOCUS_OVERRIDE_USE_CONFIG,
        };
        self.focus_filter_override.store(value, Ordering::SeqCst);
    }

    fn notify_builtin(&self, notification: &UserNotification) {
        let Some(message) = notification.builtin_message() else {
            return;
        };
        if let Err(err) = notify_builtin(&message) {
            warn!("failed to send built-in notification: {err}");
        }
    }

    fn should_send_notification(&self) -> bool {
        if !self.focus_filter_active() {
            return true;
        }
        let focused_app = focused_app_info();
        if focused_app.is_empty() {
            return true;
        };
        self.focus_filter.allows(
            focused_app.process_name.as_deref(),
            focused_app.app_name.as_deref(),
            focused_app.bundle_id.as_deref(),
        )
    }

    fn focus_filter_active(&self) -> bool {
        let configured = self.focus_filter.is_configured();
        match self.focus_filter_override.load(Ordering::SeqCst) {
            FOCUS_OVERRIDE_ENABLED => configured,
            FOCUS_OVERRIDE_DISABLED => false,
            _ => configured,
        }
    }

    fn has_any_target(&self, notification: &UserNotification) -> bool {
        match notification {
            UserNotification::AgentTurnComplete { .. } => {
                self.completion_notify || self.completion_command.is_some()
            }
            UserNotification::ApprovalRequested { .. } => {
                self.approval_notify || self.approval_command.is_some()
            }
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

fn focused_app_info() -> FocusedAppInfo {
    #[cfg(target_os = "macos")]
    {
        let bundle_id = focused_app_bundle_id_macos();
        let app_name = bundle_id
            .as_deref()
            .and_then(focused_app_display_name_macos);
        FocusedAppInfo {
            process_name: focused_app_process_name_macos(),
            app_name,
            bundle_id,
        }
    }
    #[cfg(target_os = "windows")]
    {
        FocusedAppInfo {
            process_name: focused_app_process_name_windows(),
            app_name: None,
            bundle_id: None,
        }
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        #[cfg(target_os = "linux")]
        {
            let name = if is_wsl() && std::env::var_os("WT_SESSION").is_some() {
                focused_app_process_name_windows()
            } else {
                focused_app_process_name_linux()
            };
            FocusedAppInfo {
                process_name: name,
                app_name: None,
                bundle_id: None,
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            FocusedAppInfo::default()
        }
    }
}

#[cfg(target_os = "macos")]
fn focused_app_process_name_macos() -> Option<String> {
    let script = r#"tell application "System Events" to get name of first application process whose frontmost is true"#;
    command_output_trimmed("osascript", &["-e", script])
        .ok()
        .flatten()
}

#[cfg(target_os = "macos")]
fn focused_app_bundle_id_macos() -> Option<String> {
    let script = r#"tell application "System Events" to get bundle identifier of first application process whose frontmost is true"#;
    command_output_trimmed("osascript", &["-e", script])
        .ok()
        .flatten()
}

#[cfg(target_os = "linux")]
fn focused_app_process_name_linux() -> Option<String> {
    if let Ok(Some(class_name)) =
        command_output_trimmed("xdotool", &["getwindowfocus", "getwindowclassname"])
    {
        return Some(class_name);
    }
    let Ok(Some(active_window)) = command_output_trimmed("xprop", &["-root", "_NET_ACTIVE_WINDOW"])
    else {
        return None;
    };
    let window_id = active_window
        .split_whitespace()
        .find_map(|token| token.strip_prefix("0x"))
        .map(|hex| format!("0x{hex}"))?;
    let wm_class = command_output_trimmed("xprop", &["-id", &window_id, "WM_CLASS"])
        .ok()
        .flatten()?;
    extract_last_quoted_string(&wm_class)
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn focused_app_process_name_windows() -> Option<String> {
    let script = r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class User32 {
    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
}
"@ | Out-Null
$hwnd = [User32]::GetForegroundWindow()
if ($hwnd -eq [IntPtr]::Zero) { exit }
$pid = 0
[User32]::GetWindowThreadProcessId($hwnd, [ref]$pid) | Out-Null
if ($pid -eq 0) { exit }
$proc = Get-Process -Id $pid -ErrorAction SilentlyContinue
if ($null -eq $proc) { exit }
$proc.ProcessName
"#;
    let encoded_command = encode_script_for_powershell(script);
    let mut command = Command::new("powershell.exe");
    command
        .arg("-NoProfile")
        .arg("-NoLogo")
        .arg("-EncodedCommand")
        .arg(encoded_command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    match command.output() {
        Ok(output) => {
            if !output.status.success() {
                return None;
            }
            let text = String::from_utf8_lossy(&output.stdout);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Err(err) => {
            if err.kind() == io::ErrorKind::NotFound {
                return None;
            }
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn focused_app_display_name_macos(bundle_id: &str) -> Option<String> {
    let script = format!("name of application id \"{bundle_id}\"");
    command_output_trimmed("osascript", &["-e", &script])
        .ok()
        .flatten()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn command_output_trimmed(command: &str, args: &[&str]) -> io::Result<Option<String>> {
    let output = Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(output) => {
            if !output.status.success() {
                return Ok(None);
            }
            let text = String::from_utf8_lossy(&output.stdout);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Err(err) => {
            if err.kind() == io::ErrorKind::NotFound {
                return Ok(None);
            }
            Err(err)
        }
    }
}

#[cfg(target_os = "linux")]
fn extract_last_quoted_string(input: &str) -> Option<String> {
    let mut current = String::new();
    let mut in_quotes = false;
    let mut last = None;
    for ch in input.chars() {
        if ch == '"' {
            if in_quotes {
                if !current.is_empty() {
                    last = Some(current.clone());
                }
                current.clear();
                in_quotes = false;
            } else {
                in_quotes = true;
            }
            continue;
        }
        if in_quotes {
            current.push(ch);
        }
    }
    last
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

    #[test]
    fn test_focus_filter_lists() {
        let config = NotificationFocusConfig {
            process_name_whitelist: vec!["Slack".to_string()],
            process_name_blacklist: vec!["Code".to_string()],
            app_name_whitelist: vec!["Visual Studio Code".to_string()],
            app_name_blacklist: vec!["Adobe*".to_string()],
            bundle_id_whitelist: vec!["com.apple.Terminal".to_string()],
            bundle_id_blacklist: vec!["com.microsoft.VSCode".to_string()],
        };
        let filter = NotificationFocusFilter::from_config(&config);
        assert_eq!(filter.allows(Some("Code"), None, None), false);
        assert_eq!(filter.allows(Some("Slack"), None, None), true);
        assert_eq!(filter.allows(Some("Safari"), None, None), false);
        assert_eq!(
            filter.allows(Some("Electron"), Some("Visual Studio Code"), None),
            true
        );
        assert_eq!(
            filter.allows(Some("Slack"), Some("Adobe Photoshop"), None),
            false
        );
        assert_eq!(
            filter.allows(
                Some("Electron"),
                Some("Visual Studio Code"),
                Some("com.microsoft.VSCode")
            ),
            false
        );
        assert_eq!(
            filter.allows(
                Some("Electron"),
                Some("Visual Studio Code"),
                Some("com.apple.Terminal")
            ),
            true
        );
        assert_eq!(filter.allows(None, None, None), true);
    }
}
