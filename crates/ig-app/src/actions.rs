//! IdeaGit's actions, keyed by IntelliJ action ids, and their key bindings from the imported
//! IntelliJ keymaps (`assets/keymaps/{default,macos}.tsv`, written by `cargo xtask import-ui` from
//! `platform/platform-resources/src/keymaps/$default.xml` and `Mac OS X 10.5+.xml`).
//!
//! GPUI action names can't contain dots, so each action has a GPUI name and the IntelliJ id it
//! stands for ([`ACTION_IDS`]); bindings are looked up by IntelliJ id.
//!
//! Deviation: only the actions IdeaGit has are bound; keystrokes IntelliJ writes that GPUI can't
//! express are skipped (logged).

use std::sync::OnceLock;

use gpui::{Action, App, KeyBinding};

gpui::actions!(
    ideagit,
    [
        /// `OpenFile`
        OpenFile,
        /// `CloseProject`
        CloseProject,
        /// `Exit`
        Exit,
        /// `About`
        About,
        /// `ActivateVersionControlToolWindow`
        ActivateVersionControlToolWindow,
        /// `HideActiveWindow`
        HideActiveWindow,
        /// `Vcs.Log.Refresh`
        RefreshLog,
        /// IdeaGit addition: show silent (read) commands in the Git console.
        ToggleShowReadCommands,
    ]
);

/// Table navigation in the Log (`EditorUp` & co. in the keymaps; Swing's `JTable` bindings).
pub mod table {
    gpui::actions!(
        git_log_table,
        [
            SelectUp,
            SelectDown,
            ExtendUp,
            ExtendDown,
            PageUp,
            PageDown,
            SelectFirst,
            SelectLast,
            SelectAll
        ]
    );
}

/// The key context of the Log table.
pub const LOG_TABLE_CONTEXT: &str = "GitLogTable";

/// IntelliJ action id -> (GPUI action, key context).
pub fn bindable(id: &str) -> Option<(Box<dyn Action>, Option<&'static str>)> {
    let global = |a: Box<dyn Action>| Some((a, None));
    let table = |a: Box<dyn Action>| Some((a, Some(LOG_TABLE_CONTEXT)));
    match id {
        "OpenFile" => global(Box::new(OpenFile)),
        "CloseProject" => global(Box::new(CloseProject)),
        "Exit" => global(Box::new(Exit)),
        "About" => global(Box::new(About)),
        "ActivateVersionControlToolWindow" => global(Box::new(ActivateVersionControlToolWindow)),
        "HideActiveWindow" => global(Box::new(HideActiveWindow)),
        "Vcs.Log.Refresh" => global(Box::new(RefreshLog)),
        "EditorUp" => table(Box::new(table::SelectUp)),
        "EditorDown" => table(Box::new(table::SelectDown)),
        "EditorUpWithSelection" => table(Box::new(table::ExtendUp)),
        "EditorDownWithSelection" => table(Box::new(table::ExtendDown)),
        "EditorPageUp" => table(Box::new(table::PageUp)),
        "EditorPageDown" => table(Box::new(table::PageDown)),
        "EditorTextStart" => table(Box::new(table::SelectFirst)),
        "EditorTextEnd" => table(Box::new(table::SelectLast)),
        "$SelectAll" => table(Box::new(table::SelectAll)),
        _ => None,
    }
}

/// The IntelliJ ids IdeaGit implements (for menus showing shortcuts).
pub const ACTION_IDS: &[&str] = &[
    "OpenFile",
    "CloseProject",
    "Exit",
    "About",
    "ActivateVersionControlToolWindow",
    "HideActiveWindow",
    "Vcs.Log.Refresh",
];

/// Converts an IntelliJ keystroke (`KeyStroke.getKeyStroke` syntax, e.g. `meta R`,
/// `control F5`, `shift PAGE_UP`) to GPUI's (`cmd-r`, `ctrl-f5`, `shift-pageup`).
pub fn convert_keystroke(stroke: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for token in stroke.split_whitespace() {
        let mapped = match token {
            "meta" => "cmd".to_string(),
            "control" | "ctrl" => "ctrl".to_string(),
            "alt" => "alt".to_string(),
            "shift" => "shift".to_string(),
            "pressed" => continue,
            // key-release / key-typed strokes can't be expressed as GPUI bindings
            "released" | "typed" => return None,
            key => convert_key(key)?,
        };
        parts.push(mapped);
    }
    (!parts.is_empty()).then(|| parts.join("-"))
}

fn convert_key(key: &str) -> Option<String> {
    let named = match key {
        "UP" => "up",
        "DOWN" => "down",
        "LEFT" => "left",
        "RIGHT" => "right",
        "PAGE_UP" => "pageup",
        "PAGE_DOWN" => "pagedown",
        "HOME" => "home",
        "END" => "end",
        "ESCAPE" => "escape",
        "ENTER" => "enter",
        "TAB" => "tab",
        "SPACE" => "space",
        "BACK_SPACE" => "backspace",
        "DELETE" => "delete",
        "INSERT" => "insert",
        "COMMA" => ",",
        "PERIOD" => ".",
        "SLASH" => "/",
        "SEMICOLON" => ";",
        "EQUALS" => "=",
        "MINUS" => "-",
        "OPEN_BRACKET" => "[",
        "CLOSE_BRACKET" => "]",
        "BACK_SLASH" => "\\",
        "BACK_QUOTE" => "`",
        "QUOTE" => "'",
        _ => "",
    };
    if !named.is_empty() {
        return Some(named.to_string());
    }
    let is_function_key = key.len() > 1
        && key.starts_with('F')
        && key[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n));
    if is_function_key {
        return Some(key.to_ascii_lowercase());
    }
    if key.chars().count() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Some(key.to_ascii_lowercase());
    }
    None
}

/// A parsed keymap line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub action_id: String,
    /// GPUI keystroke sequence (`cmd-k cmd-s` for two-stroke shortcuts).
    pub keystrokes: String,
}

/// Parses an imported `.tsv` keymap.
pub fn parse_keymap(text: &str) -> Vec<Shortcut> {
    let mut out = Vec::new();
    for line in text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
    {
        let mut fields = line.split('\t');
        let Some(id) = fields.next() else { continue };
        let strokes: Option<Vec<String>> = fields.map(convert_keystroke).collect();
        match strokes {
            Some(strokes) if !strokes.is_empty() => out.push(Shortcut {
                action_id: id.to_string(),
                keystrokes: strokes.join(" "),
            }),
            _ => log::debug!("skipping keymap line {line:?}"),
        }
    }
    out
}

static DEFAULT_KEYMAP: &str = include_str!("../../../assets/keymaps/default.tsv");
static MACOS_KEYMAP: &str = include_str!("../../../assets/keymaps/macos.tsv");

fn cached_os_keymap() -> &'static [Shortcut] {
    static KEYMAP: OnceLock<Vec<Shortcut>> = OnceLock::new();
    KEYMAP.get_or_init(|| {
        parse_keymap(if cfg!(target_os = "macos") {
            MACOS_KEYMAP
        } else {
            DEFAULT_KEYMAP
        })
    })
}

/// The keymap of this OS (IntelliJ's default per OS: `Mac OS X 10.5+` on macOS, `$default`
/// elsewhere). Parsed once.
pub fn os_keymap() -> Vec<Shortcut> {
    cached_os_keymap().to_vec()
}

/// The first shortcut of `action_id` in this OS keymap, for menus and tooltips.
pub fn shortcut_for(action_id: &str) -> Option<String> {
    cached_os_keymap()
        .iter()
        .find(|s| s.action_id == action_id)
        .map(|s| s.keystrokes.clone())
}

/// Marks an `App` whose shortcuts are bound.
struct KeysBound;

impl gpui::Global for KeysBound {}

/// Binds every implemented action's shortcuts.
/// Idempotent per `App`.
pub fn bind_keys(cx: &mut App) {
    if cx.has_global::<KeysBound>() {
        return;
    }
    cx.set_global(KeysBound);
    let bindings: Vec<KeyBinding> = cached_os_keymap()
        .iter()
        .filter_map(|s| {
            let (action, context) = bindable(&s.action_id)?;
            let context = context.and_then(|c| {
                gpui::KeyBindingContextPredicate::parse(c)
                    .ok()
                    .map(std::rc::Rc::new)
            });
            KeyBinding::load(
                &s.keystrokes,
                action,
                context,
                false,
                None,
                cx.keyboard_mapper().as_ref(),
            )
            .map_err(|e| log::warn!("binding {} to {}: {e}", s.action_id, s.keystrokes))
            .ok()
        })
        .collect();
    cx.bind_keys(bindings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keystrokes_convert_to_gpui_syntax() {
        assert_eq!(convert_keystroke("meta R").as_deref(), Some("cmd-r"));
        assert_eq!(convert_keystroke("control F5").as_deref(), Some("ctrl-f5"));
        assert_eq!(
            convert_keystroke("shift PAGE_UP").as_deref(),
            Some("shift-pageup")
        );
        assert_eq!(convert_keystroke("alt 9").as_deref(), Some("alt-9"));
        assert_eq!(
            convert_keystroke("shift ESCAPE").as_deref(),
            Some("shift-escape")
        );
        assert_eq!(convert_keystroke("control NUMPAD5"), None);
        assert_eq!(convert_keystroke("pressed F5").as_deref(), Some("f5"));
    }

    #[test]
    fn release_and_typed_strokes_are_not_bound() {
        assert_eq!(convert_keystroke("released ESCAPE"), None);
        assert_eq!(convert_keystroke("control typed A"), None);
        assert!(parse_keymap("X\treleased ESCAPE\n").is_empty());
    }

    #[test]
    fn os_keymap_is_cached() {
        assert!(std::ptr::eq(cached_os_keymap(), cached_os_keymap()));
        assert_eq!(os_keymap(), cached_os_keymap());
        assert!(shortcut_for("Vcs.Log.Refresh").is_some());
    }

    #[test]
    fn imported_keymaps_bind_the_log_refresh() {
        let mac = parse_keymap(MACOS_KEYMAP);
        let default = parse_keymap(DEFAULT_KEYMAP);
        let find = |map: &[Shortcut], id: &str| {
            map.iter()
                .find(|s| s.action_id == id)
                .map(|s| s.keystrokes.clone())
        };
        assert_eq!(find(&mac, "Vcs.Log.Refresh").as_deref(), Some("cmd-r"));
        assert_eq!(
            find(&default, "Vcs.Log.Refresh").as_deref(),
            Some("ctrl-f5")
        );
        assert_eq!(
            find(&mac, "ActivateVersionControlToolWindow").as_deref(),
            Some("cmd-9")
        );
        // every implemented id is known to `bindable`
        for id in ACTION_IDS {
            assert!(bindable(id).is_some(), "{id}");
        }
    }

    #[test]
    fn two_stroke_lines_become_sequences() {
        let parsed = parse_keymap("# c\nX\tmeta K\tmeta S\nY\tcontrol NUMPAD1\n");
        assert_eq!(
            parsed,
            [Shortcut {
                action_id: "X".into(),
                keystrokes: "cmd-k cmd-s".into()
            }]
        );
    }
}
