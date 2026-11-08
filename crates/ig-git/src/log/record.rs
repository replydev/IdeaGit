//! One commit record of `git log` output, and the builders that create records.
//!
//! Port of `plugins/git4idea/backend/src/history/GitLogRecord.java`, `GitLogFullRecord.java`,
//! `GitLogRecordBuilder.kt`, `GitChangeType.java` and `GitChangesParser.getChangeType`.
//!
//! Like upstream, accessors return an empty string (or 0) for a value that wasn't requested.
//! Deviations: `commit_time`/`author_time_stamp` return 0 for malformed values (upstream throws); no `setUsedHandler` (it only enriches log messages); a full record keeps its
//! `--name-status` entries but doesn't build `Change` objects (`parseChanges`, out of scope until
//! commit details are ported); path interning (`InternedGitLogRecordBuilder`) is not needed.

use crate::log::parser::{GitFormatError, GitLogOption, OptionValues};

/// The kind of change of a file (`Change.Type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeType {
    Modification,
    New,
    Deleted,
    Moved,
}

/// `GitChangesParser.getChangeType(GitChangeType.fromString(status))`: the type of a
/// `--name-status` status (`M`, `A`, `C..`, `D`, `R..`, `U`, `T`).
pub fn change_type_from_status(status: &str) -> Result<ChangeType, GitFormatError> {
    match status.chars().next() {
        Some('A') => Ok(ChangeType::New),
        Some('T') | Some('M') => Ok(ChangeType::Modification),
        Some('D') => Ok(ChangeType::Deleted),
        Some('C') | Some('R') => Ok(ChangeType::Moved),
        // upstream logs an error for UNRESOLVED and treats it as a modification
        Some('U') => Ok(ChangeType::Modification),
        _ => Err(GitFormatError(format!("Unexpected status [{status}]"))),
    }
}

/// A `--name-status` entry (`VcsFileStatusInfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VcsFileStatusInfo {
    pub change_type: ChangeType,
    pub first_path: String,
    pub second_path: Option<String>,
}

/// Creates records from the parsed options (`GitLogRecordBuilder`).
pub trait GitLogRecordBuilder {
    type Record;

    fn add_path(
        &mut self,
        change_type: ChangeType,
        first_path: String,
        second_path: Option<String>,
    );

    fn build(&mut self, options: OptionValues, supports_raw_body: bool) -> Self::Record;

    fn clear(&mut self);
}

/// Builds [`GitLogRecord`]s (`DefaultGitLogRecordBuilder`).
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultGitLogRecordBuilder;

impl GitLogRecordBuilder for DefaultGitLogRecordBuilder {
    type Record = GitLogRecord;

    /// # Panics
    /// Always: records without `--name-status` have no paths (upstream `UnsupportedOperationException`).
    fn add_path(&mut self, _: ChangeType, _: String, _: Option<String>) {
        panic!("Can not add paths to GitLogRecord");
    }

    fn build(&mut self, options: OptionValues, supports_raw_body: bool) -> GitLogRecord {
        GitLogRecord {
            options,
            supports_raw_body,
        }
    }

    fn clear(&mut self) {}
}

/// Builds [`GitLogFullRecord`]s with their `--name-status` entries (`DefaultGitLogFullRecordBuilder`).
#[derive(Debug, Default, Clone)]
pub struct DefaultGitLogFullRecordBuilder {
    statuses: Vec<VcsFileStatusInfo>,
}

impl GitLogRecordBuilder for DefaultGitLogFullRecordBuilder {
    type Record = GitLogFullRecord;

    fn add_path(
        &mut self,
        change_type: ChangeType,
        first_path: String,
        second_path: Option<String>,
    ) {
        self.statuses.push(VcsFileStatusInfo {
            change_type,
            first_path,
            second_path,
        });
    }

    fn build(&mut self, options: OptionValues, supports_raw_body: bool) -> GitLogFullRecord {
        GitLogFullRecord {
            record: GitLogRecord {
                options,
                supports_raw_body,
            },
            statuses: std::mem::take(&mut self.statuses),
        }
    }

    fn clear(&mut self) {
        self.statuses = Vec::new();
    }
}

/// The information of one commit (`GitLogRecord`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLogRecord {
    options: OptionValues,
    supports_raw_body: bool,
}

impl GitLogRecord {
    pub fn new(options: OptionValues, supports_raw_body: bool) -> GitLogRecord {
        GitLogRecord {
            options,
            supports_raw_body,
        }
    }

    /// The value of an option, `None` if it wasn't requested.
    pub fn option(&self, option: GitLogOption) -> Option<&str> {
        self.options[option.index()].as_deref()
    }

    fn lookup(&self, option: GitLogOption) -> &str {
        self.option(option).unwrap_or("")
    }

    pub fn hash(&self) -> &str {
        self.lookup(GitLogOption::Hash)
    }

    pub fn tree_hash(&self) -> &str {
        self.lookup(GitLogOption::Tree)
    }

    pub fn author_name(&self) -> &str {
        self.lookup(GitLogOption::AuthorName)
    }

    pub fn author_email(&self) -> &str {
        self.lookup(GitLogOption::AuthorEmail)
    }

    pub fn committer_name(&self) -> &str {
        self.lookup(GitLogOption::CommitterName)
    }

    pub fn committer_email(&self) -> &str {
        self.lookup(GitLogOption::CommitterEmail)
    }

    pub fn subject(&self) -> &str {
        self.lookup(GitLogOption::Subject)
    }

    pub fn body(&self) -> &str {
        self.lookup(GitLogOption::Body)
    }

    pub fn raw_body(&self) -> &str {
        self.lookup(GitLogOption::RawBody)
    }

    pub fn shortened_ref_log(&self) -> &str {
        self.lookup(GitLogOption::ShortRefLogSelector)
    }

    /// `getCommitTime` (`getDate`): milliseconds, 0 if missing or malformed.
    pub fn commit_time(&self) -> i64 {
        parse_time(self.option(GitLogOption::CommitTime)).unwrap_or(0)
    }

    /// `getAuthorTimeStamp`: milliseconds, 0 if missing or malformed.
    pub fn author_time_stamp(&self) -> i64 {
        parse_time(self.option(GitLogOption::AuthorTime)).unwrap_or(0)
    }

    /// `getFullMessage`: the raw body, or subject and body for git without `%B`, trailing
    /// whitespace stripped.
    pub fn full_message(&self) -> String {
        let message = if self.supports_raw_body {
            self.raw_body().to_string()
        } else {
            format!("{}\n\n{}", self.subject(), self.body())
        };
        message.trim_end().to_string()
    }

    /// `getParentsHashes`.
    pub fn parents_hashes(&self) -> Vec<&str> {
        let parents = self.lookup(GitLogOption::Parents);
        if parents.trim().is_empty() {
            return Vec::new();
        }
        java_split(parents, " ")
    }

    /// `getRefs`: the ref names of `%d` (with `--decorate=full`: `HEAD`, `refs/heads/...`, ...).
    pub fn refs(&self) -> Vec<String> {
        parse_ref_names(self.option(GitLogOption::RefNames))
    }

    pub fn options(&self) -> &OptionValues {
        &self.options
    }

    pub fn is_supports_raw_body(&self) -> bool {
        self.supports_raw_body
    }
}

/// A record with its `--name-status` entries (`GitLogFullRecord`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLogFullRecord {
    record: GitLogRecord,
    statuses: Vec<VcsFileStatusInfo>,
}

impl GitLogFullRecord {
    pub fn new(
        options: OptionValues,
        statuses: Vec<VcsFileStatusInfo>,
        supports_raw_body: bool,
    ) -> GitLogFullRecord {
        GitLogFullRecord {
            record: GitLogRecord::new(options, supports_raw_body),
            statuses,
        }
    }

    pub fn record(&self) -> &GitLogRecord {
        &self.record
    }

    pub fn status_infos(&self) -> &[VcsFileStatusInfo] {
        &self.statuses
    }

    /// `getPaths`: the distinct paths, in order.
    pub fn paths(&self) -> Vec<&str> {
        let mut result: Vec<&str> = Vec::new();
        for info in &self.statuses {
            for path in std::iter::once(info.first_path.as_str()).chain(info.second_path.as_deref())
            {
                if !result.contains(&path) {
                    result.push(path);
                }
            }
        }
        result
    }
}

impl std::ops::Deref for GitLogFullRecord {
    type Target = GitLogRecord;

    fn deref(&self) -> &GitLogRecord {
        &self.record
    }
}

/// `GitLogUtil.parseTime`: seconds to milliseconds.
pub fn parse_time(time: Option<&str>) -> Option<i64> {
    time?
        .trim()
        .parse::<i64>()
        .ok()
        .map(|t| t.saturating_mul(1000))
}

/// Java `String.split(regex)` for a literal separator: trailing empty strings are dropped.
fn java_split<'a>(s: &'a str, separator: &str) -> Vec<&'a str> {
    let mut parts: Vec<&str> = s.split(separator).collect();
    while parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    if parts == [""] && !s.is_empty() {
        parts.clear();
    }
    parts
}

/// `parseRefNames`.
fn parse_ref_names(decoration: Option<&str>) -> Vec<String> {
    let Some(decoration) = decoration else {
        return Vec::new();
    };
    let (Some(start), Some(end)) = (decoration.find('('), decoration.rfind(')')) else {
        return Vec::new();
    };
    if end < start + 1 {
        return Vec::new();
    }
    let refs = &decoration[start + 1..end];
    let mut result = Vec::new();
    for item in java_split(refs, ", ") {
        const POINTER: &str = " -> "; // HEAD -> refs/heads/master in Git 2.4.3+
        if item.contains(POINTER) {
            result.extend(
                item.split(POINTER)
                    .filter(|part| !part.is_empty())
                    .map(|part| part.trim().to_string()),
            );
        } else {
            // tags have the "tag:" prefix
            let raw = match item.find(':') {
                Some(colon) if colon > 0 => item[colon + 1..].trim(),
                _ => item,
            };
            result.push(raw.to_string());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_with(option: GitLogOption, value: &str) -> GitLogRecord {
        let mut options: OptionValues = Default::default();
        options[option.index()] = Some(value.to_string());
        GitLogRecord::new(options, true)
    }

    #[test]
    fn ref_names_in_old_and_new_formats() {
        let r = record_with(
            GitLogOption::RefNames,
            " (HEAD -> refs/heads/master, tag: refs/tags/v1, refs/remotes/origin/HEAD)",
        );
        assert_eq!(
            r.refs(),
            [
                "HEAD",
                "refs/heads/master",
                "refs/tags/v1",
                "refs/remotes/origin/HEAD"
            ]
        );
        assert!(record_with(GitLogOption::RefNames, "").refs().is_empty());
        assert!(record_with(GitLogOption::Hash, "a").refs().is_empty());
    }

    #[test]
    fn parents_times_and_missing_values() {
        let r = record_with(GitLogOption::Parents, "a b");
        assert_eq!(r.parents_hashes(), ["a", "b"]);
        assert!(
            record_with(GitLogOption::Parents, " ")
                .parents_hashes()
                .is_empty()
        );
        assert_eq!(
            record_with(GitLogOption::CommitTime, " 12 ").commit_time(),
            12_000
        );
        assert_eq!(record_with(GitLogOption::CommitTime, "x").commit_time(), 0);
        assert_eq!(r.subject(), "");
        assert_eq!(r.option(GitLogOption::Subject), None);
    }

    #[test]
    fn full_message_without_raw_body() {
        let mut options: OptionValues = Default::default();
        options[GitLogOption::Subject.index()] = Some("s".into());
        options[GitLogOption::Body.index()] = Some("b\n\n".into());
        assert_eq!(GitLogRecord::new(options, false).full_message(), "s\n\nb");
    }

    #[test]
    fn status_types() {
        assert_eq!(change_type_from_status("R100").unwrap(), ChangeType::Moved);
        assert_eq!(
            change_type_from_status("T").unwrap(),
            ChangeType::Modification
        );
        assert!(change_type_from_status("X").is_err());
    }
}
