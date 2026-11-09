//! The menu bar: native on macOS, `gpui-component`'s in-window `AppMenuBar` on Windows and Linux.
//!
//! Items follow IntelliJ's main menu (`platform/platform-resources/src/idea/PlatformActions.xml`
//! groups), reduced to what IdeaGit has: File (Open…, Open Recent, Close Project, Exit), View (Tool
//! Windows → Git and Console, plus the IdeaGit "Show read commands in Console" toggle), Git
//! (Refresh Log), Help (About).

use std::path::PathBuf;

use gpui::{App, Menu, MenuItem};
use gpui_component::GlobalState;

use crate::actions::{
    About, ActivateConsoleToolWindow, ActivateVersionControlToolWindow, CloseProject, Exit,
    OpenFile, RefreshLog, ToggleShowReadCommands,
};
use crate::app::AppState;
use crate::strings;

/// Opens a recent project (`ReopenProjectAction`). Carries the path, not a list index: a menu
/// built before the list changed still opens what it shows.
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = ideagit, no_json)]
pub struct OpenRecent {
    pub path: PathBuf,
}

/// Recent projects listed in the menu.
const MENU_RECENTS: usize = 10;

/// Whether the console shows read commands (mirrors the toggle for the menu check mark).
#[derive(Default)]
pub struct ShowReadCommands(pub bool);

impl gpui::Global for ShowReadCommands {}

/// Set whenever the menus are reinstalled; the in-window menu bars observe it to reload.
pub struct MenusChanged;

impl gpui::Global for MenusChanged {}

pub fn build(cx: &App) -> Vec<Menu> {
    let recents: Vec<MenuItem> = cx
        .try_global::<AppState>()
        .map(|s| s.settings.recent_projects.clone())
        .unwrap_or_default()
        .into_iter()
        .take(MENU_RECENTS)
        .map(|path| MenuItem::action(path.display().to_string(), OpenRecent { path }))
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
            MenuItem::submenu(Menu::new("Tool Windows").items([
                MenuItem::action(git.clone(), ActivateVersionControlToolWindow),
                MenuItem::action(
                    strings::text("VcsBundle", "vcs.console.toolwindow.display.name"),
                    ActivateConsoleToolWindow,
                ),
            ])),
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

/// (Re)installs the menus: natively, and for the in-window menu bars (which reload on
/// [`MenusChanged`]).
pub fn install(cx: &mut App) {
    let owned = build(cx).into_iter().map(Menu::owned).collect();
    GlobalState::global_mut(cx).set_app_menus(owned);
    let menus = build(cx);
    cx.set_menus(menus);
    cx.set_global(MenusChanged);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gpui::{MenuItem, TestAppContext};

    use super::{OpenRecent, build};
    use crate::app::{AppState, GitStatus};
    use crate::settings::Settings;

    fn recent_actions(cx: &mut TestAppContext) -> Vec<OpenRecent> {
        cx.update(|cx| {
            let menus = build(cx);
            let file = menus.iter().find(|m| m.name == "File").unwrap();
            let recent = file
                .items
                .iter()
                .find_map(|item| match item {
                    MenuItem::Submenu(menu) if menu.name == "Open Recent" => Some(menu),
                    _ => None,
                })
                .unwrap();
            recent
                .items
                .iter()
                .filter_map(|item| match item {
                    MenuItem::Action { action, .. } => {
                        action.as_any().downcast_ref::<OpenRecent>().cloned()
                    }
                    _ => None,
                })
                .collect()
        })
    }

    #[gpui::test]
    fn open_recent_items_carry_their_paths(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = Settings::default();
        settings.add_recent(&PathBuf::from("/older"));
        settings.add_recent(&PathBuf::from("/newer"));
        cx.update(|cx| {
            cx.set_global(AppState {
                settings,
                settings_path: dir.path().join("settings.toml"),
                git: GitStatus::Missing(String::new()),
                git_environment: None,
            })
        });
        let paths = |actions: Vec<OpenRecent>| -> Vec<PathBuf> {
            actions.into_iter().map(|a| a.path).collect()
        };
        let before = recent_actions(cx);
        assert_eq!(
            paths(before.clone()),
            [PathBuf::from("/newer"), PathBuf::from("/older")]
        );
        // the list changes after the menu was built: its items still name their own projects
        cx.update(|cx| {
            cx.global_mut::<AppState>()
                .settings
                .remove_recent(&PathBuf::from("/newer"))
        });
        assert_eq!(before[0].path, PathBuf::from("/newer"));
        assert_eq!(paths(recent_actions(cx)), [PathBuf::from("/older")]);
    }
}
