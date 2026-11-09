//! A project window: main toolbar, tool-window stripes, the dock with the Git tool window (Log and
//! Console tabs) at the bottom, the status bar, and error balloons.
//!
//! Layout ported from IntelliJ's New UI frame (`IdeRootPane`, `ToolWindowPane`, `ToolWindowToolbar`,
//! `MainToolbar`, `IdeStatusBarImpl`): the Git tool window lives at the bottom and its stripe button
//! at the bottom of the left stripe. Refreshes the log on window activation (upstream refreshes on
//! frame activation through the VFS) and after commands IdeaGit runs.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle,
    Focusable, InteractiveElement, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, TitlebarOptions, Window, WindowBounds, WindowOptions, div, point, px, size,
};
use gpui_component::dock::{DockArea, DockLayout, DockPlacement, PanelEvent, panel_handle};
use ig_git::console::{ConsoleEvent, ConsoleSink};
use ig_git::runner::Git;
use ig_git::version::GitVersion;
use ig_ui_kit::metrics::MAIN_TOOLBAR_HEIGHT;
use ig_ui_kit::notification::{Action as BalloonAction, Balloon, Severity};
use ig_ui_kit::status_bar::{StatusWidget, status_bar};
use ig_ui_kit::stripe::{StripeButton, StripeSide, stripe};
use ig_ui_kit::tool_window::IjDockRenderer;
use ig_ui_kit::{ActiveIjTheme, IjTheme, color};

use crate::actions::{
    ActivateVersionControlToolWindow, CloseProject, HideActiveWindow, RefreshLog,
    ToggleShowReadCommands,
};
use crate::console::{ChannelConsole, ConsoleView};
use crate::log::model::{LogModel, LogModelEvent};
use crate::log::view::LogView;
use crate::menus::ShowReadCommands;
use crate::strings;

/// The bottom dock's initial height. IdeaGit has no editor yet, so the Log gets most of the frame
/// (IntelliJ's default bottom tool window weight is 0.33).
const BOTTOM_DOCK_HEIGHT: f32 = 560.0;

/// `VcsLogUtil.SHORT_HASH_LENGTH`.
const SHORT_HASH_LENGTH: usize = 8;

/// Window activations closer than this refresh the log once (switching windows back and forth
/// shouldn't queue refreshes).
const ACTIVATION_REFRESH_INTERVAL: Duration = Duration::from_millis(500);

/// Forwards console events and refreshes the log after a non-silent (IdeaGit-run) command
/// finishes (upstream: `GitRepository` updates after write commands trigger a log refresh).
struct ProjectConsole {
    console: ChannelConsole,
    refresh: flume::Sender<()>,
}

impl ConsoleSink for ProjectConsole {
    fn event(&self, event: ConsoleEvent) {
        let finished = matches!(event, ConsoleEvent::Finished { .. });
        self.console.event(event);
        if finished {
            let _ = self.refresh.send(());
        }
    }

    fn silent_command(&self, working_dir: &std::path::Path, command_line: &str) {
        self.console.silent_command(working_dir, command_line);
    }
}

struct Notice {
    id: usize,
    severity: Severity,
    title: String,
    content: String,
}

/// An editor-area placeholder (IdeaGit has no editor yet).
pub struct EmptyEditor(FocusHandle);

impl gpui::EventEmitter<PanelEvent> for EmptyEditor {}
impl Focusable for EmptyEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.0.clone()
    }
}
impl gpui_component::dock::BasePanel for EmptyEditor {
    fn panel_name(&self) -> &'static str {
        "EmptyEditor"
    }
}
impl gpui_component::dock::Panel for EmptyEditor {}
impl Render for EmptyEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // focusable, so hiding a tool window has somewhere to return focus to
        div()
            .track_focus(&self.0)
            .size_full()
            .bg(color(cx.ij().tokens.panel_background))
    }
}

/// The console as a dock panel.
pub struct ConsolePanel {
    pub view: Entity<ConsoleView>,
}

impl gpui::EventEmitter<PanelEvent> for ConsolePanel {}
impl Focusable for ConsolePanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.view.read(cx).focus_handle(cx)
    }
}
impl gpui_component::dock::BasePanel for ConsolePanel {
    fn panel_name(&self) -> &'static str {
        "GitConsole"
    }
}
impl gpui_component::dock::Panel for ConsolePanel {
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some(strings::text("VcsBundle", "vcs.console.toolwindow.display.name").into())
    }
}
impl Render for ConsolePanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.view.clone()
    }
}

pub struct ProjectWindow {
    pub path: PathBuf,
    pub dock: Entity<DockArea>,
    pub log: Entity<LogModel>,
    pub log_view: Entity<LogView>,
    pub console: Entity<ConsoleView>,
    editor: Entity<EmptyEditor>,
    /// The window's own focus target: actions and menus keep working when nothing inside has
    /// focus.
    focus: FocusHandle,
    last_activation_refresh: Option<Instant>,
    notices: Vec<Notice>,
    next_notice: usize,
    menu_bar: Option<Entity<gpui_component::menu::AppMenuBar>>,
    _subscriptions: Vec<Subscription>,
}

impl ProjectWindow {
    /// Builds the window content. `git` is built here so its console feeds this window.
    pub fn new(
        path: PathBuf,
        roots: Vec<PathBuf>,
        executable: PathBuf,
        version: GitVersion,
        window: &mut Window,
        cx: &mut Context<ProjectWindow>,
    ) -> ProjectWindow {
        let (console, sink) = crate::console::create(cx);
        let (refresh_tx, refresh_rx) = flume::unbounded();
        let git = Arc::new(Git::new(
            executable,
            version,
            Arc::new(ProjectConsole {
                console: sink,
                refresh: refresh_tx,
            }),
        ));
        let log = cx.new(|cx| LogModel::new(git, roots, cx));
        let log_view = cx.new(|cx| LogView::new(log.clone(), cx));
        // the "Show Read Commands in Console" toggle is app-wide
        let show_read = cx.try_global::<ShowReadCommands>().is_some_and(|s| s.0);
        console.update(cx, |c, cx| c.set_show_silent(show_read, cx));
        let console_panel = cx.new(|_| ConsolePanel {
            view: console.clone(),
        });
        let editor = cx.new(|cx| EmptyEditor(cx.focus_handle()));

        let this = cx.entity().downgrade();
        let renderer = IjDockRenderer::with_headerless(
            strings::text("GitBundle", "git4idea.vcs.name"),
            move |window, cx| {
                let _ = this.update(cx, |this, cx| this.set_git_tool_window(false, window, cx));
            },
            vec!["EmptyEditor"],
        );
        let dock = cx.new(|cx| DockArea::new("project", None, window, cx).with_renderer(renderer));
        dock.update(cx, |dock, cx| {
            dock.set_center(DockLayout::tabs().panel(editor.clone()), window, cx);
            dock.set_dock(
                DockPlacement::Bottom,
                DockLayout::tabs()
                    .panel_view(panel_handle(log_view.clone()), cx)
                    .panel_view(panel_handle(console_panel), cx),
                window,
                cx,
            );
            dock.set_dock_size(DockPlacement::Bottom, px(BOTTOM_DOCK_HEIGHT), window, cx);
        });

        let log_for_refresh = log.downgrade();
        cx.spawn(async move |_, cx| {
            while refresh_rx.recv_async().await.is_ok() {
                while refresh_rx.try_recv().is_ok() {}
                if log_for_refresh.update(cx, |log, _| log.refresh()).is_err() {
                    break;
                }
            }
        })
        .detach();

        let subscriptions = vec![
            cx.subscribe_in(&log, window, |this, _, event: &LogModelEvent, _, cx| {
                if let LogModelEvent::WorkerFailed(message) = event {
                    this.notify(
                        Severity::Error,
                        "Git Log failed",
                        &format!("{message}. Refresh to restart it; details in ideagit.log."),
                        cx,
                    );
                }
                cx.notify();
            }),
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.refresh_on_activation(Instant::now(), cx);
                }
            }),
            cx.observe_global::<ShowReadCommands>(|this, cx| {
                let show = cx.global::<ShowReadCommands>().0;
                this.console.update(cx, |c, cx| c.set_show_silent(show, cx));
            }),
            cx.observe_global::<crate::menus::MenusChanged>(|this, cx| {
                if let Some(bar) = &this.menu_bar {
                    bar.update(cx, |bar, cx| bar.reload(cx));
                }
            }),
            cx.observe_window_appearance(window, |_, window, cx| {
                if IjTheme::sync_with_window(window, cx) {
                    cx.refresh_windows();
                }
            }),
        ];
        let menu_bar =
            (!cfg!(target_os = "macos")).then(|| gpui_component::menu::AppMenuBar::new(cx));
        window.focus(&log_view.focus_handle(cx), cx);
        ProjectWindow {
            path,
            dock,
            log,
            log_view,
            console,
            editor,
            focus: cx.focus_handle(),
            last_activation_refresh: None,
            notices: Vec::new(),
            next_notice: 0,
            menu_bar,
            _subscriptions: subscriptions,
        }
    }

    pub fn notify(
        &mut self,
        severity: Severity,
        title: &str,
        content: &str,
        cx: &mut Context<Self>,
    ) {
        self.next_notice += 1;
        self.notices.push(Notice {
            id: self.next_notice,
            severity,
            title: title.into(),
            content: content.into(),
        });
        cx.notify();
    }

    pub fn is_git_tool_window_open(&self, cx: &App) -> bool {
        self.dock.read(cx).is_dock_open(DockPlacement::Bottom)
    }

    pub fn set_git_tool_window(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_git_tool_window_open(cx) != open {
            self.dock.update(cx, |dock, cx| {
                dock.toggle_dock(DockPlacement::Bottom, window, cx)
            });
        }
        if open {
            window.focus(&self.log_view.focus_handle(cx), cx);
        } else {
            // `HideActiveWindow` returns focus to the editor
            window.focus(&self.editor.focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// Refreshes the log when the window is activated, at most once per
    /// `ACTIVATION_REFRESH_INTERVAL` (500 ms) and not while it is loading or refreshing anyway. Returns
    /// whether it refreshed.
    pub fn refresh_on_activation(&mut self, now: Instant, cx: &mut Context<Self>) -> bool {
        let recent = self
            .last_activation_refresh
            .is_some_and(|last| now.saturating_duration_since(last) < ACTIVATION_REFRESH_INTERVAL);
        let log = self.log.read(cx);
        if recent || log.is_busy() || log.state() == crate::log::model::LogState::Loading {
            return false;
        }
        self.last_activation_refresh = Some(now);
        self.log.update(cx, |log, _| log.refresh());
        true
    }

    fn project_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    /// The branch widget text (`GitBranchWidget` / `GitBranchUtil.getDisplayableBranchText`):
    /// the branch of one repository (upstream guesses it from the selected file; here the
    /// project folder's own repository, else the first), or the short hash of a detached HEAD.
    pub fn branch_text(&self, cx: &App) -> Option<String> {
        let repos = &self.log.read(cx).snapshot()?.repos;
        let repo = repos
            .iter()
            .find(|r| *r.root == *self.path)
            .or_else(|| repos.first())?;
        Some(match (&repo.current_branch, &repo.head) {
            (Some(branch), _) => branch.clone(),
            (None, Some(head)) => head.chars().take(SHORT_HASH_LENGTH).collect(),
            (None, None) => strings::text("GitBundle", "git.status.bar.widget.text.unknown"),
        })
    }

    fn render_toolbar(&self, cx: &App) -> AnyElement {
        let t = cx.ij().tokens;
        let branch = self.branch_text(cx);
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.0))
            .h(px(MAIN_TOOLBAR_HEIGHT))
            // room for the macOS traffic lights in the transparent title bar
            .pl(px(if cfg!(target_os = "macos") {
                80.0
            } else {
                12.0
            }))
            .bg(color(t.main_toolbar_background))
            .text_color(color(t.main_toolbar_foreground))
            .text_size(px(ig_ui_kit::metrics::UI_FONT_SIZE))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child(self.project_name())
            .when_some(branch, |d, branch| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .font_weight(gpui::FontWeight::NORMAL)
                        // the main toolbar is dark in both New UI themes
                        .child(ig_ui_kit::icons::icon("vcs/branch", true, 16.0))
                        .child(branch),
                )
            })
            .into_any_element()
    }
}

impl Render for ProjectWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.ij().tokens;
        let git_open = self.is_git_tool_window_open(cx);
        let git_name = strings::text("GitBundle", "git4idea.vcs.name");
        let this = cx.entity().downgrade();
        let git_button = StripeButton::new("stripe-git", "toolwindows/git")
            .selected(git_open)
            .tooltip(git_name)
            .on_click(move |_, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    let open = !this.is_git_tool_window_open(cx);
                    this.set_git_tool_window(open, window, cx);
                });
            });
        let busy = self.log.read(cx).is_busy();
        let mut right: Vec<AnyElement> = Vec::new();
        if let Some(branch) = self.branch_text(cx) {
            right.push(
                StatusWidget::new("status-branch", branch)
                    .icon("vcs/branch")
                    .into_any_element(),
            );
        }
        let left: Vec<AnyElement> = if busy {
            vec![
                StatusWidget::progress(
                    "status-progress",
                    strings::text("VcsLogBundle", "vcs.log.loading.status"),
                )
                .into_any_element(),
            ]
        } else {
            Vec::new()
        };
        let notices: Vec<AnyElement> = self
            .notices
            .iter()
            .map(|n| {
                let id = n.id;
                let this = cx.entity().downgrade();
                Balloon::new(
                    ("notice", id),
                    n.severity,
                    n.title.clone(),
                    n.content.clone(),
                )
                .action(BalloonAction::new("Dismiss", move |_, _, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.notices.retain(|n| n.id != id);
                        cx.notify();
                    });
                }))
                .into_any_element()
            })
            .collect();
        let _ = window;
        div()
            .id("project-window")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(color(t.panel_background))
            .text_color(color(t.foreground))
            .font_family(ig_ui_kit::fonts::UI_FAMILY)
            .on_action(cx.listener(|this, _: &RefreshLog, _, cx| {
                this.log.update(cx, |log, _| log.refresh())
            }))
            .on_action(
                cx.listener(|this, _: &ActivateVersionControlToolWindow, window, cx| {
                    this.set_git_tool_window(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &HideActiveWindow, window, cx| {
                this.set_git_tool_window(false, window, cx)
            }))
            .on_action(cx.listener(|_, _: &CloseProject, window, _| window.remove_window()))
            .on_action(cx.listener(|_, _: &ToggleShowReadCommands, _, cx| {
                // every project window's console follows (observing the global)
                let show = !cx.try_global::<ShowReadCommands>().is_some_and(|s| s.0);
                cx.set_global(ShowReadCommands(show));
                crate::menus::install(cx);
            }))
            .when_some(self.menu_bar.clone(), |d, bar| d.child(bar))
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(stripe(StripeSide::Left, vec![git_button], cx))
                    .child(div().flex_1().min_w_0().h_full().child(self.dock.clone()))
                    .child(stripe(StripeSide::Right, Vec::new(), cx)),
            )
            .child(status_bar(left, right, cx))
            .child(
                div()
                    .absolute()
                    .bottom(px(40.0))
                    .right(px(56.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(notices),
            )
    }
}

/// The project window shown by a window's root view (a `Root` wrapping it, or the view itself).
pub fn project_of(root: AnyView, cx: &App) -> Option<Entity<ProjectWindow>> {
    match root.downcast::<ProjectWindow>() {
        Ok(project) => Some(project),
        Err(root) => root
            .downcast::<gpui_component::Root>()
            .ok()?
            .read(cx)
            .view()
            .clone()
            .downcast::<ProjectWindow>()
            .ok(),
    }
}

/// The window showing the project at `path` (canonical), if one is open. Windows being updated
/// are skipped.
pub fn find_window(path: &Path, cx: &mut App) -> Option<AnyWindowHandle> {
    cx.windows().into_iter().find(|window| {
        window
            .update(cx, |root, _, cx| {
                project_of(root, cx).is_some_and(|p| p.read(cx).path == path)
            })
            .unwrap_or(false)
    })
}

/// Opens a project window.
pub fn open(
    path: PathBuf,
    roots: Vec<PathBuf>,
    executable: PathBuf,
    version: GitVersion,
    cx: &mut App,
) {
    let title = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(gpui::Bounds::centered(
            None,
            size(px(1400.0), px(900.0)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(12.0), px(13.0))),
        }),
        app_id: Some("IdeaGit".into()),
        ..Default::default()
    };
    let result = cx.open_window(options, move |window, cx| {
        let project = cx.new(|cx| ProjectWindow::new(path, roots, executable, version, window, cx));
        cx.new(|cx| gpui_component::Root::new(project, window, cx))
    });
    if let Err(e) = result {
        log::error!("opening a project window: {e}");
    } else {
        crate::welcome::close(cx);
    }
}
