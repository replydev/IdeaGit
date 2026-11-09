//! Theme and editor color scheme resolution, a simplified port of IntelliJ's UI theme loading:
//!
//! - `platform/platform-impl/src/com/intellij/ide/ui/UIThemeBean.kt` (`readFlatMapFromJson`,
//!   `putEntry`, `importFromParentTheme`): `ui` objects are flattened to dotted keys (a `UI`
//!   segment is appended without a dot), `os.default` values are kept, and a theme's `ui` and
//!   `colors` maps override its `parentTheme`'s.
//! - `platform/platform-impl/src/com/intellij/ide/ui/UITheme.kt` (`applyTheme`, `addPattern`):
//!   the merged `ui` map is applied in order; a value naming a `colors` entry becomes that color,
//!   `*.x` overrides every key applied so far that ends with `.x` and is remembered as a pattern.
//! - `platform/platform-impl/src/com/intellij/ide/ui/uiThemeParser.kt` and
//!   `platform/util/src/com/intellij/ui/ColorHexUtil.java`: `#RGB[A]` / `#RRGGBB[AA]` colors,
//!   alpha last.
//! - `platform/util/ui/src/com/intellij/ui/JBColor.java` (`findPatternMatch`): a color key that
//!   is not set falls back to a `*` pattern it ends with.
//! - `platform/editor-ui-ex/src/com/intellij/openapi/editor/colors/impl/AbstractColorsScheme.java`
//!   and `ValueElementReader.java`: scheme `<colors>`/`<attributes>`, `parent_scheme` chains,
//!   short hex values.
//!
//! Deviations: only `os.default` values are used (upstream prefers the running OS's `os.*` key);
//! LaF defaults that come from Java code are absent; a theme without `parentTheme` has no parent;
//! when several `*` patterns match a missing key, the longest wins (upstream: hash-map order).

use std::collections::HashMap;

use serde_json::{Map, Value as Json};

use crate::source::Source;
use crate::xml::{self, Element};
use crate::{Error, Result, bail};

pub const THEMES_DIR: &str = "platform/platform-resources/src/themes/";
pub const DEFAULT_SCHEMES: &str = "platform/platform-resources/src/DefaultColorSchemesManager.xml";
/// The New UI themes the tokens are generated for (theme `"name"`s).
pub const LIGHT_THEME: &str = "Light";
pub const DARK_THEME: &str = "Dark";

/// A theme `ui` value after parsing.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `0xRRGGBBAA`.
    Color(u32),
    Number(f64),
    Text(String),
    Bool(bool),
}

/// A resolved theme: its applied `ui` defaults plus its editor scheme chain.
pub struct Theme {
    pub name: String,
    /// Theme files from the theme itself up to the root parent.
    pub files: Vec<String>,
    defaults: HashMap<String, Value>,
    /// `*.x` patterns, keyed by `x`.
    patterns: HashMap<String, Value>,
    /// Editor schemes from the theme's `editorScheme` up to the root `parent_scheme`.
    pub schemes: Vec<(String, Element)>,
}

/// Parses `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA` (the `#` is optional), as `ColorHexUtil`.
pub fn parse_hex(s: &str) -> Option<u32> {
    let h = s.strip_prefix('#').or(s.strip_prefix("0x")).unwrap_or(s);
    if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok();
    // Expands nibble `i` (from the right) to a byte placed at byte position `at` (from the right).
    let byte = |i: u32, at: u32| v.map(|v| (((v >> (4 * i)) & 0xF) * 17) << (8 * at));
    match h.len() {
        3 => Some(byte(2, 3)? | byte(1, 2)? | byte(0, 1)? | 0xFF),
        4 => Some(byte(3, 3)? | byte(2, 2)? | byte(1, 1)? | byte(0, 0)?),
        6 => Some((v? << 8) | 0xFF),
        8 => v,
        _ => None,
    }
}

/// Parses a scheme color value as `ValueElementReader.toColor`: 6+ digits via `ColorHexUtil`,
/// otherwise a (left-padded) hex RGB integer.
pub fn parse_scheme_hex(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.len() >= 6
        && let Some(c) = parse_hex(s)
    {
        return Some(c);
    }
    let rgb = u32::from_str_radix(s, 16).ok()?;
    Some(((rgb & 0xFF_FFFF) << 8) | 0xFF)
}

fn is_color_like(s: &str) -> bool {
    s.len() <= 9 && s.starts_with('#')
}

/// A theme file: its path and parsed JSON object.
type ThemeFile = (String, Map<String, Json>);

/// The `name -> files` index of the theme files under [`THEMES_DIR`].
fn theme_index(src: &dyn Source) -> Result<HashMap<String, Vec<ThemeFile>>> {
    let mut index: HashMap<String, Vec<_>> = HashMap::new();
    for path in src.files_under(THEMES_DIR)? {
        if !path.ends_with(".theme.json") {
            continue;
        }
        let json: Json = serde_json::from_str(&src.read_text(path)?)
            .map_err(|e| Error(format!("{path}: invalid JSON: {e}")))?;
        let Json::Object(obj) = json else {
            bail!("{path}: not a JSON object")
        };
        if let Some(Json::String(name)) = obj.get("name") {
            index
                .entry(name.clone())
                .or_default()
                .push((path.to_string(), obj));
        }
    }
    Ok(index)
}

/// Flattens a theme's `ui` object as `readFlatMapFromJson` does, keeping key order.
fn flatten(obj: &Map<String, Json>, prefix: &mut Vec<String>, out: &mut Vec<(String, Value)>) {
    for (key, value) in obj {
        let path = |key: Option<&str>| {
            let mut path = String::new();
            for (i, e) in prefix.iter().enumerate() {
                if i > 0 && e != "UI" {
                    path.push('.');
                }
                path.push_str(e);
            }
            if let Some(key) = key {
                if key != "UI" && !path.is_empty() {
                    path.push('.');
                }
                path.push_str(key);
            }
            path
        };
        let parsed = match value {
            Json::Object(inner) => {
                prefix.push(key.clone());
                flatten(inner, prefix, out);
                prefix.pop();
                continue;
            }
            Json::Array(items) => match items.iter().rev().find_map(Json::as_str) {
                Some(s) => Value::Text(s.to_string()),
                None => continue,
            },
            Json::String(s) if is_color_like(s) => parse_hex(s)
                .map(Value::Color)
                .unwrap_or_else(|| Value::Text(s.clone())),
            Json::String(s) => Value::Text(s.clone()),
            Json::Number(n) => Value::Number(n.as_f64().unwrap_or_default()),
            Json::Bool(b) => Value::Bool(*b),
            Json::Null => continue,
        };
        match key.as_str() {
            "os.default" => {
                let composite = path(None);
                if !out.iter().any(|(k, _)| *k == composite) {
                    out.push((composite, parsed));
                }
            }
            k if k.starts_with("os.") => {}
            k => {
                let full = path(Some(k));
                match out.iter_mut().find(|(k, _)| *k == full) {
                    Some(slot) => slot.1 = parsed,
                    None => out.push((full, parsed)),
                }
            }
        }
    }
}

/// `importMapFromParentTheme`: parent entries the child doesn't set, then the child's entries.
fn merge<T: Clone>(parent: &[(String, T)], child: Vec<(String, T)>) -> Vec<(String, T)> {
    let mut result: Vec<(String, T)> = parent
        .iter()
        .filter(|(k, _)| !child.iter().any(|(c, _)| c == k))
        .cloned()
        .collect();
    result.extend(child);
    result
}

impl Theme {
    /// Loads the theme named `name` with its parent chain and editor scheme chain.
    pub fn load(src: &dyn Source, name: &str) -> Result<Theme> {
        let index = theme_index(src)?;
        let lookup = |name: &str| -> Result<&ThemeFile> {
            match index.get(name).map(Vec::as_slice) {
                Some([one]) => Ok(one),
                Some(many) => bail!(
                    "theme name {name:?} is ambiguous: {}",
                    many.iter()
                        .map(|(p, _)| p.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                None => bail!(
                    "no theme named {name:?} under {THEMES_DIR} in {}",
                    src.describe()
                ),
            }
        };
        let mut chain = vec![lookup(name)?];
        while let Some(Json::String(parent)) = chain.last().and_then(|(_, o)| o.get("parentTheme"))
        {
            let next = lookup(parent)?;
            if chain.iter().any(|(p, _)| *p == next.0) {
                bail!("theme {name:?}: parentTheme cycle at {}", next.0);
            }
            chain.push(next);
        }

        let mut ui: Vec<(String, Value)> = Vec::new();
        let mut colors: Vec<(String, Value)> = Vec::new();
        for (_, obj) in chain.iter().rev() {
            let mut own = Vec::new();
            if let Some(Json::Object(map)) = obj.get("ui") {
                flatten(map, &mut Vec::new(), &mut own);
            }
            ui = merge(&ui, own);
            let mut own_colors = Vec::new();
            if let Some(Json::Object(map)) = obj.get("colors") {
                flatten(map, &mut Vec::new(), &mut own_colors);
            }
            colors = merge(&colors, own_colors);
        }

        let palette: HashMap<&str, &Value> = colors.iter().map(|(k, v)| (k.as_str(), v)).collect();
        let resolve_named = |v: &Value| -> Value {
            let mut v = v.clone();
            for _ in 0..16 {
                match &v {
                    Value::Text(s) if palette.contains_key(s.as_str()) => {
                        v = palette[s.as_str()].clone()
                    }
                    _ => break,
                }
            }
            v
        };

        let mut defaults: HashMap<String, Value> = HashMap::new();
        let mut patterns: HashMap<String, Value> = HashMap::new();
        for (key, value) in &colors {
            if let Value::Color(_) = resolve_named(value) {
                defaults.insert(format!("ColorPalette.{key}"), resolve_named(value));
            }
        }
        for (key, value) in &ui {
            let value = resolve_named(value);
            if let Some(tail) = key.strip_prefix('*').filter(|t| t.starts_with('.')) {
                patterns.insert(tail[1..].to_string(), value.clone());
                for (k, v) in defaults.iter_mut() {
                    if k.ends_with(tail) {
                        *v = value.clone();
                    }
                }
            } else {
                defaults.insert(key.clone(), value);
            }
        }

        let schemes = match chain[0].1.get("editorScheme") {
            Some(Json::String(scheme)) => scheme_chain(src, scheme)?,
            _ => Vec::new(),
        };
        Ok(Theme {
            name: name.to_string(),
            files: chain.iter().map(|(p, _)| p.clone()).collect(),
            defaults,
            patterns,
            schemes,
        })
    }

    /// The color for a `ui` key, and the `*` pattern it came from if it wasn't set directly.
    pub fn color(&self, key: &str) -> Option<(u32, Option<String>)> {
        if let Some(Value::Color(c)) = self.defaults.get(key) {
            return Some((*c, None));
        }
        self.patterns
            .iter()
            .filter(|(tail, v)| key.ends_with(tail.as_str()) && matches!(v, Value::Color(_)))
            .max_by(|(a, _), (b, _)| a.len().cmp(&b.len()).then_with(|| b.cmp(a)))
            .and_then(|(tail, v)| match v {
                Value::Color(c) => Some((*c, Some(format!("*.{tail}")))),
                _ => None,
            })
    }

    /// The number for a `ui` key (set directly; numbers don't use `*` patterns).
    pub fn number(&self, key: &str) -> Option<f64> {
        match self.defaults.get(key) {
            Some(Value::Number(n)) => Some(*n),
            _ => None,
        }
    }

    /// A scheme `<colors>` option, from the nearest scheme in the chain that sets it.
    pub fn scheme_color(&self, option: &str) -> Option<u32> {
        for (_, scheme) in &self.schemes {
            let found = scheme
                .children_named("colors")
                .flat_map(|c| c.children_named("option"))
                .find(|o| o.attr("name") == Some(option));
            if let Some(o) = found {
                return o.attr("value").and_then(parse_scheme_hex);
            }
        }
        None
    }

    /// An attribute of a scheme `<attributes>` entry (e.g. `FOREGROUND`), following
    /// `baseAttributes` inheritance.
    pub fn scheme_attribute(&self, attr: &str, field: &str) -> Option<u32> {
        let mut name = attr.to_string();
        for _ in 0..16 {
            let found = self.schemes.iter().find_map(|(_, scheme)| {
                scheme
                    .children_named("attributes")
                    .flat_map(|a| a.children_named("option"))
                    .find(|o| o.attr("name") == Some(name.as_str()))
            })?;
            if let Some(value) = found.child("value") {
                let opt = value
                    .children_named("option")
                    .find(|o| o.attr("name") == Some(field))?;
                return opt.attr("value").and_then(parse_scheme_hex);
            }
            name = found.attr("baseAttributes")?.to_string();
        }
        None
    }
}

/// The scheme named `name` and its `parent_scheme` ancestors, from the XML files under
/// [`THEMES_DIR`] and [`DEFAULT_SCHEMES`].
fn scheme_chain(src: &dyn Source, name: &str) -> Result<Vec<(String, Element)>> {
    let mut schemes: HashMap<String, Vec<(String, Element)>> = HashMap::new();
    let mut paths: Vec<&str> = src
        .files_under(THEMES_DIR)?
        .into_iter()
        .filter(|p| p.ends_with(".xml"))
        .collect();
    paths.push(DEFAULT_SCHEMES);
    for path in paths {
        let root = xml::parse(&src.read_text(path)?).map_err(|e| Error(format!("{path}: {e}")))?;
        let found: Vec<Element> = if root.name == "scheme" {
            vec![root]
        } else {
            root.children_named("scheme").cloned().collect()
        };
        for scheme in found {
            if let Some(n) = scheme.attr("name") {
                schemes
                    .entry(n.to_string())
                    .or_default()
                    .push((path.to_string(), scheme));
            }
        }
    }
    let mut chain: Vec<(String, Element)> = Vec::new();
    let mut next = Some(name.to_string());
    while let Some(n) = next.take() {
        let (path, scheme) = match schemes.get(&n).map(Vec::as_slice) {
            Some([one]) => one.clone(),
            Some(_) => bail!("editor scheme {n:?} is defined more than once"),
            None => bail!(
                "no editor scheme named {n:?} in {} or {DEFAULT_SCHEMES}",
                THEMES_DIR
            ),
        };
        if chain
            .iter()
            .any(|(_, s)| s.attr("name") == Some(n.as_str()))
        {
            bail!("editor scheme parent cycle at {n:?}");
        }
        next = scheme.attr("parent_scheme").map(str::to_string);
        chain.push((path, scheme));
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_put_alpha_last() {
        assert_eq!(parse_hex("#3574F0"), Some(0x3574F0FF));
        assert_eq!(parse_hex("#FFFFFF1A"), Some(0xFFFFFF1A));
        assert_eq!(parse_hex("#abc"), Some(0xAABBCCFF));
        assert_eq!(parse_hex("#1234"), Some(0x11223344));
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("Gray1"), None);
        assert_eq!(parse_scheme_hex("0"), Some(0x000000FF));
        assert_eq!(parse_scheme_hex("7f0000"), Some(0x7F0000FF));
        assert_eq!(parse_scheme_hex("ffffff4d"), Some(0xFFFFFF4D));
        assert_eq!(parse_scheme_hex(""), None);
    }

    #[test]
    fn flatten_follows_upstream_key_rules() {
        let json: Json = serde_json::from_str(
            r##"{"A":{"B":{"c":"#000000"},"UI":"cls","x":{"os.mac":"#111111","os.default":"#222222"}},
                 "D.e":1.5,"Label":{"UI":{"x":"#333333"}}}"##,
        )
        .unwrap();
        let mut out = Vec::new();
        flatten(json.as_object().unwrap(), &mut Vec::new(), &mut out);
        let keys: Vec<&str> = out.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["A.B.c", "AUI", "A.x", "D.e", "LabelUI.x"]);
        assert_eq!(out[2].1, Value::Color(0x222222FF));
        assert_eq!(out[3].1, Value::Number(1.5));
    }
}
