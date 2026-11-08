//! Feeding `git log` output line by line to a [`GitLogParser`], without storing it.
//!
//! Port of `plugins/git4idea/backend/src/history/GitLogOutputSplitter.java`.

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
                    self.exception = Some(e);
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
