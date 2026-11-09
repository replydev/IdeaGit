//! Runner integration tests against the real `git` executable.

mod common;

use std::sync::Arc;

use common::{init_repo, isolate, runner, runner_with};
use ig_git::command::GitCommand;
use ig_git::console::{ConsoleEvent, OutputKind, RecordingConsole};

#[test]
fn detects_supported_version() {
    let git = runner();
    assert!(
        git.version().is_supported(),
        "installed git {:?} should be supported",
        git.version()
    );
}

#[test]
fn write_commands_are_logged_to_console_read_commands_are_not() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    let console = Arc::new(RecordingConsole::default());
    let g = runner_with(console.clone());
    let before = console.events().len();

    let mut status = g.handler(tmp.path(), GitCommand::STATUS);
    isolate(&mut status);
    status.add_parameters(["--porcelain"]);
    assert!(g.run_command(status).success());
    assert_eq!(console.events().len(), before, "status is silent");

    let mut checkout = g.handler(tmp.path(), GitCommand::CHECKOUT);
    isolate(&mut checkout);
    checkout.add_parameters(["-b", "feature"]);
    let result = g.run_command(checkout);
    assert!(result.success(), "{:?}", result);

    let events = console.events()[before..].to_vec();
    match &events[0] {
        ConsoleEvent::Started {
            command_line,
            working_dir,
            ..
        } => {
            assert_eq!(
                command_line,
                "git -c core.quotepath=false -c log.showSignature=false checkout -b feature"
            );
            assert_eq!(working_dir, tmp.path());
        }
        other => panic!("expected Started, got {other:?}"),
    }
    assert!(events.iter().any(|e| matches!(e,
        ConsoleEvent::Output { kind: OutputKind::Stderr, line, .. } if line.contains("Switched to a new branch 'feature'"))));
    assert!(matches!(
        events.last(),
        Some(ConsoleEvent::Finished { exit_code: 0, .. })
    ));
}

#[test]
fn failed_command_reports_stderr() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    let g = runner();
    let mut h = g.handler(tmp.path(), GitCommand::CHECKOUT);
    isolate(&mut h);
    h.add_parameters(["no-such-branch"]);
    let result = g.run_command(h);
    assert!(!result.success());
    let err = result.output_or_error().unwrap_err().to_string();
    assert!(err.contains("no-such-branch"), "{err}");
}

#[test]
fn missing_executable_is_start_failure() {
    let g = ig_git::runner::Git::new(
        "/definitely/not/git",
        ig_git::version::GitVersion::NULL,
        Arc::new(ig_git::console::NullConsole),
    );
    let result = g.run_command(g.handler(std::path::Path::new("."), GitCommand::VERSION));
    assert!(result.start_failed);
    assert!(result.error_output[0].starts_with("Failed to start Git process"));
}

/// IdeaGit addition: silent commands are reported to `ConsoleSink::silent_command` (command line
/// only), never as console events.
#[test]
fn silent_commands_reach_only_the_silent_hook() {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Sink {
        events: Mutex<Vec<ConsoleEvent>>,
        silent: Mutex<Vec<(PathBuf, String)>>,
    }
    impl ig_git::console::ConsoleSink for Sink {
        fn event(&self, event: ConsoleEvent) {
            self.events.lock().unwrap().push(event);
        }
        fn silent_command(&self, working_dir: &Path, command_line: &str) {
            self.silent
                .lock()
                .unwrap()
                .push((working_dir.to_path_buf(), command_line.to_string()));
        }
    }

    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    let sink = Arc::new(Sink::default());
    let g = runner_with(sink.clone());
    let before = sink.events.lock().unwrap().len();
    let mut status = g.handler(tmp.path(), GitCommand::STATUS);
    isolate(&mut status);
    status.add_parameters(["--porcelain"]);
    assert!(g.run_command(status).success());
    assert_eq!(sink.events.lock().unwrap().len(), before);
    let silent = sink.silent.lock().unwrap();
    let (dir, line) = silent.last().expect("status reported as silent");
    assert_eq!(dir, tmp.path());
    assert!(
        line.contains("status") && line.contains("--porcelain"),
        "{line}"
    );
}
