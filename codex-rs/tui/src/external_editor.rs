use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

pub(crate) fn edit_text_in_external_editor(
    initial_text: &str,
    cwd: &Path,
) -> Result<String, String> {
    let editor = resolve_editor_command()
        .ok_or_else(|| "Set $VISUAL or $EDITOR to use the external editor.".to_string())?;

    let mut tokens = split_editor_command(&editor);
    if tokens.is_empty() {
        return Err("External editor command is empty.".to_string());
    }

    let program = tokens.remove(0);
    let temp_dir =
        tempdir().map_err(|err| format!("Failed to create temp dir for editor: {err}"))?;
    let path = temp_dir.path().join("codex-prompt.txt");

    fs::write(&path, initial_text)
        .map_err(|err| format!("Failed to write editor temp file: {err}"))?;

    let status = Command::new(&program)
        .args(tokens)
        .arg(&path)
        .current_dir(cwd)
        .status()
        .map_err(|err| format!("Failed to launch editor '{program}': {err}"))?;

    if !status.success() {
        return Err(format!("Editor exited with status {status}."));
    }

    fs::read_to_string(&path).map_err(|err| format!("Failed to read editor temp file: {err}"))
}

fn resolve_editor_command() -> Option<String> {
    let editor = env::var("VISUAL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if editor.is_some() {
        return editor;
    }

    env::var("EDITOR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn split_editor_command(command: &str) -> Vec<String> {
    shlex::split(command).unwrap_or_else(|| {
        command
            .split_whitespace()
            .map(ToString::to_string)
            .collect()
    })
}
