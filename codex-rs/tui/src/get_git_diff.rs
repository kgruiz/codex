//! Utility to compute the current Git diff for the working directory.
//!
//! The implementation mirrors the behaviour of the TypeScript version in
//! `codex-cli`: it returns the diff for tracked changes as well as any
//! untracked files. When the current directory is not inside a Git
//! repository, the function returns `Ok(GitDiffResult::NotGitRepo)`.

use codex_ansi_escape::ansi_escape_line;
use ratatui::text::Line as RtLine;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

use crate::diff_semantic::SemanticDiffInput;
use crate::diff_semantic::difftastic_available;
use crate::diff_semantic::difftastic_lines;

#[derive(Debug)]
pub(crate) enum GitDiffResult {
    NotGitRepo,
    Error(String),
    Views(GitDiffViews),
}

#[derive(Debug)]
pub(crate) struct GitDiffViews {
    pub line: Vec<RtLine<'static>>,
    pub inline: Vec<RtLine<'static>>,
    pub semantic: Vec<RtLine<'static>>,
}

pub(crate) async fn get_git_diff(width: usize) -> io::Result<GitDiffResult> {
    // First check if we are inside a Git repository.
    if !inside_git_repo().await? {
        return Ok(GitDiffResult::NotGitRepo);
    }

    // Run tracked diff and untracked file listing in parallel.
    let (tracked_diff_res, tracked_inline_res, untracked_output_res) = tokio::join!(
        run_git_capture_diff(&["diff", "--color"]),
        run_git_capture_diff(&["diff", "--color", "--word-diff"]),
        run_git_capture_stdout(&["ls-files", "--others", "--exclude-standard"]),
    );
    let tracked_diff = tracked_diff_res?;
    let tracked_inline = tracked_inline_res?;
    let untracked_output = untracked_output_res?;

    let null_device: &Path = if cfg!(windows) {
        Path::new("NUL")
    } else {
        Path::new("/dev/null")
    };

    let null_path = null_device.to_str().unwrap_or("/dev/null").to_string();
    let mut untracked_line = String::new();
    let mut untracked_inline = String::new();
    let mut join_set: tokio::task::JoinSet<io::Result<(String, String)>> =
        tokio::task::JoinSet::new();
    for file in untracked_output
        .split('\n')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let null_path = null_path.clone();
        let file = file.to_string();
        join_set.spawn(async move {
            let line =
                run_git_capture_diff(&["diff", "--color", "--no-index", "--", &null_path, &file])
                    .await?;
            let inline = run_git_capture_diff(&[
                "diff",
                "--color",
                "--word-diff",
                "--no-index",
                "--",
                &null_path,
                &file,
            ])
            .await?;
            Ok((line, inline))
        });
    }
    while let Some(res) = join_set.join_next().await {
        match res {
            Ok(Ok((line, inline))) => {
                untracked_line.push_str(&line);
                untracked_inline.push_str(&inline);
            }
            Ok(Err(err)) if err.kind() == io::ErrorKind::NotFound => {}
            Ok(Err(err)) => return Err(err),
            Err(_) => {}
        }
    }

    let line_diff = format!("{tracked_diff}{untracked_line}");
    let inline_diff = format!("{tracked_inline}{untracked_inline}");

    let line_lines = line_diff.lines().map(ansi_escape_line).collect();
    let inline_lines = inline_diff.lines().map(ansi_escape_line).collect();

    let semantic_lines = if difftastic_available() {
        let SemanticInputs { inputs, warnings } = semantic_inputs_for_git_diff().await?;
        difftastic_lines(&inputs, width, &warnings)
    } else {
        difftastic_lines(&[], width, &[])
    };

    Ok(GitDiffResult::Views(GitDiffViews {
        line: line_lines,
        inline: inline_lines,
        semantic: semantic_lines,
    }))
}

struct SemanticInputs {
    inputs: Vec<SemanticDiffInput>,
    warnings: Vec<String>,
}

async fn semantic_inputs_for_git_diff() -> io::Result<SemanticInputs> {
    let repo_root = git_repo_root().await?;
    let status_output = run_git_capture_stdout(&["diff", "--name-status", "-z", "-M"]).await?;
    let mut inputs = Vec::new();
    let mut warnings = Vec::new();

    let mut parts = status_output.split('\0').filter(|s| !s.is_empty());
    while let Some(status) = parts.next() {
        let status_char = status.chars().next().unwrap_or('\0');
        match status_char {
            'R' | 'C' => {
                let Some(old_path) = parts.next() else {
                    break;
                };
                let Some(new_path) = parts.next() else {
                    break;
                };
                if let Some(input) = build_semantic_input(
                    &repo_root,
                    status_char,
                    old_path,
                    Some(new_path),
                    &mut warnings,
                )
                .await?
                {
                    inputs.push(input);
                }
            }
            'A' | 'D' | 'M' => {
                let Some(path) = parts.next() else {
                    break;
                };
                if let Some(input) =
                    build_semantic_input(&repo_root, status_char, path, None, &mut warnings).await?
                {
                    inputs.push(input);
                }
            }
            _ => {}
        }
    }

    let untracked_output =
        run_git_capture_stdout(&["ls-files", "--others", "--exclude-standard", "-z"]).await?;
    for path in untracked_output.split('\0').filter(|s| !s.is_empty()) {
        if let Some(input) =
            build_semantic_input(&repo_root, 'A', path, None, &mut warnings).await?
        {
            inputs.push(input);
        }
    }

    Ok(SemanticInputs { inputs, warnings })
}

async fn build_semantic_input(
    repo_root: &Path,
    status: char,
    path: &str,
    renamed_to: Option<&str>,
    warnings: &mut Vec<String>,
) -> io::Result<Option<SemanticDiffInput>> {
    let display_path = if let Some(new_path) = renamed_to {
        format!("{path} -> {new_path}")
    } else {
        path.to_string()
    };

    let old_bytes = if matches!(status, 'A') {
        Vec::new()
    } else {
        match run_git_capture_bytes(&["show", &format!(":{path}")]).await {
            Ok(bytes) => bytes,
            Err(err) => {
                warnings.push(format!("Skipping {display_path}: {err}"));
                return Ok(None);
            }
        }
    };
    let new_bytes = if matches!(status, 'D') {
        Vec::new()
    } else {
        match tokio::fs::read(repo_root.join(renamed_to.unwrap_or(path))).await {
            Ok(bytes) => bytes,
            Err(err) => {
                warnings.push(format!("Skipping {display_path}: {err}"));
                return Ok(None);
            }
        }
    };

    if is_binary(&old_bytes) || is_binary(&new_bytes) {
        warnings.push(format!("Skipping {display_path}: binary file"));
        return Ok(None);
    }

    Ok(Some(SemanticDiffInput {
        display_path,
        old: old_bytes,
        new: new_bytes,
    }))
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0)
}

async fn git_repo_root() -> io::Result<PathBuf> {
    let output = run_git_capture_stdout(&["rev-parse", "--show-toplevel"]).await?;
    Ok(PathBuf::from(output.trim()))
}

/// Helper that executes `git` with the given `args` and returns `stdout` as a
/// UTF-8 string. Any non-zero exit status is considered an *error*.
async fn run_git_capture_stdout(args: &[&str]) -> io::Result<String> {
    let output = Command::new("git")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(io::Error::other(format!(
            "git {args:?} failed with status {status}",
            status = output.status
        )))
    }
}

async fn run_git_capture_bytes(args: &[&str]) -> io::Result<Vec<u8>> {
    let output = Command::new("git")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await?;

    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(io::Error::other(format!(
            "git {args:?} failed with status {status}",
            status = output.status
        )))
    }
}

/// Like [`run_git_capture_stdout`] but treats exit status 1 as success and
/// returns stdout. Git returns 1 for diffs when differences are present.
async fn run_git_capture_diff(args: &[&str]) -> io::Result<String> {
    let output = Command::new("git")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await?;

    if output.status.success() || output.status.code() == Some(1) {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(io::Error::other(format!(
            "git {args:?} failed with status {status}",
            status = output.status
        )))
    }
}

/// Determine if the current directory is inside a Git repository.
async fn inside_git_repo() -> io::Result<bool> {
    let status = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    match status {
        Ok(s) if s.success() => Ok(true),
        Ok(_) => Ok(false),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false), // git not installed
        Err(e) => Err(e),
    }
}
