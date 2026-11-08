//! Parsing `git log` output produced with a custom `--pretty=format:`.
//!
//! Port of `plugins/git4idea/backend/src/history/GitLogParser.java` (and `GitFormatException`).
//!
//! A record is `RECORD_START item ITEMS_SEPARATOR item ... RECORD_END`, optionally followed by
//! `--name-status` lines. Each parser instance appends a random tail of control characters to the
//! three markers; its length grows with the number of format errors seen in the process, so that
//! a commit message containing a marker breaks at most a few parses.
//!
//! Lines are processed byte-wise: the markers are ASCII control characters, which never occur
//! inside a multi-byte UTF-8 sequence, so items are cut at character boundaries (upstream works
//! on UTF-16 chars).
//!
//! Deviation: errors upstream logs with `LOG.error` (format errors in `parse`) are dropped.

use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::log::record::{GitLogRecordBuilder, change_type_from_status};
use crate::log::unescape::unescape_git_path;

/// Records begin with `%x01%x01`.
pub const RECORD_START: &str = "\u{1}\u{1}";
/// Items of commit information (hash, committer, subject, ...) are separated by `%x02%x02`.
pub const ITEMS_SEPARATOR: &str = "\u{2}\u{2}";
/// Records end with `%x03%x03`.
pub const RECORD_END: &str = "\u{3}\u{3}";
const MAX_SEPARATOR_LENGTH: usize = 10;
const CONTROL_CHARS: [char; 3] = ['\u{1}', '\u{2}', '\u{3}'];
/// Limits the line quoted in an error to ~2mb.
const INPUT_ERROR_MESSAGE_HEAD_LIMIT: usize = 1_000_000;
const INPUT_ERROR_MESSAGE_TAIL_LIMIT: usize = 100;

static ERROR_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Output that doesn't match the format (`GitFormatException`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct GitFormatError(pub String);

/// `--name-status` or no flag (`GitLogParser.NameStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameStatus {
    /// No flag.
    None,
    /// `--name-status`.
    Status,
}

/// A piece of commit information requested with a `--pretty=format:` placeholder
/// (`GitLogParser.GitLogOption`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GitLogOption {
    Hash,
    Tree,
    CommitTime,
    AuthorName,
    AuthorTime,
    AuthorEmail,
    CommitterName,
    CommitterEmail,
    Subject,
    Body,
    Parents,
    RefNames,
    ShortRefLogSelector,
    RawBody,
}

impl GitLogOption {
    pub const COUNT: usize = 14;

    pub fn placeholder(self) -> &'static str {
        match self {
            GitLogOption::Hash => "H",
            GitLogOption::Tree => "T",
            GitLogOption::CommitTime => "ct",
            GitLogOption::AuthorName => "an",
            GitLogOption::AuthorTime => "at",
            GitLogOption::AuthorEmail => "ae",
            GitLogOption::CommitterName => "cn",
            GitLogOption::CommitterEmail => "ce",
            GitLogOption::Subject => "s",
            GitLogOption::Body => "b",
            GitLogOption::Parents => "P",
            GitLogOption::RefNames => "d",
            GitLogOption::ShortRefLogSelector => "gd",
            GitLogOption::RawBody => "B",
        }
    }

    /// Position in [`GitLogOption`] order, used to store option values in an array.
    pub fn index(self) -> usize {
        self as usize
    }
}

/// The parsed options of one record, by option.
pub type OptionValues = [Option<String>; GitLogOption::COUNT];

/// Parses `git log` output for a fixed list of options into records built by `B`.
pub struct GitLogParser<B: GitLogRecordBuilder> {
    supports_raw_body: bool,
    pretty: String,
    options_parser: OptionsParser,
    paths_parser: PathsParser,
    record_builder: B,
    is_in_body: bool,
}

impl<B: GitLogRecordBuilder> GitLogParser<B> {
    /// `supports_raw_body`: `GitVersionSpecialty.STARTED_USING_RAW_BODY_IN_FORMAT` (see
    /// [`crate::log::util::supports_raw_body`]).
    pub fn new(
        record_builder: B,
        supports_raw_body: bool,
        name_status: NameStatus,
        options: &[GitLogOption],
    ) -> GitLogParser<B> {
        let record_start = format!("{RECORD_START}{}", generate_random_sequence());
        let record_end = format!("{RECORD_END}{}", generate_random_sequence());
        let items_separator = format!("{ITEMS_SEPARATOR}{}", generate_random_sequence());
        let pretty = format!(
            "--pretty=format:{}{}{}",
            encode_for_git(&record_start),
            make_format_from_options(options, &encode_for_git(&items_separator)),
            encode_for_git(&record_end)
        );
        GitLogParser {
            supports_raw_body,
            pretty,
            options_parser: OptionsParser {
                options: options.to_vec(),
                result: PartialResult::default(),
                record_start: record_start.into_bytes(),
                record_end: record_end.into_bytes(),
                items_separator: items_separator.into_bytes(),
            },
            paths_parser: PathsParser {
                name_status,
                error_text: None,
            },
            record_builder,
            is_in_body: true,
        }
    }

    /// `parse`: all records of `output`. Records with format errors are skipped.
    pub fn parse(&mut self, output: &str) -> Vec<B::Record> {
        let mut result = Vec::new();
        for line in output.split('\n') {
            match self.parse_line(line) {
                Ok(Some(record)) => result.push(record),
                Ok(None) => {}
                Err(_) => self.clear(),
            }
        }
        if let Some(record) = self.finish() {
            result.push(record);
        }
        result
    }

    /// `parseOneRecord`: the first record of `output`.
    pub fn parse_one_record(&mut self, output: &str) -> Option<B::Record> {
        let records = self.parse(output);
        self.clear();
        records.into_iter().next()
    }

    /// `parseLine`: feeds one line without its separator; returns a record when one is complete.
    pub fn parse_line(&mut self, line: &str) -> Result<Option<B::Record>, GitFormatError> {
        if self.paths_parser.expects_paths() {
            return self.parse_line_with_paths(line);
        }
        self.parse_line_without_paths(line)
    }

    fn parse_line_with_paths(&mut self, line: &str) -> Result<Option<B::Record>, GitFormatError> {
        if self.is_in_body {
            self.is_in_body = !self.options_parser.parse_line(line)?;
        } else {
            if line
                .as_bytes()
                .starts_with(&self.options_parser.record_start)
            {
                let record = self.create_record();
                self.is_in_body = !self.options_parser.parse_line(line)?;
                return Ok(record);
            }
            self.paths_parser
                .parse_line(line, &mut self.record_builder)?;
        }
        Ok(None)
    }

    fn parse_line_without_paths(
        &mut self,
        line: &str,
    ) -> Result<Option<B::Record>, GitFormatError> {
        if self.options_parser.parse_line(line)? {
            return Ok(self.create_record());
        }
        Ok(None)
    }

    /// `finish`: the last record, at the end of the output.
    pub fn finish(&mut self) -> Option<B::Record> {
        if self.options_parser.is_empty() {
            return None;
        }
        self.create_record()
    }

    fn create_record(&mut self) -> Option<B::Record> {
        if self.paths_parser.error_text.is_some()
            || !self.options_parser.has_complete_options_list()
        {
            self.options_parser.clear();
            self.record_builder.clear();
            self.paths_parser.clear();
            return None;
        }
        let options = self.options_parser.get_result();
        self.options_parser.clear();
        let record = self.record_builder.build(options, self.supports_raw_body);
        self.record_builder.clear();
        self.paths_parser.clear();
        self.is_in_body = true;
        Some(record)
    }

    pub fn clear(&mut self) {
        self.options_parser.clear();
        self.record_builder.clear();
        self.is_in_body = true;
    }

    /// `getPretty`: the `--pretty=format:...` parameter to pass to `git log`.
    pub fn get_pretty(&self) -> &str {
        &self.pretty
    }
}

/// `makeFormatFromOptions(options, separator)`.
pub fn make_format_from_options(options: &[GitLogOption], separator: &str) -> String {
    options
        .iter()
        .map(|o| format!("%{}", o.placeholder()))
        .collect::<Vec<_>>()
        .join(separator)
}

fn encode_for_git(line: &str) -> String {
    line.chars()
        .map(|c| format!("%x{:02x}", c as u32))
        .collect()
}

fn generate_random_sequence() -> String {
    let length = ERROR_COUNT.load(Ordering::Relaxed) % (MAX_SEPARATOR_LENGTH - RECORD_START.len());
    (0..length)
        .map(|_| CONTROL_CHARS[(random_u64() % CONTROL_CHARS.len() as u64) as usize])
        .collect()
}

/// A random number without a dependency: `RandomState` is randomly seeded per instance.
fn random_u64() -> u64 {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u8(0);
    hasher.finish()
}

fn throw_gfe(message: &str, line: &str) -> GitFormatError {
    ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
    GitFormatError(format!(
        "{message} [{}]",
        get_truncated_escaped_output(line)
    ))
}

fn get_truncated_escaped_output(line: &str) -> String {
    let len = line.chars().count();
    let line = if len > INPUT_ERROR_MESSAGE_HEAD_LIMIT + INPUT_ERROR_MESSAGE_TAIL_LIMIT + 32 {
        let head: String = line.chars().take(INPUT_ERROR_MESSAGE_HEAD_LIMIT).collect();
        let tail: String = line
            .chars()
            .skip(len - INPUT_ERROR_MESSAGE_TAIL_LIMIT)
            .collect();
        let more = len - INPUT_ERROR_MESSAGE_HEAD_LIMIT - INPUT_ERROR_MESSAGE_TAIL_LIMIT;
        format!("{head}...({more} more characters)...{tail}")
    } else {
        line.to_string()
    };
    line.escape_default().to_string()
}

struct OptionsParser {
    options: Vec<GitLogOption>,
    result: PartialResult,
    record_start: Vec<u8>,
    record_end: Vec<u8>,
    items_separator: Vec<u8>,
}

impl OptionsParser {
    /// Returns `true` when the record ended on this line.
    fn parse_line(&mut self, line: &str) -> Result<bool, GitFormatError> {
        let bytes = line.as_bytes();
        let mut offset = 0;

        if self.result.is_empty() {
            if !bytes.starts_with(&self.record_start) {
                return Ok(false);
            }
            offset += self.record_start.len();
        }

        while offset < bytes.len() {
            if self.at_record_end(bytes, offset) {
                self.result.finish_item();
                if !self.has_complete_options_list() {
                    return Err(throw_gfe(
                        &format!(
                            "Parsed incomplete options {:?} for {:?}",
                            self.result.result, self.options
                        ),
                        line,
                    ));
                }
                return Ok(true);
            }
            if bytes[offset..].starts_with(&self.items_separator) {
                self.result.finish_item();
                offset += self.items_separator.len();
            } else {
                self.result.current_item.push(bytes[offset]);
                offset += 1;
            }
        }

        self.result.current_item.push(b'\n');
        Ok(false)
    }

    fn has_complete_options_list(&self) -> bool {
        self.result.result.len() == self.options.len()
    }

    fn at_record_end(&self, bytes: &[u8], offset: usize) -> bool {
        bytes.len() >= self.record_end.len()
            && offset == bytes.len() - self.record_end.len()
            && bytes[offset..].starts_with(&self.record_end)
    }

    fn get_result(&self) -> OptionValues {
        let mut values: OptionValues = Default::default();
        for (option, value) in self.options.iter().zip(&self.result.result) {
            values[option.index()] = Some(value.clone());
        }
        values
    }

    fn clear(&mut self) {
        self.result.clear();
    }

    fn is_empty(&self) -> bool {
        self.result.is_empty()
    }
}

/// `--name-status` lines (`PathsParser`).
struct PathsParser {
    name_status: NameStatus,
    error_text: Option<String>,
}

impl PathsParser {
    fn parse_line<B: GitLogRecordBuilder>(
        &mut self,
        line: &str,
        record_builder: &mut B,
    ) -> Result<(), GitFormatError> {
        if line.is_empty() {
            return Ok(());
        }
        let parts = parse_paths_line(line);
        if parts.is_empty() {
            return Ok(());
        }
        if self.name_status != NameStatus::Status {
            return Err(throw_gfe("Status list not expected", line));
        }
        if parts.len() < 2 {
            self.error_text = Some(format!("Could not parse status line [{line}]"));
            return Ok(());
        }
        let change_type = change_type_from_status(parts[0])?;
        let first_path = try_unescape_path(parts[1]);
        let second_path = parts.get(2).map(|p| try_unescape_path(p));
        record_builder.add_path(change_type, first_path, second_path);
        Ok(())
    }

    fn expects_paths(&self) -> bool {
        self.name_status == NameStatus::Status
    }

    fn clear(&mut self) {
        self.error_text = None;
    }
}

/// `tryUnescapePath`: the path itself if it can't be unescaped.
fn try_unescape_path(path: &str) -> String {
    unescape_git_path(path).unwrap_or_else(|_| path.to_string())
}

/// Tab-separated tokens; a trailing tab doesn't add an empty token.
fn parse_paths_line(line: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < line.len() {
        let token_end = line[offset..].find('\t').map_or(line.len(), |i| offset + i);
        result.push(&line[offset..token_end]);
        offset = token_end + 1;
    }
    result
}

#[derive(Default)]
struct PartialResult {
    result: Vec<String>,
    current_item: Vec<u8>,
}

impl PartialResult {
    fn finish_item(&mut self) {
        let item = std::mem::take(&mut self.current_item);
        self.result.push(
            String::from_utf8(item)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()),
        );
    }

    fn clear(&mut self) {
        self.current_item.clear();
        self.result = Vec::new();
    }

    fn is_empty(&self) -> bool {
        self.result.is_empty() && self.current_item.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::record::DefaultGitLogRecordBuilder;

    #[test]
    fn pretty_format_encodes_markers() {
        let parser = GitLogParser::new(
            DefaultGitLogRecordBuilder,
            true,
            NameStatus::None,
            &[GitLogOption::Hash, GitLogOption::Parents],
        );
        let start = String::from_utf8(parser.options_parser.record_start.clone()).unwrap();
        let expected_start: String = start
            .chars()
            .map(|c| format!("%x{:02x}", c as u32))
            .collect();
        assert!(
            parser
                .get_pretty()
                .starts_with(&format!("--pretty=format:{expected_start}%H"))
        );
        assert!(parser.get_pretty().starts_with("--pretty=format:%x01%x01"));
        assert!(parser.get_pretty().contains("%x02%x02"));
        assert!(parser.get_pretty().contains("%P%x03%x03"));
    }

    #[test]
    fn incomplete_record_is_a_format_error_and_bumps_the_separator_length() {
        let mut parser = GitLogParser::new(
            DefaultGitLogRecordBuilder,
            true,
            NameStatus::None,
            &[GitLogOption::Hash, GitLogOption::Parents],
        );
        let start = String::from_utf8(parser.options_parser.record_start.clone()).unwrap();
        let end = String::from_utf8(parser.options_parser.record_end.clone()).unwrap();
        let before = ERROR_COUNT.load(Ordering::Relaxed);
        let error = parser.parse_line(&format!("{start}abc{end}")).unwrap_err();
        assert!(
            error.0.starts_with("Parsed incomplete options [\"abc\"]"),
            "{error}"
        );
        assert!(ERROR_COUNT.load(Ordering::Relaxed) > before);
        parser.clear();
        // a status line without --name-status
        let mut parser = GitLogParser::new(
            DefaultGitLogRecordBuilder,
            true,
            NameStatus::None,
            &[GitLogOption::Hash],
        );
        assert!(
            parser
                .paths_parser
                .parse_line("M\ta", &mut DefaultGitLogRecordBuilder)
                .is_err()
        );
    }

    #[test]
    fn paths_line_tokens() {
        assert_eq!(parse_paths_line("M\tfile \t"), ["M", "file "]);
        assert_eq!(parse_paths_line("R100\ta\tb"), ["R100", "a", "b"]);
        assert_eq!(parse_paths_line("\t"), [""]);
    }
}
