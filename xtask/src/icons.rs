//! Icon copying from `assets/icons.list` (`<logical/name> <upstream path to the light svg>`).
//! The dark variant is the `<name>_dark.svg` file next to the light one, the naming convention
//! of `platform/util/ui/src/com/intellij/ui/icons/ImageDescriptor.kt` (`_dark` suffix). Files are
//! copied byte for byte.

use crate::source::Source;
use crate::{Result, bail};

/// One `icons.list` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconEntry {
    pub name: String,
    pub path: String,
}

/// Parses `icons.list`; `#` starts a comment.
pub fn parse_list(text: &str) -> Result<Vec<IconEntry>> {
    let mut entries: Vec<IconEntry> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(name), Some(path), None) = (parts.next(), parts.next(), parts.next()) else {
            bail!(
                "icons.list:{}: expected `<logical/name> <upstream path>`",
                i + 1
            )
        };
        let valid = name.split('/').all(|seg| {
            !seg.is_empty()
                && seg != "."
                && seg != ".."
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
        });
        if !valid {
            bail!("icons.list:{}: bad logical name {name:?}", i + 1);
        }
        if !path.ends_with(".svg") || path.ends_with("_dark.svg") {
            bail!("icons.list:{}: {path} is not a light .svg", i + 1);
        }
        if entries.iter().any(|e| e.name == name) {
            bail!("icons.list:{}: {name} is listed twice", i + 1);
        }
        entries.push(IconEntry {
            name: name.to_string(),
            path: path.to_string(),
        });
    }
    Ok(entries)
}

/// Reads every listed icon and its dark variant; returns `(output path, bytes)` pairs.
pub fn collect(src: &dyn Source, list: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    let mut missing = Vec::new();
    for e in parse_list(list)? {
        if !src.exists(&e.path)? {
            missing.push(format!("{} ({})", e.name, e.path));
            continue;
        }
        out.push((format!("assets/icons/{}.svg", e.name), src.read(&e.path)?));
        let dark = format!("{}_dark.svg", e.path.trim_end_matches(".svg"));
        if src.exists(&dark)? {
            out.push((
                format!("assets/icons/{}_dark.svg", e.name),
                src.read(&dark)?,
            ));
        }
    }
    if !missing.is_empty() {
        bail!(
            "icons.list: missing upstream icons in {}: {}",
            src.describe(),
            missing.join(", ")
        );
    }
    Ok(out)
}
