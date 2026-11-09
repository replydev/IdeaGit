//! UI tests with GPUI's `TestAppContext` (headless, no GPU): the Log loads, selects and scrolls,
//! an arrow click jumps, Refresh reaches the worker, the Git tool window toggles and gives focus
//! back, Open Recent opens by path, and a folder opens in one window only.
//!
//! The app is initialized with a scratch settings file (`init_with_settings`), never the user's.
//!
//! The log worker runs on real threads, so the tests allow parking and poll until it answers.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    AnyWindowHandle, AppContext as _, Entity, Focusable as _, TestAppContext, WindowHandle,
};
use gpui_component::Root;
use ig_app::actions::ActivateVersionControlToolWindow;
use ig_app::app::AppState;
use ig_app::log::model::{LogModel, LogState};
use ig_app::menus::OpenRecent;
use ig_app::project::ProjectWindow;
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

/// The worker's `git`, isolated like the test's own commands (no global/system config, no
/// inherited repository redirection).
fn scrubbed_git() -> Arc<Git> {
    let config = empty_global_config();
    let config = config.to_str().unwrap();
    let git = Git::detect("git", Arc::new(NullConsole))
        .unwrap()
        .with_base_environment(
            &[("GIT_CONFIG_GLOBAL", config), ("GIT_CONFIG_NOSYSTEM", "1")],
            SCRUBBED_ENV,
        );
    Arc::new(git)
}

fn log_model(cx: &mut TestAppContext, root: &Path) -> Entity<LogModel> {
    cx.executor().allow_parking();
    let git = scrubbed_git();
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
        // the last arrow (the one pointing up at the bottom of the long edge), far below row 5
        let arrow = (0..count)
            .flat_map(|r| m.row(r).unwrap().1)
            .filter(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.has_arrow()))
            .last()
            .expect("the long `side` edge is drawn with arrows");
        assert!(arrow_row(&arrow) >= 5, "arrow on row {}", arrow_row(&arrow));
        arrow
    });
    // a much shorter log (5 rows): the arrow's row no longer exists
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
    let git = scrubbed_git();
    let model = cx.new(|cx| LogModel::new(git, Vec::new(), cx));
    model.read_with(cx, |m, _| assert_eq!(m.state(), LogState::NoRoots));
}

/// `ig_app::init` with a settings file in `dir` (so tests never touch the user's settings).
fn init_app(cx: &mut TestAppContext, dir: &Path) {
    cx.executor().allow_parking();
    let settings = dir.join("settings.toml");
    cx.update(|cx| ig_app::init_with_settings(settings, cx));
}

/// A project window as `project::open` builds it (a `Root` around the project).
fn project_window(cx: &mut TestAppContext, root: &Path) -> WindowHandle<Root> {
    let root = root.canonicalize().unwrap();
    let version = Git::detect("git", Arc::new(NullConsole)).unwrap().version();
    cx.add_window(move |window, cx| {
        let project = cx.new(|cx| {
            ProjectWindow::new(
                root.clone(),
                vec![root.clone()],
                "git".into(),
                version,
                window,
                cx,
            )
        });
        Root::new(project, window, cx)
    })
}

fn project_in(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<Entity<ProjectWindow>> {
    window
        .update(cx, |root, _, cx| ig_app::project::project_of(root, cx))
        .ok()
        .flatten()
}

fn project_paths(cx: &mut TestAppContext) -> Vec<PathBuf> {
    let windows = cx.update(|cx| cx.windows());
    windows
        .into_iter()
        .filter_map(|w| {
            let project = project_in(cx, w)?;
            Some(project.read_with(cx, |p, _| p.path.clone()))
        })
        .collect()
}

#[gpui::test]
fn git_tool_window_toggles_and_returns_focus(cx: &mut TestAppContext) {
    let dir = repo(3);
    let config = tempfile::tempdir().unwrap();
    init_app(cx, config.path());
    let window = project_window(cx, dir.path());
    cx.run_until_parked();
    let project = project_in(cx, window.into()).unwrap();
    window
        .update(cx, |_, window, cx| {
            project.update(cx, |p, cx| {
                assert!(p.is_git_tool_window_open(cx));
                p.set_git_tool_window(false, window, cx);
                assert!(!p.is_git_tool_window_open(cx));
            });
        })
        .unwrap();
    cx.run_until_parked();
    // focus left the hidden Log for the editor area (keys no longer reach the hidden table), and
    // window actions still dispatch from there
    window
        .update(cx, |_, window, cx| {
            let log = project.read(cx).log_view.read(cx).focus_handle(cx);
            assert!(!log.contains_focused(window, cx));
            assert!(window.focused(cx).is_some());
        })
        .unwrap();
    cx.dispatch_action(window.into(), ActivateVersionControlToolWindow);
    cx.run_until_parked();
    project.read_with(cx, |p, cx| assert!(p.is_git_tool_window_open(cx)));
}

#[gpui::test]
fn home_and_end_without_a_selection(cx: &mut TestAppContext) {
    let dir = repo(10);
    let model = log_model(cx, dir.path());
    model.update(cx, |m, _| {
        let last = m.row_count() - 1;
        let row_of_selection = |m: &LogModel| {
            let commit = *m.selection.commits().iter().next().unwrap();
            (0..m.row_count()).find(|&r| m.row(r).unwrap().0 == commit)
        };
        assert!(m.selection.is_empty());
        m.select_first();
        assert_eq!(row_of_selection(m), Some(0));
        m.selection = Default::default();
        m.select_last();
        assert_eq!(row_of_selection(m), Some(last));
        // with a selection too
        m.select_first();
        assert_eq!(row_of_selection(m), Some(0));
        assert_eq!(m.selection.len(), 1);
    });
}

#[gpui::test]
fn a_folder_opens_in_one_window(cx: &mut TestAppContext) {
    let dir = repo(3);
    let config = tempfile::tempdir().unwrap();
    init_app(cx, config.path());
    let root = dir.path().canonicalize().unwrap();
    cx.update(|cx| ig_app::app::open_project(&root, cx));
    cx.run_until_parked();
    assert_eq!(project_paths(cx), std::slice::from_ref(&root));
    // again, through a non-canonical spelling of the same folder
    let same = root.join(".");
    cx.update(|cx| ig_app::app::open_project(&same, cx));
    cx.run_until_parked();
    assert_eq!(project_paths(cx), std::slice::from_ref(&root));
    // the scratch settings recorded it; the user's settings weren't touched
    let saved = std::fs::read_to_string(config.path().join("settings.toml")).unwrap();
    assert!(saved.contains(&*root.to_string_lossy()), "{saved}");
}

#[gpui::test]
fn open_recent_opens_its_path(cx: &mut TestAppContext) {
    let (a, b) = (repo(2), repo(2));
    let config = tempfile::tempdir().unwrap();
    init_app(cx, config.path());
    let (a, b) = (
        a.path().canonicalize().unwrap(),
        b.path().canonicalize().unwrap(),
    );
    cx.update(|cx| {
        let settings = &mut cx.global_mut::<AppState>().settings;
        settings.add_recent(&a);
        settings.add_recent(&b);
    });
    // the list changes after the menu item was built: the item still opens its own project
    let item = OpenRecent { path: a.clone() };
    cx.update(|cx| cx.global_mut::<AppState>().settings.remove_recent(&b));
    cx.update(|cx| cx.dispatch_action(&item));
    cx.run_until_parked();
    assert_eq!(project_paths(cx), [a]);
}

#[gpui::test]
fn open_recent_of_a_missing_folder_offers_to_remove_it(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    init_app(cx, config.path());
    let missing = config.path().join("gone");
    cx.update(|cx| {
        cx.global_mut::<AppState>().settings.add_recent(&missing);
        cx.dispatch_action(&OpenRecent {
            path: missing.clone(),
        })
    });
    cx.run_until_parked();
    // no project window: the welcome window shows the Reopen Project dialog
    assert!(project_paths(cx).is_empty());
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Remove From List");
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(AppState::get(cx).settings.recent_projects.is_empty());
    });
}

#[gpui::test]
fn window_activation_refreshes_are_throttled(cx: &mut TestAppContext) {
    let dir = repo(3);
    let config = tempfile::tempdir().unwrap();
    init_app(cx, config.path());
    let window = project_window(cx, dir.path());
    let project = project_in(cx, window.into()).unwrap();
    let log = project.read_with(cx, |p, _| p.log.clone());
    // while loading, activation doesn't queue a refresh
    let now = Instant::now();
    if log.read_with(cx, |m, _| m.state() == LogState::Loading) {
        assert!(!project.update(cx, |p, cx| p.refresh_on_activation(now, cx)));
    }
    wait(cx, &log, |m| m.state() == LogState::Ready && !m.is_busy());
    let now = Instant::now();
    project.update(cx, |p, cx| {
        assert!(p.refresh_on_activation(now, cx));
        assert!(!p.refresh_on_activation(now + Duration::from_millis(100), cx));
        assert!(p.refresh_on_activation(now + Duration::from_millis(600), cx));
    });
}

#[gpui::test]
fn status_bar_branch_and_detached_head(cx: &mut TestAppContext) {
    let dir = repo(3);
    let config = tempfile::tempdir().unwrap();
    init_app(cx, config.path());
    let window = project_window(cx, dir.path());
    let project = project_in(cx, window.into()).unwrap();
    let log = project.read_with(cx, |p, _| p.log.clone());
    wait(cx, &log, |m| m.state() == LogState::Ready && !m.is_busy());
    let branch = project.read_with(cx, |p, cx| p.branch_text(cx));
    assert_eq!(branch.as_deref(), Some("main"));
    git(dir.path(), &["checkout", "-q", "--detach"]);
    log.update(cx, |m, _| m.refresh());
    let mut text = None;
    let started = Instant::now();
    while text.as_ref().is_none_or(|t: &String| t == "main") {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        cx.run_until_parked();
        text = project.read_with(cx, |p, cx| p.branch_text(cx));
        std::thread::sleep(Duration::from_millis(10));
    }
    let text = text.unwrap();
    assert_eq!(text.len(), 8, "{text}");
    assert!(text.chars().all(|c| c.is_ascii_hexdigit()), "{text}");
}

#[gpui::test]
fn show_read_commands_applies_to_every_window(cx: &mut TestAppContext) {
    let (a, b) = (repo(2), repo(2));
    let config = tempfile::tempdir().unwrap();
    init_app(cx, config.path());
    let first = project_window(cx, a.path());
    let second = project_window(cx, b.path());
    cx.run_until_parked();
    cx.dispatch_action(first.into(), ig_app::actions::ToggleShowReadCommands);
    cx.run_until_parked();
    for window in [first, second] {
        let project = project_in(cx, window.into()).unwrap();
        let shown = project.read_with(cx, |p, cx| p.console.read(cx).show_silent());
        assert!(shown);
    }
    // a window opened later starts with the toggle's state
    let third = project_window(cx, a.path());
    let project = project_in(cx, third.into()).unwrap();
    assert!(project.read_with(cx, |p, cx| p.console.read(cx).show_silent()));
}
