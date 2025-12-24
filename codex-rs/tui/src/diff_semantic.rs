use codex_ansi_escape::ansi_escape_line;
use ratatui::style::Stylize;
use ratatui::text::Line;
use std::io;
use std::io::Write as _;
use std::process::Command;
use std::process::Stdio;
use std::sync::OnceLock;
use tempfile::NamedTempFile;

pub(crate) struct SemanticDiffInput {
    pub display_path: String,
    pub old: Vec<u8>,
    pub new: Vec<u8>,
}

pub(crate) fn difftastic_available() -> bool {
    if cfg!(test) {
        return false;
    }
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("difft")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

pub(crate) fn difftastic_lines(
    inputs: &[SemanticDiffInput],
    width: usize,
    warnings: &[String],
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !difftastic_available() {
        lines.push(Line::from(
            "difftastic not found on PATH (install with `cargo install difftastic`)".dim(),
        ));
        append_warnings(&mut lines, warnings);
        return lines;
    }
    if inputs.is_empty() {
        lines.push(Line::from("(no files to diff)".dim().italic()));
        append_warnings(&mut lines, warnings);
        return lines;
    }

    match run_difftastic(inputs, width) {
        Ok(output) => {
            if output.trim().is_empty() {
                lines.push(Line::from("(no diff output)".dim().italic()));
            } else {
                lines.extend(output.lines().map(ansi_escape_line));
            }
        }
        Err(err) => {
            lines.push(Line::from(format!("Failed to run difftastic: {err}").dim()));
        }
    }

    append_warnings(&mut lines, warnings);
    lines
}

fn append_warnings(lines: &mut Vec<Line<'static>>, warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Warnings:".dim().bold()));
    lines.extend(
        warnings
            .iter()
            .map(|warning| Line::from(warning.clone().dim())),
    );
}

fn run_difftastic(inputs: &[SemanticDiffInput], width: usize) -> io::Result<String> {
    let mut output = String::new();
    let width = width.max(20);
    let width_arg = width.to_string();

    for input in inputs {
        let mut old_file = NamedTempFile::new()?;
        old_file.write_all(&input.old)?;
        let mut new_file = NamedTempFile::new()?;
        new_file.write_all(&input.new)?;
        let old_path = old_file.path().to_str().unwrap_or("");
        let new_path = new_file.path().to_str().unwrap_or("");

        let result = Command::new("difft")
            .args([
                "--display",
                "inline",
                "--color",
                "always",
                "--width",
                width_arg.as_str(),
                "--syntax-highlight",
                "off",
                &input.display_path,
                old_path,
                "0000000",
                "100644",
                new_path,
                "0000000",
                "100644",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;

        if !(result.status.success() || result.status.code() == Some(1)) {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(io::Error::other(format!(
                "difftastic exited with status {status}: {stderr}",
                status = result.status
            )));
        }
        let stdout = String::from_utf8_lossy(&result.stdout);
        output.push_str(&stdout);
        if !stdout.ends_with('\n') {
            output.push('\n');
        }
    }

    Ok(output)
}
