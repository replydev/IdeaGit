//! Feeding `git log` output line by line to a [`GitLogParser`], without storing it.
//!
//! Port of `plugins/git4idea/backend/src/history/GitLogOutputSplitter.java`.
//!
//! Deviation: the first parse error wins (upstream keeps the last); a non-zero exit still
//! overrides, as upstream.
//!
//! Not ported: `record.setUsedHandler`; a panicking consumer propagates (upstream catches
//! `Throwable` in the listener).

use crate::console::OutputKind;
use crate::error::{GitError, Result};
use crate::handler::GitHandler;
use crate::log::parser::GitLogParser;
use crate::log::record::GitLogRecordBuilder;
use crate::runner::{Git, cleanup_error_prefixes};

/// Parses stdout lines into records passed to a consumer; collects stderr for the error message
/// (`GitLogOutputSplitter`, a `GitLineHandlerListener`).
pub struct GitLogOutputSplitter<'p, B: GitLogRecordBuilder, F: FnMut(B::Record)> {
    printable_command_line: String,
    parser: &'p mut GitLogParser<B>,
    record_consumer: F,
    errors: String,
    exception: Option<GitError>,
}

impl<'p, B: GitLogRecordBuilder, F: FnMut(B::Record)> GitLogOutputSplitter<'p, B, F> {
    pub fn new(handler: &GitHandler, parser: &'p mut GitLogParser<B>, record_consumer: F) -> Self {
        GitLogOutputSplitter {
            printable_command_line: handler.printable_command_line(),
            parser,
            record_consumer,
            errors: String::new(),
            exception: None,
        }
    }

    /// `onLineAvailable`.
    pub fn on_line_available(&mut self, line: &str, kind: OutputKind) {
        match kind {
            OutputKind::Stderr => {
                self.errors.push_str(&cleanup_error_prefixes(line));
                self.errors.push('\n');
            }
            OutputKind::Stdout => {
                if let Err(e) = self.process_output_line(line) {
                    self.exception.get_or_insert(e);
                }
            }
        }
    }

    fn process_output_line(&mut self, line: &str) -> Result<()> {
        match self.parser.parse_line(line) {
            Ok(Some(record)) => {
                (self.record_consumer)(record);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(e) => {
                self.parser.clear();
                Err(GitError::Command(format!(
                    "Error while parsing line \"{}\": {e}",
                    line.escape_default()
                )))
            }
        }
    }

    /// `processTerminated`.
    pub fn process_terminated(&mut self, exit_code: i32) {
        if exit_code != 0 {
            let mut error_message = self.errors.clone();
            if error_message.is_empty() {
                error_message = format!("The Git process exited with the code {exit_code}");
            }
            self.exception = Some(GitError::Command(format!(
                "{error_message}\nCommand line: [{}]",
                self.printable_command_line
            )));
        } else if let Some(record) = self.parser.finish() {
            (self.record_consumer)(record);
        }
    }

    /// `startFailed`.
    pub fn start_failed(&mut self, error: String) {
        self.exception = Some(GitError::Command(error));
    }

    pub fn has_errors(&self) -> bool {
        self.exception.is_some()
    }

    /// `reportErrors`.
    pub fn report_errors(self) -> Result<()> {
        match self.exception {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// Runs `handler` with a splitter: `runCommandWithoutCollectingOutput(handler).throwOnError()`
/// followed by `reportErrors()`.
pub fn run_and_split<B: GitLogRecordBuilder>(
    git: &Git,
    handler: GitHandler,
    parser: &mut GitLogParser<B>,
    record_consumer: impl FnMut(B::Record),
) -> Result<()> {
    let mut splitter = GitLogOutputSplitter::new(&handler, parser, record_consumer);
    let result = git.run_command_without_collecting_output(handler, &mut |line, kind| {
        splitter.on_line_available(line, kind)
    });
    if result.start_failed {
        splitter.start_failed(result.error_output_as_joined_string());
    } else {
        splitter.process_terminated(result.exit_code);
    }
    result.throw_on_error()?;
    splitter.report_errors()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::GitCommand;
    use crate::log::parser::{GitLogOption, NameStatus};
    use crate::log::record::DefaultGitLogRecordBuilder;

    #[test]
    fn first_stdout_error_wins() {
        let handler = GitHandler::new("/repo", "git", GitCommand::LOG);
        let mut parser = GitLogParser::new(
            DefaultGitLogRecordBuilder,
            true,
            NameStatus::None,
            &[GitLogOption::Hash, GitLogOption::Parents],
        );
        let (start, end) = (
            parser.record_start().to_string(),
            parser.record_end().to_string(),
        );
        let mut records = 0;
        let mut splitter = GitLogOutputSplitter::new(&handler, &mut parser, |_| records += 1);
        splitter.on_line_available(&format!("{start}first{end}"), OutputKind::Stdout);
        splitter.on_line_available(&format!("{start}second{end}"), OutputKind::Stdout);
        assert!(splitter.has_errors());
        let message = splitter.report_errors().unwrap_err().to_string();
        assert!(message.contains("first"), "{message}");
        assert!(!message.contains("second"), "{message}");
        assert_eq!(records, 0);
    }

    fn two_option_parser() -> GitLogParser<DefaultGitLogRecordBuilder> {
        GitLogParser::new(
            DefaultGitLogRecordBuilder,
            true,
            NameStatus::None,
            &[GitLogOption::Hash, GitLogOption::Parents],
        )
    }

    #[test]
    fn mid_stream_error_is_first_reported_and_later_records_still_arrive() {
        let handler = GitHandler::new("/repo", "git", GitCommand::LOG);
        let mut parser = two_option_parser();
        let (start, end, sep) = (
            parser.record_start().to_string(),
            parser.record_end().to_string(),
            parser.items_separator().to_string(),
        );
        let good = |h: &str| format!("{start}{h}{sep}{end}");
        let mut hashes: Vec<String> = Vec::new();
        let mut splitter =
            GitLogOutputSplitter::new(&handler, &mut parser, |r| hashes.push(r.hash().to_string()));
        splitter.on_line_available(&good("aaa"), OutputKind::Stdout);
        splitter.on_line_available(&format!("{start}bad1{end}"), OutputKind::Stdout);
        // after `clear()` the parser recovers, as upstream
        splitter.on_line_available(&good("ccc"), OutputKind::Stdout);
        splitter.on_line_available(&format!("{start}bad2{end}"), OutputKind::Stdout);
        splitter.on_line_available(&good("ddd"), OutputKind::Stdout);
        splitter.process_terminated(0);
        assert!(splitter.has_errors());
        let message = splitter.report_errors().unwrap_err().to_string();
        assert!(message.contains("bad1"), "{message}");
        assert!(!message.contains("bad2"), "{message}");
        // a record is emitted when the next record starts or at the end of the output
        assert_eq!(hashes, ["aaa", "ccc", "ddd"]);
    }

    #[test]
    fn non_zero_exit_reports_code_and_command_line() {
        let handler = GitHandler::new("/repo", "git", GitCommand::LOG);
        let mut parser = two_option_parser();
        let mut splitter = GitLogOutputSplitter::new(&handler, &mut parser, |_| {});
        splitter.process_terminated(128);
        let message = splitter.report_errors().unwrap_err().to_string();
        assert!(
            message.contains("The Git process exited with the code 128"),
            "{message}"
        );
        assert!(message.contains("Command line:"), "{message}");
    }

    #[test]
    fn non_zero_exit_prefers_stderr_and_overrides_parse_error() {
        let handler = GitHandler::new("/repo", "git", GitCommand::LOG);
        let mut parser = two_option_parser();
        let start = parser.record_start().to_string();
        let end = parser.record_end().to_string();
        let mut splitter = GitLogOutputSplitter::new(&handler, &mut parser, |_| {});
        splitter.on_line_available(&format!("{start}bad{end}"), OutputKind::Stdout);
        splitter.on_line_available("fatal: boom", OutputKind::Stderr);
        splitter.process_terminated(1);
        let message = splitter.report_errors().unwrap_err().to_string();
        assert!(message.contains("boom"), "{message}");
        assert!(message.contains("Command line:"), "{message}");
        assert!(!message.contains("bad"), "{message}");
    }
}
