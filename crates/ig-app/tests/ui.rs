//! UI tests with GPUI's `TestAppContext` (headless, no GPU): the Log loads, selects and scrolls,
//! an arrow click jumps, Refresh reaches the worker, and the Git tool window toggles.
//!
//! The log worker runs on real threads, so the tests allow parking and poll until it answers.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{AppContext as _, Entity, TestAppContext};
use ig_app::log::model::{LogModel, LogState};
use ig_git::console::NullConsole;
use ig_git::runner::Git;
use ig_log::GraphPrintElement;
use ig_log::print_element::EdgePrintElement;

/// An empty file used as the global git config (portable, unlike `/dev/null`).
fn empty_global_config() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ui-empty-gitconfig");
    if !path.exists() {
        std::fs::write(&path, "").unwrap();
    }
    path
}

/// Inherited variables that would redirect git away from the test repository (as in
/// `crates/ig-git/tests/common/mod.rs`).
const SCRUBBED_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CEILING_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
    // `GIT_CONFIG_KEY_<n>` / `GIT_CONFIG_VALUE_<n>` are ignored once `GIT_CONFIG_COUNT` is removed
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG",
    "GIT_AUTHOR_DATE",
    "GIT_COMMITTER_DATE",
];

fn git(dir: &Path, args: &[&str]) {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", empty_global_config())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "T")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "T")
        .env("GIT_COMMITTER_EMAIL", "t@example.com");
    for v in SCRUBBED_ENV {
        command.env_remove(v);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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

fn wait(
    cx: &mut TestAppContext,
    model: &Entity<LogModel>,
    mut done: impl FnMut(&mut LogModel) -> bool,
) {
    let started = Instant::now();
    loop {
        cx.run_until_parked();
        if model.update(cx, |m, _| done(m)) {
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
    // real metadata, not just loading placeholders
    wait(cx, &model, |m| m.metadata_loaded_for(10) && !m.is_busy());
}

#[gpui::test]
fn stale_rows_and_elements_are_ignored(cx: &mut TestAppContext) {
    let dir = repo(80);
    let model = log_model(cx, dir.path());
    let geometry = ig_app::log::graph_painter::GraphGeometry::new(26.0, 2.0);
    let arrow = model.update(cx, |m, _| {
        let count = m.row_count();
        assert!(m.row(count).is_none());
        assert!(!m.metadata_loaded_for(count + 1));
        m.click_row(count, false, false);
        m.click_row(usize::MAX, true, true);
        assert!(m.selection.is_empty());
        assert!(!m.hover_graph(count + 5, 5.0, 5.0, &geometry));
        assert!(!m.click_graph(count + 5, 5.0, 5.0, &geometry));
        (0..count)
            .flat_map(|r| m.row(r).unwrap().1)
            .find(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.has_arrow()))
            .expect("the long `side` edge is drawn with arrows")
    });
    // a much shorter log: the arrow's rows no longer exist
    git(dir.path(), &["branch", "-D", "side"]);
    git(dir.path(), &["reset", "-q", "--hard", "HEAD~75"]);
    model.update(cx, |m, _| {
        m.click_row(70, false, false);
        m.refresh();
    });
    wait(cx, &model, |m| m.row_count() == 5 && !m.is_busy());
    model.update(cx, |m, _| {
        // the selected commit is no longer shown
        assert!(m.selection.is_empty());
        m.click_element(arrow);
        assert!(m.selection.is_empty());
    });
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

#[gpui::test]
fn git_tool_window_toggles(cx: &mut TestAppContext) {
    let dir = repo(3);
    // `init` only reads `settings.toml` (nothing is written unless a project is opened via
    // `open_project`), so the test doesn't touch the user's settings.
    cx.executor().allow_parking();
    cx.update(ig_app::init);
    let root = dir.path().canonicalize().unwrap();
    let version = Git::detect("git", Arc::new(NullConsole)).unwrap().version();
    let (project, cx) = cx.add_window_view(|window, cx| {
        ig_app::project::ProjectWindow::new(
            root.clone(),
            vec![root.clone()],
            "git".into(),
            version,
            window,
            cx,
        )
    });
    project.update_in(cx, |p, window, cx| {
        assert!(p.is_git_tool_window_open(cx));
        p.set_git_tool_window(false, window, cx);
        assert!(!p.is_git_tool_window_open(cx));
        p.set_git_tool_window(true, window, cx);
        assert!(p.is_git_tool_window_open(cx));
    });
}
