//! UI tests with GPUI's `TestAppContext` (headless, no GPU): the Log loads, selects and scrolls,
//! an arrow click jumps, and Refresh reaches the worker.
//!
//! The log worker runs on real threads, so the tests allow parking and poll until it answers.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{AppContext as _, Entity, TestAppContext};
use ig_app::log::model::{LogModel, LogState};
use ig_git::console::NullConsole;
use ig_git::runner::Git;
use ig_log::GraphPrintElement;
use ig_log::print_element::EdgePrintElement;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "T")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "T")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// `main` with `n` commits; `side` forks from the first commit and gets one recent commit, so its
/// edge to the root spans the whole log (drawn with arrows).
fn repo(n: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    git(d, &["init", "-q"]);
    git(d, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    for i in 0..n {
        git(
            d,
            &["commit", "-q", "--allow-empty", "-m", &format!("c{i}")],
        );
        if i == 0 {
            git(d, &["branch", "side"]);
        }
    }
    git(d, &["checkout", "-q", "side"]);
    git(d, &["commit", "-q", "--allow-empty", "-m", "side"]);
    git(d, &["checkout", "-q", "main"]);
    dir
}

fn wait(cx: &mut TestAppContext, model: &Entity<LogModel>, done: impl Fn(&LogModel) -> bool) {
    let started = Instant::now();
    loop {
        cx.run_until_parked();
        if model.read_with(cx, |m, _| done(m)) {
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn log_model(cx: &mut TestAppContext, root: &Path) -> Entity<LogModel> {
    cx.executor().allow_parking();
    let git = Arc::new(Git::detect("git", Arc::new(NullConsole)).unwrap());
    let roots = vec![root.canonicalize().unwrap()];
    let model = cx.new(|cx| LogModel::new(git, roots, cx));
    wait(cx, &model, |m| {
        m.state() == LogState::Ready
            && m.snapshot().is_some_and(|s| s.data.is_full())
            && !m.is_busy()
    });
    model
}

#[gpui::test]
fn log_loads_selects_and_scrolls(cx: &mut TestAppContext) {
    let dir = repo(60);
    let model = log_model(cx, dir.path());
    model.update(cx, |m, _| {
        assert_eq!(m.row_count(), 61);
        m.click_row(3, false, false);
        m.move_selection(1, true);
        m.move_selection(1, true);
        assert_eq!(m.selection.len(), 3);
        m.move_selection(isize::MAX / 2, false);
        assert_eq!(m.selection.len(), 1);
        m.request_metadata(0..10);
    });
    wait(cx, &model, |m| m.cache.len() >= 10);
}

#[gpui::test]
fn arrow_click_jumps_to_the_target_row(cx: &mut TestAppContext) {
    let dir = repo(80);
    let model = log_model(cx, dir.path());
    model.update(cx, |m, _| {
        let arrow = (0..m.row_count())
            .flat_map(|r| m.row(r).unwrap().1)
            .find(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.has_arrow()))
            .expect("the long `side` edge is drawn with arrows");
        let from = arrow_row(&arrow);
        m.click_element(arrow);
        assert_eq!(m.selection.len(), 1, "the jump selects its target");
        let commit = *m.selection.commits().iter().next().unwrap();
        let target = (0..m.row_count())
            .find(|&r| m.row(r).unwrap().0 == commit)
            .unwrap();
        assert!(
            target.abs_diff(from) > 10,
            "jumped from row {from} to {target}"
        );
    });
}

fn arrow_row(element: &GraphPrintElement) -> usize {
    use ig_log::print_element::PrintElement;
    element.row_index() as usize
}

#[gpui::test]
fn refresh_reaches_the_worker(cx: &mut TestAppContext) {
    let dir = repo(5);
    let model = log_model(cx, dir.path());
    git(dir.path(), &["commit", "-q", "--allow-empty", "-m", "new"]);
    model.update(cx, |m, _| m.refresh());
    wait(cx, &model, |m| m.row_count() == 7);
}

#[gpui::test]
fn no_repositories_state(cx: &mut TestAppContext) {
    let git = Arc::new(Git::detect("git", Arc::new(NullConsole)).unwrap());
    let model = cx.new(|cx| LogModel::new(git, Vec::new(), cx));
    model.read_with(cx, |m, _| assert_eq!(m.state(), LogState::NoRoots));
}
