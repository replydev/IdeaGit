//! Port of `plugins/git4idea/backend/src/index/GitIndexStatusUtil.kt`:
//! runs `git status --porcelain -z` (format v1) and parses its output.
//!
//! `git status` porcelain v1 `XY PATH` / `XY PATH\0ORIG_PATH` codes:
//! ` `=unmodified, M=modified, A=added, D=deleted, R=renamed, C=copied, T=type changed,
//! U=unmerged, ?=untracked, !=ignored. See the upstream file for the full table.
//!
//! Not ported: splitting very long path lists into chunks (`GitUtil.createHandlerWithPaths`).

use std::path::{Path, PathBuf};

use crate::command::GitCommand;
use crate::error::{GitError, Result};
use crate::runner::Git;
use crate::version::{GitVersion, GitVersionSpecialty};

pub type StatusCode = char;

/// Subset of IntelliJ's `FileStatus` produced by git status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    NotChanged,
    Modified,
    Added,
    Deleted,
    MergedWithConflicts,
    Ignored,
    /// Untracked.
    Unknown,
}

/// Upstream `LightFileStatus.StatusRecord`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRecord {
    pub index: StatusCode,
    pub work_tree: StatusCode,
    pub path: String,
    pub orig_path: Option<String>,
}

impl StatusRecord {
    pub fn file_status(&self) -> FileStatus {
        if self.is_conflicted() {
            return FileStatus::MergedWithConflicts;
        }
        status_code_to_file_status(self.index)
            .or_else(|| status_code_to_file_status(self.work_tree))
            .unwrap_or(FileStatus::NotChanged)
    }

    pub fn is_conflicted(&self) -> bool {
        is_conflicted(self.index, self.work_tree)
    }

    pub fn is_tracked(&self) -> bool {
        !is_ignored(self.index) && !is_untracked(self.index)
    }

    /// Path of the file in HEAD, or `None` if the file is not in the repository yet.
    pub fn repository_path(&self) -> Option<&str> {
        if !self.is_tracked() || self.index == 'A' || self.work_tree == 'A' {
            None
        } else {
            Some(self.orig_path.as_deref().unwrap_or(&self.path))
        }
    }
}

/// Upstream `getFileStatus(StatusCode)`; `None` for unmodified (` `) and unrecognized codes.
pub fn status_code_to_file_status(status: StatusCode) -> Option<FileStatus> {
    match status {
        'M' | 'R' | 'C' | 'T' => Some(FileStatus::Modified),
        'A' => Some(FileStatus::Added),
        'D' => Some(FileStatus::Deleted),
        'U' => Some(FileStatus::MergedWithConflicts),
        '!' => Some(FileStatus::Ignored),
        '?' => Some(FileStatus::Unknown),
        _ => None,
    }
}

pub fn is_ignored(status: StatusCode) -> bool {
    status == '!'
}
pub fn is_untracked(status: StatusCode) -> bool {
    status == '?'
}
pub fn is_renamed(status: StatusCode) -> bool {
    status == 'R' || status == 'C'
}
pub fn is_added(status: StatusCode) -> bool {
    status == 'A'
}
pub fn is_intended_to_be_added(index: StatusCode, work_tree: StatusCode) -> bool {
    index == ' ' && work_tree == 'A'
}
pub fn is_deleted(status: StatusCode) -> bool {
    status == 'D'
}
pub fn is_conflicted(index: StatusCode, work_tree: StatusCode) -> bool {
    (index == 'D' && work_tree == 'D')
        || (index == 'A' && work_tree == 'A')
        || (index == 'T' && work_tree == 'T')
        || (index == 'U' || work_tree == 'U')
}

fn is_known_status(status: char) -> bool {
    matches!(
        status,
        ' ' | 'M' | 'A' | 'D' | 'C' | 'R' | 'U' | 'T' | '!' | '?'
    )
}

fn parse_error(message: &str, output: &str, line: &str, x: char, y: char) -> GitError {
    GitError::StatusParse {
        message: format!("{message}\noutput:\n{output}"),
        x,
        y,
        line: line.to_string(),
    }
}

/// Upstream `parseGitStatusOutput`.
pub fn parse_git_status_output(output: &str) -> Result<Vec<StatusRecord>> {
    let mut result = Vec::new();
    let mut it = output.split('\0');
    while let Some(line) = it.next() {
        if line.trim().is_empty() {
            continue; // e.g. the whole output is empty on a clean working tree
        }
        if line.starts_with("starting fsmonitor-daemon in ") {
            continue; // debug output from git-for-windows 2.33
        }
        if line.chars().count() < 4 || line.as_bytes().get(2) != Some(&b' ') {
            return Err(parse_error("The line is too short", output, line, '0', '0'));
        }
        let mut chars = line.chars();
        let x = chars.next().expect("length checked");
        let y = chars.next().expect("length checked");
        if !is_known_status(x) || !is_known_status(y) {
            return Err(parse_error(
                "Unexpected symbol as status",
                output,
                line,
                x,
                y,
            ));
        }
        let path = line[3..].to_string();
        if is_renamed(x) || is_renamed(y) {
            let orig = it
                .next()
                .ok_or_else(|| parse_error("Missing the original path", output, line, x, y))?;
            result.push(StatusRecord {
                index: x,
                work_tree: y,
                path,
                orig_path: Some(orig.to_string()),
            });
        } else {
            result.push(StatusRecord {
                index: x,
                work_tree: y,
                path,
                orig_path: None,
            });
        }
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy)]
pub struct StatusOptions {
    pub with_renames: bool,
    pub with_untracked: bool,
    pub with_ignored: bool,
    pub expand_ignored_directories: bool,
}

impl Default for StatusOptions {
    /// Upstream `appendParameters` defaults.
    fn default() -> Self {
        Self {
            with_renames: true,
            with_untracked: true,
            with_ignored: false,
            expand_ignored_directories: false,
        }
    }
}

/// Upstream `GitLineHandler.appendParameters` for `git status`.
pub fn status_parameters(version: &GitVersion, opts: StatusOptions) -> Vec<String> {
    let mut p = vec!["--porcelain".to_string(), "-z".to_string()];
    if !opts.with_renames && GitVersionSpecialty::StatusSupportsNoRenames.exists_in(version) {
        p.push("--no-renames".into());
    }
    p.push(format!(
        "--untracked-files={}",
        if opts.with_untracked { "all" } else { "no" }
    ));
    if opts.with_ignored && opts.expand_ignored_directories {
        p.push("--ignored".into());
    } else if GitVersionSpecialty::StatusSupportsIgnoredModes.exists_in(version) {
        p.push(
            if opts.with_ignored {
                "--ignored=matching"
            } else {
                "--ignored=no"
            }
            .into(),
        );
    } else if opts.with_ignored {
        p.push("--ignored".into());
    }
    p
}

/// Upstream `getFileStatus(project, root, files, ...)`. An empty `files` list means the whole root.
pub fn get_file_status(
    git: &Git,
    root: &Path,
    files: &[PathBuf],
    opts: StatusOptions,
) -> Result<Vec<StatusRecord>> {
    let mut h = git.handler(root, GitCommand::STATUS);
    h.set_silent(true);
    h.add_parameters(status_parameters(&git.version(), opts));
    if !files.is_empty() {
        h.end_options();
        h.add_relative_paths(files);
    }
    let output = git.run_command(h).output_or_error()?;
    parse_git_status_output(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(index: char, work_tree: char, path: &str, orig: Option<&str>) -> StatusRecord {
        StatusRecord {
            index,
            work_tree,
            path: path.into(),
            orig_path: orig.map(Into::into),
        }
    }

    #[test]
    fn parses_records_renames_and_skips_noise() {
        let out = " M a.txt\0R  new.txt\0old.txt\0?? dir/u.txt\0\0starting fsmonitor-daemon in x\0UU c.txt\0";
        let records = parse_git_status_output(out).unwrap();
        assert_eq!(
            records,
            vec![
                rec(' ', 'M', "a.txt", None),
                rec('R', ' ', "new.txt", Some("old.txt")),
                rec('?', '?', "dir/u.txt", None),
                rec('U', 'U', "c.txt", None),
            ]
        );
    }

    #[test]
    fn empty_output_is_clean() {
        assert!(parse_git_status_output("").unwrap().is_empty());
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(parse_git_status_output("M").is_err());
        assert!(parse_git_status_output("MMXa.txt").is_err());
        assert!(parse_git_status_output("ZZ a.txt").is_err());
        assert!(
            parse_git_status_output("R  new.txt").is_err(),
            "rename without orig path"
        );
    }

    #[test]
    fn file_status_mapping() {
        assert_eq!(rec(' ', 'M', "a", None).file_status(), FileStatus::Modified);
        assert_eq!(rec('A', ' ', "a", None).file_status(), FileStatus::Added);
        assert_eq!(
            rec('A', 'A', "a", None).file_status(),
            FileStatus::MergedWithConflicts
        );
        assert_eq!(
            rec('D', 'D', "a", None).file_status(),
            FileStatus::MergedWithConflicts
        );
        assert_eq!(rec('?', '?', "a", None).file_status(), FileStatus::Unknown);
        assert_eq!(rec('!', '!', "a", None).file_status(), FileStatus::Ignored);
        assert_eq!(rec('R', 'M', "n", Some("o")).repository_path(), Some("o"));
        assert_eq!(rec('A', ' ', "n", None).repository_path(), None);
        assert_eq!(rec('?', '?', "n", None).repository_path(), None);
    }

    #[test]
    fn parameters_by_version() {
        let modern = GitVersion::new(2, 40, 0, 0);
        assert_eq!(
            status_parameters(&modern, StatusOptions::default()),
            ["--porcelain", "-z", "--untracked-files=all", "--ignored=no"]
        );
        let opts = StatusOptions {
            with_renames: false,
            with_ignored: true,
            ..Default::default()
        };
        assert_eq!(
            status_parameters(&modern, opts),
            [
                "--porcelain",
                "-z",
                "--no-renames",
                "--untracked-files=all",
                "--ignored=matching"
            ]
        );
        let old = GitVersion::new(2, 15, 0, 0);
        assert_eq!(
            status_parameters(&old, opts),
            ["--porcelain", "-z", "--untracked-files=all", "--ignored"]
        );
    }
}
