//! IntelliJ New UI (`expui`) icons, embedded in the binary, and the GPUI asset source serving them.
//!
//! The SVGs are written by `cargo xtask import-ui` from the `icons.list` entries (upstream
//! `platform/icons/src/expui/**` and friends), each with its `_dark` variant where upstream has one.
//! They are drawn with `img()`, which keeps their colors (GPUI's `svg()` would tint them).

use std::borrow::Cow;

use gpui::{AssetSource, ImageSource, Img, SharedString, Styled, img, px};

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

/// Serves `icons/<name>.svg` from the embedded table.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(lookup(path).map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| in_dir(name, path))
            .map(|(name, _)| SharedString::new_static(name))
            .collect())
    }
}

/// Whether `name` is `dir` itself or lies below it (prefix match on a `/` boundary).
fn in_dir(name: &str, dir: &str) -> bool {
    let dir = dir.trim_end_matches('/');
    dir.is_empty()
        || name
            .strip_prefix(dir)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

fn lookup(path: &str) -> Option<&'static [u8]> {
    ICONS
        .binary_search_by(|(name, _)| (*name).cmp(path))
        .ok()
        .map(|i| ICONS[i].1)
}

/// Whether an icon exists (`name` without `icons/` and `.svg`, e.g. `vcs/branch`).
pub fn exists(name: &str) -> bool {
    lookup(&format!("icons/{name}.svg")).is_some()
}

/// The asset path of `name` for the theme: the `_dark` variant in dark themes when there is one.
pub fn path(name: &str, dark: bool) -> SharedString {
    if dark {
        let dark_path = format!("icons/{name}_dark.svg");
        if lookup(&dark_path).is_some() {
            return dark_path.into();
        }
    }
    format!("icons/{name}.svg").into()
}

/// An icon element of `size` logical pixels.
pub fn icon(name: &str, dark: bool, size: f32) -> Img {
    img(ImageSource::from(path(name, dark)))
        .size(px(size))
        .flex_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_for_binary_search() {
        assert!(ICONS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn list_matches_on_a_directory_boundary() {
        assert!(in_dir("icons/vcs/branch.svg", "icons/vcs"));
        assert!(in_dir("icons/vcs/branch.svg", "icons/vcs/"));
        assert!(!in_dir("icons/vcs2/branch.svg", "icons/vcs"));
        assert!(in_dir("icons/vcs/branch.svg", ""));
    }

    #[test]
    fn dark_variant_falls_back_to_light() {
        assert_eq!(path("no/such", true), "icons/no/such.svg");
        if let Some((name, _)) = ICONS.iter().find(|(n, _)| n.ends_with("_dark.svg")) {
            let base = name
                .trim_start_matches("icons/")
                .trim_end_matches("_dark.svg");
            assert_eq!(path(base, true), *name);
            assert_eq!(path(base, false), format!("icons/{base}.svg"));
        }
    }
}
