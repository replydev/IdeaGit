//! Port of the execution parts of `plugins/git4idea/backend/src/commands/GitImplBase.java`
//! (environment, write locking, console logging, output collection) and `GitCommandResult.java`.
//!
//! Commands run synchronously on the calling thread; callers (UI) run them on a background executor.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use crate::command::{GitCommand, LockingPolicy};
use crate::console::{ConsoleEvent, ConsoleSink, OutputKind};
use crate::error::{GitError, Result};
use crate::handler::GitHandler;
use crate::version::{GitVersion, GitVersionSpecialty};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommandResult {
    pub start_failed: bool,
    pub exit_code: i32,
    /// Stdout lines, separators trimmed.
    pub output: Vec<String>,
    /// Stderr lines, separators trimmed, progress lines removed.
    pub error_output: Vec<String>,
}

impl GitCommandResult {
    pub fn success(&self) -> bool {
        self.success_ignoring(&[])
    }

    /// `ignored_error_codes` are non-zero exit codes that count as success.
    pub fn success_ignoring(&self, ignored_error_codes: &[i32]) -> bool {
        !self.start_failed && (self.exit_code == 0 || ignored_error_codes.contains(&self.exit_code))
    }

    pub fn output_as_joined_string(&self) -> String {
        self.output.join("\n")
    }

    /// Upstream `getErrorOrStdOutput`: stderr, else stdout on failure, else a generic exit message.
    pub fn presentable_error_output(&self) -> Vec<String> {
        if !self.error_output.is_empty() {
            return self.error_output.clone();
        }
        if self.success() {
            return Vec::new();
        }
        if !self.output.is_empty() {
            return self.output.clone();
        }
        vec![format!(
            "The Git process exited with the code {}",
            self.exit_code
        )]
    }

    pub fn error_output_as_joined_string(&self) -> String {
        self.presentable_error_output().join("\n")
    }

    /// Upstream `getOutputOrThrow`.
    pub fn output_or_error(&self) -> Result<String> {
        self.throw_on_error()?;
        Ok(self.output_as_joined_string())
    }

    /// Upstream `throwOnError`.
    pub fn throw_on_error(&self) -> Result<()> {
        if self.success() {
            Ok(())
        } else {
            Err(GitError::Command(self.error_output_as_joined_string()))
        }
    }
}

/// Runs git commands for one project. Holds the per-project command lock (upstream
/// `GitVcs.getCommandLock`) that serializes write commands.
pub struct Git {
    executable: PathBuf,
    version: GitVersion,
    console: Arc<dyn ConsoleSink>,
    command_lock: Mutex<()>,
    next_id: AtomicU64,
}

impl Git {
    pub fn new(
        executable: impl Into<PathBuf>,
        version: GitVersion,
        console: Arc<dyn ConsoleSink>,
    ) -> Self {
        Self {
            executable: executable.into(),
            version,
            console,
            command_lock: Mutex::new(()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Runs `git version` with `executable` and returns a runner configured with the detected version.
    pub fn detect(executable: impl Into<PathBuf>, console: Arc<dyn ConsoleSink>) -> Result<Git> {
        let executable = executable.into();
        let probe = Git::new(executable.clone(), GitVersion::NULL, console.clone());
        let cwd = std::env::current_dir()?;
        let result = probe.run_command(probe.handler(&cwd, GitCommand::VERSION));
        let output = result.output_or_error()?;
        let version = GitVersion::parse(&output)?;
        Ok(Git::new(executable, version, console))
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn version(&self) -> GitVersion {
        self.version
    }

    /// Creates a handler for this executable.
    pub fn handler(&self, working_dir: &Path, command: GitCommand) -> GitHandler {
        GitHandler::new(working_dir, &self.executable, command)
    }

    /// Upstream `GitImplBase.doRun`.
    pub fn run_command(&self, mut handler: GitHandler) -> GitCommandResult {
        for (k, v) in trace_environment(&self.version) {
            handler.add_custom_environment_variable(k, v);
        }
        let can_suppress_optional_locks =
            GitVersionSpecialty::EnvGitOptionalLocksAllowed.exists_in(&self.version);
        if can_suppress_optional_locks {
            handler.add_custom_environment_variable("GIT_OPTIONAL_LOCKS", "0");
        }
        let _guard = if should_take_write_lock(handler.command(), can_suppress_optional_locks) {
            Some(self.command_lock.lock().unwrap_or_else(|e| e.into_inner()))
        } else {
            None
        };
        self.execute(&handler)
    }

    fn execute(&self, handler: &GitHandler) -> GitCommandResult {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let log_to_console = !handler.is_silent();

        let mut cmd = handler.build_command();
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                return GitCommandResult {
                    start_failed: true,
                    exit_code: -1,
                    output: Vec::new(),
                    error_output: vec![format!("Failed to start Git process: {e}")],
                };
            }
        };
        if log_to_console {
            self.console.event(ConsoleEvent::Started {
                id,
                working_dir: handler.working_dir().to_path_buf(),
                command_line: handler.printable_command_line(),
            });
        }

        let (tx, rx) = mpsc::channel::<(OutputKind, String)>();
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let out_thread = spawn_reader(stdout, OutputKind::Stdout, tx.clone());
        let err_thread = spawn_reader(stderr, OutputKind::Stderr, tx);

        let mut output = Vec::new();
        let mut error_output = Vec::new();
        for (kind, line) in rx {
            if log_to_console && should_log_line(handler, kind, &line) {
                self.console.event(ConsoleEvent::Output {
                    id,
                    kind,
                    line: line.clone(),
                });
            }
            match kind {
                OutputKind::Stdout => output.push(line),
                OutputKind::Stderr => {
                    if !looks_like_progress(&line) {
                        error_output.push(line);
                    }
                }
            }
        }
        let _ = out_thread.join();
        let _ = err_thread.join();
        let exit_code = match child.wait() {
            Ok(status) => status.code().unwrap_or(-1),
            Err(_) => -1,
        };
        if log_to_console {
            self.console.event(ConsoleEvent::Finished { id, exit_code });
        }
        GitCommandResult {
            start_failed: false,
            exit_code,
            output,
            error_output,
        }
    }
}

fn should_take_write_lock(command: GitCommand, can_suppress_optional_locks: bool) -> bool {
    let policy = command.locking_policy();
    policy != LockingPolicy::Read
        && (policy != LockingPolicy::ReadOptionalLocking || !can_suppress_optional_locks)
}

/// Upstream `getGitTraceEnvironmentVariables` with tracing disabled (registry `git.execution.trace` = 0).
fn trace_environment(version: &GitVersion) -> Vec<(&'static str, &'static str)> {
    let mut env = vec![("GIT_TRACE", "0")];
    if GitVersionSpecialty::EnvGitTracePackAccessAllowed.exists_in(version) {
        env.push(("GIT_TRACE_PACK_ACCESS", ""));
    }
    env.push(("GIT_TRACE_PACKET", ""));
    env.push(("GIT_TRACE_PERFORMANCE", "0"));
    env.push(("GIT_TRACE_SETUP", "0"));
    env
}

/// Upstream `GitCommandOutputLogger.onLineAvailable` filters.
fn should_log_line(handler: &GitHandler, kind: OutputKind, line: &str) -> bool {
    if line.trim().is_empty() {
        return false;
    }
    match kind {
        OutputKind::Stdout => !handler.is_stdout_suppressed(),
        OutputKind::Stderr => !handler.is_stderr_suppressed(),
    }
}

const REMOTE_PROGRESS_PREFIX: &str = "remote: ";
const SUPPRESSED_PROGRESS_INDICATORS: &[&str] = &[
    "Counting objects: ",
    "Enumerating objects: ",
    "Compressing objects: ",
    "Writing objects: ",
    "Receiving objects: ",
    "Resolving deltas: ",
    "Finding sources: ",
    "Updating files: ",
    "Checking out files: ",
    "Expanding reachable commits in commit graph: ",
    "Delta compression using up to ",
];

/// Port of `GitImplBase.looksLikeProgress` (`PROGRESS_PATTERN` = `.*:\s*\d{1,3}% \(\d+/\d+\).*`).
pub fn looks_like_progress(line: &str) -> bool {
    use std::sync::LazyLock;
    static PROGRESS: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^.*:\s*\d{1,3}% \(\d+/\d+\).*$").expect("valid regex")
    });
    if PROGRESS.is_match(line) {
        return true;
    }
    SUPPRESSED_PROGRESS_INDICATORS.iter().any(|prefix| {
        line.starts_with(prefix)
            || line
                .strip_prefix(REMOTE_PROGRESS_PREFIX)
                .is_some_and(|rest| rest.starts_with(prefix))
    })
}

/// Splits a byte stream into lines on `\n`, `\r\n` and `\r` (upstream `BufferingTextSplitter`).
#[derive(Default)]
pub(crate) struct LineSplitter {
    current: Vec<u8>,
    pending_cr: bool,
}

impl LineSplitter {
    pub(crate) fn feed(&mut self, bytes: &[u8], emit: &mut dyn FnMut(String)) {
        for &b in bytes {
            if self.pending_cr {
                self.pending_cr = false;
                if b == b'\n' {
                    continue; // CRLF: the line was already emitted at CR
                }
            }
            match b {
                b'\n' => emit(self.take_line()),
                b'\r' => {
                    emit(self.take_line());
                    self.pending_cr = true;
                }
                _ => self.current.push(b),
            }
        }
    }

    pub(crate) fn finish(&mut self, emit: &mut dyn FnMut(String)) {
        if !self.current.is_empty() {
            emit(self.take_line());
        }
    }

    fn take_line(&mut self) -> String {
        let line = String::from_utf8_lossy(&self.current).into_owned();
        self.current.clear();
        line
    }
}

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    kind: OutputKind,
    tx: mpsc::Sender<(OutputKind, String)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut splitter = LineSplitter::default();
        let mut buf = [0u8; 8192];
        let mut emit = |line: String| {
            let _ = tx.send((kind, line));
        };
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => splitter.feed(&buf[..n], &mut emit),
            }
        }
        splitter.finish(&mut emit);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(chunks: &[&[u8]]) -> Vec<String> {
        let mut s = LineSplitter::default();
        let mut lines = Vec::new();
        for c in chunks {
            s.feed(c, &mut |l| lines.push(l));
        }
        s.finish(&mut |l| lines.push(l));
        lines
    }

    #[test]
    fn splits_lf_crlf_cr_across_chunks() {
        assert_eq!(split(&[b"a\nb\r\nc\rd"]), ["a", "b", "c", "d"]);
        assert_eq!(split(&[b"a\r", b"\nb\n"]), ["a", "b"]);
        assert_eq!(split(&[b"\n\n"]), ["", ""]);
    }

    #[test]
    fn progress_detection() {
        assert!(looks_like_progress(
            "remote: Finding sources:   1% (575/57489)   "
        ));
        assert!(looks_like_progress("Receiving objects:  45% (450/1000)"));
        assert!(looks_like_progress(
            "remote: Counting objects: 198285, done"
        ));
        assert!(looks_like_progress(
            "Expanding reachable commits in commit graph: 95907"
        ));
        assert!(!looks_like_progress("fatal: not a git repository"));
    }

    #[test]
    fn write_lock_rules() {
        assert!(should_take_write_lock(GitCommand::COMMIT, true));
        assert!(!should_take_write_lock(GitCommand::LOG, false));
        assert!(!should_take_write_lock(GitCommand::STATUS, true));
        assert!(should_take_write_lock(GitCommand::STATUS, false));
    }

    #[test]
    fn result_error_presentation() {
        let failed_silently = GitCommandResult {
            start_failed: false,
            exit_code: 128,
            output: vec![],
            error_output: vec![],
        };
        assert_eq!(
            failed_silently.error_output_as_joined_string(),
            "The Git process exited with the code 128"
        );
        assert!(failed_silently.output_or_error().is_err());
        let ok = GitCommandResult {
            exit_code: 0,
            output: vec!["x".into()],
            ..failed_silently
        };
        assert_eq!(ok.output_or_error().unwrap(), "x");
        assert!(
            GitCommandResult {
                exit_code: 1,
                ..ok.clone()
            }
            .success_ignoring(&[1])
        );
    }
}
