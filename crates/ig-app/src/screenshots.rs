//! `cargo xtask screenshots`: renders a repository's Log in the light and dark themes to PNG
//! without a display or screen-recording permission (GPUI's `HeadlessAppContext` with the
//! platform's offscreen renderer: Metal on macOS, wgpu on Linux).

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{AppContext as _, HeadlessAppContext, px, size};
use ig_ui_kit::IjTheme;

use crate::app::{AppState, GitStatus};
use crate::log::model::LogState;
use crate::project::ProjectWindow;

/// Renders `repo` into `out/ideagit-log-{light,dark}.png`; returns the process exit code.
pub fn run(out: &Path, repo: &Path) -> i32 {
    let repo = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    if let Err(e) = std::fs::create_dir_all(out) {
        eprintln!("can't create {}: {e}", out.display());
        return 1;
    }
    for (name, theme) in [("light", IjTheme::light()), ("dark", IjTheme::dark())] {
        let path = out.join(format!("ideagit-log-{name}.png"));
        match render(&repo, theme, &path) {
            Ok(elapsed) => eprintln!("wrote {} (log ready after {elapsed:?})", path.display()),
            Err(e) => {
                eprintln!("{name}: {e}");
                return 1;
            }
        }
    }
    0
}

fn render(repo: &Path, theme: IjTheme, path: &Path) -> Result<Duration, String> {
    let text_system = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(
        text_system,
        Arc::new(ig_ui_kit::icons::Assets),
        gpui_platform::current_headless_renderer,
    );
    cx.allow_parking();
    cx.update(|cx| {
        crate::init(cx);
        theme.pin(cx);
    });
    let (executable, version) = match cx.update(|cx| AppState::get(cx).git.clone()) {
        GitStatus::Ok {
            executable,
            version,
        } => (executable, version),
        other => return Err(format!("git is not usable: {other:?}")),
    };
    let roots = ig_git::roots::detect_roots(repo, crate::app::ROOTS_SCAN_DEPTH);
    let repo_path = repo.to_path_buf();
    let window = cx
        .open_window(size(px(1400.0), px(900.0)), move |window, cx| {
            cx.new(|cx| ProjectWindow::new(repo_path, roots, executable, version, window, cx))
        })
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    let mut ready_at = None;
    // wait for the log and the metadata of the first rows (the worker runs on real threads)
    while started.elapsed() < Duration::from_secs(60) {
        cx.run_until_parked();
        let done = cx
            .update_window(window.into(), |root, _, cx| {
                let project = root.downcast::<ProjectWindow>().ok()?;
                let log = project.read(cx).log.read(cx);
                let ready = match log.state() {
                    LogState::Ready => {
                        log.row_count() > 0 && log.cache.len() >= log.row_count().min(30)
                    }
                    LogState::Loading => false,
                    _ => true,
                };
                Some(ready && !log.is_busy())
            })
            .ok()
            .flatten()
            .unwrap_or(false);
        if done {
            ready_at.get_or_insert(started.elapsed());
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let elapsed = ready_at.ok_or("the log didn't load within 60 s")?;
    // two frames: the first lays out the table, the second paints rows with metadata requested by it
    for _ in 0..3 {
        cx.update_window(window.into(), |_, window, _| window.refresh())
            .map_err(|e| e.to_string())?;
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(100));
    }
    let image = cx
        .capture_screenshot(window.into())
        .map_err(|e| e.to_string())?;
    image.save(path).map_err(|e| e.to_string())?;
    if std::env::var_os("IDEAGIT_SCROLL_BENCH").is_some() {
        scroll_bench(&mut cx, window.into());
    }
    Ok(elapsed)
}

/// Informal scroll measurement: jumps through the whole log (worst case: every frame shows new
/// rows) and times building each frame (layout + paint into the scene, CPU side; GPU work and
/// presentation are not included).
fn scroll_bench(cx: &mut HeadlessAppContext, window: gpui::AnyWindowHandle) {
    let mut times = Vec::new();
    for step in 0..300usize {
        let started = Instant::now();
        let _ = cx.update_window(window, |root, window, cx| {
            if let Ok(project) = root.downcast::<ProjectWindow>() {
                let log = project.read(cx).log.clone();
                log.update(cx, |log, _| {
                    let rows = log.row_count().max(1);
                    log.scroll
                        .scroll_to_item((step * 151) % rows, gpui::ScrollStrategy::Top);
                });
            }
            window.refresh();
            // free the element arena of this frame, as the platform loop does after each draw
            window.draw(cx).clear(cx);
        });
        times.push(started.elapsed());
        cx.run_until_parked();
    }
    times.sort();
    let pct = |p: usize| times[(times.len() - 1) * p / 100];
    eprintln!(
        "scroll bench: {} frames, median {:?}, p95 {:?}, max {:?}",
        times.len(),
        pct(50),
        pct(95),
        times[times.len() - 1]
    );
}
