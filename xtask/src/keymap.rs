//! Keymap extraction for the actions in `assets/keymap.list`, a port of the shortcut lookup in
//! `platform/platform-impl/src/com/intellij/openapi/keymap/impl/KeymapImpl.kt`
//! (`readExternal`, `getShortcutList`, `getParentShortcuts`) and
//! `platform/platform-impl/src/com/intellij/openapi/keymap/impl/MacOSDefaultKeymap.kt`:
//!
//! - an `<action>` listed in a keymap (even with no shortcuts) replaces its parent's shortcuts;
//! - an action without its own entry uses the shortcuts its `use-shortcut-of` target has in the
//!   same keymap, else its parent keymap's shortcuts;
//! - a keymap whose name starts with `Mac OS X` swaps `control`/`meta` in shortcuts it inherits
//!   (`MacOSDefaultKeymap.convertShortcutFromParent`) and, like upstream on macOS, skips
//!   keystrokes containing `INSERT` (`readExternal`'s `skipInserts`).
//!
//! `keymap.list` lines: `<action id> [<upstream XML declaring the action>]`; the optional file is
//! read for the action's `use-shortcut-of` (upstream `ActionManagerEx.getActionBinding`).
//! Mouse and gesture shortcuts are ignored. Keystrokes keep the upstream text.

use std::collections::HashMap;

use crate::source::Source;
use crate::xml::{self, Element};
use crate::{Error, Result, bail};

pub const KEYMAPS_DIR: &str = "platform/platform-resources/src/keymaps/";
/// `(keymap name, output file)`.
pub const OUTPUTS: [(&str, &str); 2] = [
    ("$default", "assets/keymaps/default.tsv"),
    ("Mac OS X 10.5+", "assets/keymaps/macos.tsv"),
];

/// `(first keystroke, second keystroke)`.
pub type Shortcut = (String, Option<String>);

struct Keymap {
    parent: Option<String>,
    /// Own shortcuts per listed action, in file order.
    actions: HashMap<String, Vec<Shortcut>>,
}

fn is_mac(name: &str) -> bool {
    name.starts_with("Mac OS X")
}

fn read_keymap(root: &Element, mac: bool) -> Keymap {
    let mut actions: HashMap<String, Vec<Shortcut>> = HashMap::new();
    for action in root.children_named("action") {
        let Some(id) = action.attr("id") else {
            continue;
        };
        let list = actions.entry(id.to_string()).or_default();
        for s in action.children_named("keyboard-shortcut") {
            let Some(first) = s.attr("first-keystroke") else {
                continue;
            };
            if mac && first.contains("INSERT") {
                continue;
            }
            list.push((
                first.to_string(),
                s.attr("second-keystroke").map(str::to_string),
            ));
        }
    }
    Keymap {
        parent: root.attr("parent").map(str::to_string),
        actions,
    }
}

/// Swaps `control`/`ctrl` and `meta` modifiers, as `MacOSDefaultKeymap.mapModifiers`.
pub fn convert_for_mac(keystroke: &str) -> String {
    keystroke
        .split(' ')
        .map(|t| match t {
            "control" | "ctrl" => "meta",
            "meta" => "control",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

struct Keymaps {
    by_name: HashMap<String, Keymap>,
    /// `use-shortcut-of` bindings.
    bindings: HashMap<String, String>,
}

impl Keymaps {
    fn shortcuts(&self, keymap: &str, id: &str, depth: usize) -> Result<Vec<Shortcut>> {
        if depth > 16 {
            bail!("keymap parent chain too deep at {keymap:?}");
        }
        let Some(k) = self.by_name.get(keymap) else {
            bail!("no keymap named {keymap:?} in {KEYMAPS_DIR}")
        };
        if let Some(own) = k.actions.get(id) {
            return Ok(own.clone());
        }
        if let Some(bound) = self.bindings.get(id).and_then(|b| k.actions.get(b)) {
            return Ok(bound.clone());
        }
        let Some(parent) = &k.parent else {
            return Ok(Vec::new());
        };
        let inherited = self.shortcuts(parent, id, depth + 1)?;
        if !is_mac(keymap) {
            return Ok(inherited);
        }
        Ok(inherited
            .into_iter()
            .filter(|(first, _)| !first.contains("INSERT"))
            .map(|(first, second)| {
                (
                    convert_for_mac(&first),
                    second.as_deref().map(convert_for_mac),
                )
            })
            .collect())
    }
}

/// Resolves the listed actions in each keymap of [`OUTPUTS`]; returns `(output path, bytes)`.
pub fn collect(src: &dyn Source, list: &str, commit: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let mut by_name = HashMap::new();
    for path in src.files_under(KEYMAPS_DIR)? {
        if !path.ends_with(".xml") || path[KEYMAPS_DIR.len()..].contains('/') {
            continue;
        }
        let root = xml::parse(&src.read_text(path)?).map_err(|e| Error(format!("{path}: {e}")))?;
        if root.name != "keymap" {
            continue;
        }
        let Some(name) = root.attr("name") else {
            continue;
        };
        if by_name
            .insert(name.to_string(), read_keymap(&root, is_mac(name)))
            .is_some()
        {
            bail!("keymap {name:?} is defined twice in {KEYMAPS_DIR}");
        }
    }

    let mut ids: Vec<String> = Vec::new();
    let mut bindings = HashMap::new();
    for (i, raw) in list.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(id), decl, None) = (parts.next(), parts.next(), parts.next()) else {
            bail!(
                "keymap.list:{}: expected `<action id> [<declaring xml>]`",
                i + 1
            )
        };
        if ids.iter().any(|x| x == id) {
            bail!("keymap.list:{}: {id} is listed twice", i + 1);
        }
        if let Some(decl) = decl {
            let root =
                xml::parse(&src.read_text(decl)?).map_err(|e| Error(format!("{decl}: {e}")))?;
            let Some(action) = find_action(&root, id) else {
                bail!("keymap.list:{}: no <action id=\"{id}\"> in {decl}", i + 1)
            };
            if let Some(target) = action.attr("use-shortcut-of") {
                bindings.insert(id.to_string(), target.to_string());
            }
        }
        ids.push(id.to_string());
    }

    let keymaps = Keymaps { by_name, bindings };
    let mut out = Vec::new();
    for (name, file) in OUTPUTS {
        let mut text = format!(
            "# generated by `cargo xtask import-ui` from intellij-community {commit} keymap {name:?}\n\
             # <action id>\\t<first keystroke>[\\t<second keystroke>]\n"
        );
        for id in &ids {
            for (first, second) in keymaps.shortcuts(name, id, 0)? {
                text.push_str(id);
                text.push('\t');
                text.push_str(&first);
                if let Some(second) = second {
                    text.push('\t');
                    text.push_str(&second);
                }
                text.push('\n');
            }
        }
        out.push((file.to_string(), text.into_bytes()));
    }
    Ok(out)
}

fn find_action<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if el.name == "action" && el.attr("id") == Some(id) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_action(c, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_conversion_swaps_control_and_meta() {
        assert_eq!(convert_for_mac("control shift F5"), "meta shift F5");
        assert_eq!(convert_for_mac("meta C"), "control C");
        assert_eq!(convert_for_mac("ctrl A"), "meta A");
        assert_eq!(convert_for_mac("PAGE_UP"), "PAGE_UP");
    }
}
