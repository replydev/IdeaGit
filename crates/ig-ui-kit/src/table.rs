//! Table styling shared by IntelliJ-like tables (the Log): colors for the row states and the
//! header cell.
//!
//! Colors follow `platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/VcsLogGraphTable.java`
//! in the New UI: the table background is the tool-window background (`getTableBackground`),
//! selection uses `VersionControl.Log.Commit.selectionBackground` (the list selection colors,
//! focused or not), hover uses `VersionControl.Log.Commit.hoveredBackground` mixed into the row
//! background ([`hovered_background`]). No grid lines (`setShowGrid(false)`,
//! `setIntercellSpacing(emptySize)`).

use gpui::{App, Hsla, IntoElement, ParentElement, SharedString, Styled, div, px};

use crate::metrics::UI_FONT_SIZE;
use crate::theme::{ActiveIjTheme, color};

/// The colors of a row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowColors {
    pub background: Hsla,
    pub foreground: Hsla,
}

/// Table colors of the active theme.
#[derive(Clone, Copy, Debug)]
pub struct TableColors {
    pub background: Hsla,
    pub foreground: Hsla,
    pub secondary_foreground: Hsla,
    pub selection: Hsla,
    pub selection_foreground: Hsla,
    pub selection_inactive: Hsla,
    pub selection_inactive_foreground: Hsla,
    pub hover: Hsla,
    pub header_separator: Hsla,
}

impl TableColors {
    pub fn get(cx: &App) -> TableColors {
        let t = cx.ij().tokens;
        TableColors {
            background: color(t.tool_window_background),
            foreground: color(t.table_foreground),
            secondary_foreground: color(t.secondary_foreground),
            selection: color(t.list_selection_background),
            selection_foreground: color(t.table_selection_foreground),
            selection_inactive: color(t.list_selection_inactive_background),
            selection_inactive_foreground: color(t.table_selection_inactive_foreground),
            hover: color(hovered_background(
                t.log_hovered_background,
                t.tool_window_background,
            )),
            header_separator: color(t.table_header_separator),
        }
    }

    /// `VcsLogGraphTable.getStyle` base colors for a row.
    pub fn row(&self, selected: bool, focused: bool, hovered: bool) -> RowColors {
        match (selected, focused) {
            (true, true) => RowColors {
                background: self.selection,
                foreground: self.selection_foreground,
            },
            (true, false) => RowColors {
                background: self.selection_inactive,
                foreground: self.selection_inactive_foreground,
            },
            _ => RowColors {
                background: if hovered { self.hover } else { self.background },
                foreground: self.foreground,
            },
        }
    }
}

/// `VcsLogGraphTable.getHoveredBackgroundColor`: the alpha of `VersionControl.Log.Commit.hoveredBackground`
/// is not painted as transparency but used as the weight of the row background in
/// `ColorUtil.mix(opaque hover, background, alpha / 255)` (`MixedColorProducer`: per channel
/// `hover + round(balance * (background - hover))`). New UI dark's `#FFFFFFED` is therefore a 7% white
/// tint, not near-opaque white. Colors are `0xRRGGBBAA`.
pub fn hovered_background(hover: u32, background: u32) -> u32 {
    let alpha = hover & 0xFF;
    if alpha == 0xFF {
        return hover;
    }
    if alpha == 0 {
        return background;
    }
    let balance = f64::from(alpha) / 255.0;
    // `new Color(HOVERED_BACKGROUND.getRGB())` is opaque
    let first = hover | 0xFF;
    let mut mixed = 0;
    for shift in [24, 16, 8, 0] {
        let a = f64::from((first >> shift) & 0xFF);
        let b = f64::from((background >> shift) & 0xFF);
        // `Math.round`: halves round up (Rust's `round` goes away from zero)
        let value = (a + (balance * (b - a) + 0.5).floor()) as u32;
        mixed |= value.min(0xFF) << shift;
    }
    mixed
}

/// A header cell (`VcsLogGraphTable`'s header: secondary text, no grid).
pub fn header_cell(
    text: impl Into<SharedString>,
    width: Option<f32>,
    cx: &App,
) -> impl IntoElement {
    let colors = TableColors::get(cx);
    let cell = div()
        .flex()
        .items_center()
        .h_full()
        .px(px(8.0))
        .overflow_hidden()
        .text_size(px(UI_FONT_SIZE))
        .text_color(colors.secondary_foreground)
        .child(text.into());
    match width {
        Some(w) => cell.w(px(w)).flex_none(),
        None => cell.flex_1(),
    }
}

#[cfg(test)]
mod tests {
    use super::hovered_background;

    #[test]
    fn opaque_hover_is_used_as_is() {
        // New UI light: `#E9EAEC`
        assert_eq!(hovered_background(0xE9EAECFF, 0xFFFFFFFF), 0xE9EAECFF);
    }

    #[test]
    fn translucent_hover_mixes_into_the_background() {
        // New UI dark: `#FFFFFFED` over the tool window background `#2B2D30`: 7% white
        // (255 + round(237/255 * (43 - 255)) = 58, and so on)
        assert_eq!(hovered_background(0xFFFFFFED, 0x2B2D30FF), 0x3A3C3FFF);
    }

    #[test]
    fn transparent_hover_is_the_background() {
        assert_eq!(hovered_background(0xFFFFFF00, 0x2B2D30FF), 0x2B2D30FF);
    }
}
