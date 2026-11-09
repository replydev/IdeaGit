//! Inter (UI) and JetBrains Mono (console), embedded in the binary.
//!
//! Copied unchanged by `cargo xtask import-ui` from upstream's
//! `platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts/`; both are SIL OFL 1.1 (see
//! `NOTICE` and `assets/fonts/*-OFL.txt`).

use std::borrow::Cow;

use gpui::App;

/// UI font family (IntelliJ's New UI default on all platforms).
pub const UI_FAMILY: &str = "Inter";
/// Console font family (IntelliJ's default editor font).
pub const MONO_FAMILY: &str = "JetBrains Mono";

static FONTS: &[&[u8]] = &[
    include_bytes!("../../../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf"),
];

/// Registers the embedded fonts with GPUI's text system.
pub fn load(cx: &App) -> gpui::Result<()> {
    cx.text_system()
        .add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect())
}
