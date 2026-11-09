//! IntelliJ New UI look for IdeaGit: generated theme tokens, `expui` icons, the Inter and
//! JetBrains Mono fonts, New UI metrics, the graph palette, and IntelliJ-styled widgets (tool
//! window stripes and headers, status bar, banners and balloons, table colors).
//!
//! This crate knows nothing about Git: the app crate maps Git concepts onto these pieces. Every
//! module names the upstream sources it ports; see also `reference/upstream-map.toml`.

pub mod fonts;
pub mod icons;
pub mod metrics;
pub mod notification;
pub mod palette;
pub mod status_bar;
pub mod stripe;
pub mod table;
pub mod theme;
pub mod tool_window;

pub use theme::{ActiveIjTheme, IjTheme, color};

/// Registers fonts and installs the theme for `appearance`. Call once at startup, after
/// `gpui_component::init`.
pub fn init(appearance: gpui::WindowAppearance, cx: &mut gpui::App) -> gpui::Result<()> {
    fonts::load(cx)?;
    IjTheme::for_appearance(appearance).apply(cx);
    Ok(())
}
