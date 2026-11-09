//! Developer tasks for IdeaGit, run via `cargo xtask <task>`.
//!
//! `import-ui` copies UI resources from a pinned intellij-community checkout: theme colors
//! ([`theme`], [`tokens`]), icons ([`icons`]), message bundles ([`strings`]), keymaps
//! ([`keymap`]) and fonts ([`fonts`]). Everything is resolved before anything is written, so a
//! failing import leaves the workspace untouched; writes are deterministic and only touch files
//! whose content changed ([`output`]).

pub mod fonts;
pub mod icons;
pub mod keymap;
pub mod output;
pub mod source;
pub mod strings;
pub mod theme;
pub mod tokens;
pub mod xml;

use std::fmt;
use std::path::Path;

pub use source::{FsSource, GitSource, Source};

/// An import failure, carrying a message meant for the person running the task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Returns early with an [`Error`] built from a format string.
#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::Error(format!($($arg)*)))
    };
}

/// Hand-written input lists, relative to the output workspace root.
pub const THEME_KEYS: &str = "assets/theme.keys";
pub const ICONS_LIST: &str = "assets/icons.list";
pub const STRINGS_LIST: &str = "assets/strings.list";
pub const KEYMAP_LIST: &str = "assets/keymap.list";

/// Generated outputs, relative to the output workspace root.
pub const TOKENS_RS: &str = "crates/ig-ui-kit/src/theme/tokens.rs";

/// What an import produced, for the CLI to print.
#[derive(Debug, Default)]
pub struct Summary {
    pub written: Vec<String>,
    pub unchanged: usize,
    pub removed: Vec<String>,
}

/// Runs `import-ui`: reads the input lists under `<out>/assets`, resolves them against `source`
/// and writes the generated files under `out`. `commit` is recorded in generated headers.
pub fn import_ui(source: &dyn Source, out: &Path, commit: &str) -> Result<Summary> {
    let read_input = |rel: &str| -> Result<String> {
        std::fs::read_to_string(out.join(rel)).map_err(|e| Error(format!("cannot read {rel}: {e}")))
    };
    let keys = tokens::parse_keys(&read_input(THEME_KEYS)?)?;
    let light = theme::Theme::load(source, theme::LIGHT_THEME)?;
    let dark = theme::Theme::load(source, theme::DARK_THEME)?;
    let resolved = tokens::resolve(&keys, &light, &dark)?;
    let icons = icons::collect(source, &read_input(ICONS_LIST)?)?;
    let strings = strings::collect(source, &read_input(STRINGS_LIST)?)?;
    let keymaps = keymap::collect(source, &read_input(KEYMAP_LIST)?, commit)?;
    let fonts = fonts::collect(source)?;

    let mut o = output::Output::new(out);
    o.text(
        TOKENS_RS,
        &tokens::render_rust(&resolved, &light, &dark, commit),
    );
    o.text(
        "assets/themes/light.json",
        &tokens::render_json(&resolved, tokens::Variant::Light),
    );
    o.text(
        "assets/themes/dark.json",
        &tokens::render_json(&resolved, tokens::Variant::Dark),
    );
    for (rel, bytes) in icons.into_iter().chain(fonts).chain(strings).chain(keymaps) {
        o.bytes(&rel, &bytes);
    }
    o.finish(&[
        "assets/icons",
        "assets/strings",
        "assets/keymaps",
        "assets/themes",
        "assets/fonts",
    ])
}

/// Reads `commit = "..."` from `reference/UPSTREAM` under `root`.
pub fn pinned_commit(root: &Path) -> Result<String> {
    let path = root.join("reference/UPSTREAM");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| Error(format!("cannot read {}: {e}", path.display())))?;
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("commit")
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            return Ok(value.trim().trim_matches('"').to_string());
        }
    }
    bail!("no `commit = \"...\"` line in {}", path.display())
}
