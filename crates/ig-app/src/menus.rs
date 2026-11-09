//! The menu bar: native on macOS, `gpui-component`'s in-window `AppMenuBar` on Windows and Linux.
//!
//! Items follow IntelliJ's main menu (`platform/platform-resources/src/idea/PlatformActions.xml`
//! groups), reduced to what IdeaGit has: File (Open…, Open Recent, Close Project, Exit), View (Tool
//! Windows → Git, plus the IdeaGit "Show read commands in Console" toggle), Git (Refresh Log),
//! Help (About).

use gpui::{App, Menu, MenuItem};
use gpui_component::GlobalState;

use crate::actions::{
    About, ActivateVersionControlToolWindow, CloseProject, Exit, OpenFile, RefreshLog,
    ToggleShowReadCommands,
};
use crate::app::AppState;
use crate::strings;

/// Opens the `index`-th recent project.
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = ideagit, no_json)]
pub struct OpenRecent {
    pub index: usize,
}

/// Recent projects listed in the menu.
const MENU_RECENTS: usize = 10;

/// Whether the console shows read commands (mirrors the toggle for the menu check mark).
#[derive(Default)]
pub struct ShowReadCommands(pub bool);

impl gpui::Global for ShowReadCommands {}

pub fn build(cx: &App) -> Vec<Menu> {
    let recents: Vec<MenuItem> = cx
        .try_global::<AppState>()
        .map(|s| s.settings.recent_projects.clone())
        .unwrap_or_default()
        .into_iter()
        .take(MENU_RECENTS)
        .enumerate()
        .map(|(index, path)| MenuItem::action(path.display().to_string(), OpenRecent { index }))
        .collect();
    let show_read = cx.try_global::<ShowReadCommands>().is_some_and(|s| s.0);
    let git = strings::text("GitBundle", "git4idea.vcs.name");
    let mut menus = Vec::new();
    if cfg!(target_os = "macos") {
        menus.push(Menu::new("IdeaGit").items([
            MenuItem::action("About IdeaGit", About),
            MenuItem::separator(),
            MenuItem::action("Quit IdeaGit", Exit),
        ]));
    }
    let mut file = vec![
        MenuItem::action("Open\u{2026}", OpenFile),
        MenuItem::submenu(Menu::new("Open Recent").items(recents)),
        MenuItem::separator(),
        MenuItem::action("Close Project", CloseProject),
    ];
    if !cfg!(target_os = "macos") {
        file.push(MenuItem::separator());
        file.push(MenuItem::action("Exit", Exit));
    }
    menus.push(Menu::new("File").items(file));
    menus.push(
        Menu::new("View").items([
            MenuItem::submenu(Menu::new("Tool Windows").items([MenuItem::action(
                git.clone(),
                ActivateVersionControlToolWindow,
            )])),
            MenuItem::separator(),
            MenuItem::action("Show Read Commands in Console", ToggleShowReadCommands)
                .checked(show_read),
        ]),
    );
    menus.push(Menu::new(git).items([MenuItem::action(
        strings::text("VcsLogBundle", "action.Vcs.Log.Refresh.text") + " Log",
        RefreshLog,
    )]));
    menus.push(Menu::new("Help").items([MenuItem::action("About", About)]));
    menus
}

/// (Re)installs the menus: natively, and for the in-window menu bar.
pub fn install(cx: &mut App) {
    let owned = build(cx).into_iter().map(Menu::owned).collect();
    GlobalState::global_mut(cx).set_app_menus(owned);
    let menus = build(cx);
    cx.set_menus(menus);
}
