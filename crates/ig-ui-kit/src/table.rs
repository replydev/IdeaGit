//! Table styling shared by IntelliJ-like tables (the Log): colors for the row states and the
//! header cell.
//!
//! Colors follow `platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/VcsLogGraphTable.java`
//! in the New UI: the table background is the tool-window background (`getTableBackground`),
//! selection uses `VersionControl.Log.Commit.selectionBackground` (the list selection colors,
//! focused or not), hover uses `VersionControl.Log.Commit.hoveredBackground`. No grid lines
//! (`setShowGrid(false)`, `setIntercellSpacing(emptySize)`).

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
            hover: color(t.log_hovered_background),
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
