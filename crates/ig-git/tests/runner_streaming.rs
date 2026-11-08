//! Streaming, stdin and listener-safety tests of the runner against the real `git` executable.

mod common;

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use common::{git, init_repo, isolate, runner, runner_with};
use ig_git::command::GitCommand;
use ig_git::console::{ConsoleEvent, OutputKind, RecordingConsole};

#[test]
fn listener_receives_stdout_and_stderr_without_collecting_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    for n in ["second", "third"] {
        git(tmp.path(), &["commit", "-q", "--allow-empty", "-m", n]);
    }
    let g = runner();

    let mut h = g.handler(tmp.path(), GitCommand::LOG);
    isolate(&mut h);
    h.add_parameters(["--format=%s"]);
    let mut stdout = Vec::new();
    let result = g.run_command_without_collecting_output(h, &mut |line, kind| {
        if kind == OutputKind::Stdout {
            stdout.push(line.to_string());
        }
    });
    assert!(result.success(), "{result:?}");
    assert_eq!(stdout, ["third", "second", "initial"]);
    assert!(result.output.is_empty());

    let mut h = g.handler(tmp.path(), GitCommand::REV_PARSE);
    isolate(&mut h);
    h.add_parameters(["--verify", "nonexistent"]);
    let mut stderr = Vec::new();
    let result = g.run_command_without_collecting_output(h, &mut |line, kind| {
        if kind == OutputKind::Stderr {
            stderr.push(line.to_string());
        }
    });
    assert!(!result.success());
    assert!(result.output.is_empty());
    assert!(!result.error_output.is_empty());
    assert_eq!(stderr, result.error_output);
}

#[test]
fn large_stdin_input_does_not_deadlock() {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let g = runner();
        let mut h = g.handler(tmp.path(), GitCommand::HASH_OBJECT);
        isolate(&mut h);
        h.add_parameters(["--stdin"]);
        h.set_input(vec![b'x'; 3 * 1024 * 1024]);
        let _ = tx.send(g.run_command(h));
    });
    let result = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("hash-object --stdin must not hang on large input");
    assert!(result.success(), "{result:?}");
    assert_eq!(result.output.len(), 1);
    let hash = &result.output[0];
    assert!(hash.len() >= 40 && hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn child_ignoring_stdin_does_not_hang() {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let g = runner();
        let cwd = std::env::temp_dir();
        let mut h = g.handler(&cwd, GitCommand::VERSION);
        isolate(&mut h);
        h.set_input(vec![b'x'; 3 * 1024 * 1024]);
        let _ = tx.send(g.run_command(h));
    });
    let result = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("git version must not hang when input is given");
    assert!(result.success(), "{result:?}");
    assert!(result.output[0].starts_with("git version"));
}

#[test]
fn panicking_listener_still_cleans_up_and_resumes_panic() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    let console = Arc::new(RecordingConsole::default());
    let g = runner_with(console.clone());

    let mut h = g.handler(tmp.path(), GitCommand::HASH_OBJECT);
    isolate(&mut h);
    h.add_parameters(["--stdin"]);
    h.set_input(b"hello\n".to_vec());
    assert!(!h.is_silent());

    let mut calls = 0;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        g.run_command_without_collecting_output(h, &mut |_, _| {
            calls += 1;
            panic!("listener boom");
        })
    }));
    let payload = outcome.expect_err("the listener panic must be resumed");
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"listener boom"));
    assert_eq!(calls, 1, "listener is not called after its first panic");

    let events = console.events();
    assert!(matches!(events.first(), Some(ConsoleEvent::Started { .. })));
    assert!(
        matches!(
            events.last(),
            Some(ConsoleEvent::Finished { exit_code: 0, .. })
        ),
        "{events:?}"
    );

    // The runner is still usable afterwards (the write lock was released).
    let mut h = g.handler(tmp.path(), GitCommand::HASH_OBJECT);
    isolate(&mut h);
    h.add_parameters(["--stdin"]);
    h.set_input(b"hello\n".to_vec());
    assert!(g.run_command(h).success());
}
