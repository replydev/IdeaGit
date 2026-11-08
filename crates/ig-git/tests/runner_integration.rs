//! Runner integration tests against the real `git` executable.

mod common;

use std::sync::Arc;

use common::{init_repo, runner, runner_with};
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
    status.add_parameters(["--porcelain"]);
    assert!(g.run_command(status).success());
    assert_eq!(console.events().len(), before, "status is silent");

    let mut checkout = g.handler(tmp.path(), GitCommand::CHECKOUT);
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
