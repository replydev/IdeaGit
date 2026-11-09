//! New UI metrics, in logical pixels (GPUI `px`, which equal IntelliJ's unscaled `JBUI` pixels).
//!
//! Hand-copied from the pinned upstream sources; each constant names its source. Values that
//! upstream reads from a theme key fall back to the default given here when neither expUI theme
//! sets the key (checked at the pinned commit).

/// `JBUI.CurrentTheme.VersionControl.Log.defaultRowHeight()` (`VersionControl.Log.Commit.rowHeight`,
/// not set by the expUI themes).
pub const LOG_MIN_ROW_HEIGHT: f32 = 26.0;
/// `JBUI.CurrentTheme.VersionControl.Log.defaultVerticalPadding()`.
pub const LOG_VERTICAL_PADDING: f32 = 7.0;

/// The Log row height: `GraphCommitCellRenderer.calculateHeight()` in the New UI,
/// `max(fontHeight + verticalPadding, rowHeight)` (the label painter is never taller here).
pub fn log_row_height(font_line_height: f32) -> f32 {
    (font_line_height + LOG_VERTICAL_PADDING)
        .ceil()
        .max(LOG_MIN_ROW_HEIGHT)
}

/// `PaintParameters.ROW_HEIGHT`: the row height the graph metrics below are defined for.
pub const GRAPH_BASE_ROW_HEIGHT: f32 = 22.0;
/// `PaintParameters.WIDTH_NODE`.
pub const GRAPH_WIDTH_NODE: f32 = 16.0;
/// `PaintParameters.CIRCLE_RADIUS`.
pub const GRAPH_CIRCLE_RADIUS: f32 = 4.0;
/// `PaintParameters.THICK_LINE`.
pub const GRAPH_THICK_LINE: f32 = 1.5;
/// `PaintParameters.SELECT_THICK_LINE`.
pub const GRAPH_SELECT_THICK_LINE: f32 = 2.5;
/// `PaintParameters.GRAPH_TEXT_GAP`.
pub const GRAPH_TEXT_GAP: f32 = 2.0;
/// `GraphCommitCellUtil.MAX_GRAPH_WIDTH`.
pub const GRAPH_MAX_WIDTH_ELEMENTS: f32 = 6.0;

/// `PaintParameters.scaleWithRowHeight`.
pub fn scale_with_row_height(value: f32, row_height: f32) -> f32 {
    value * row_height / GRAPH_BASE_ROW_HEIGHT
}

/// `JBUI.CurrentTheme.ToolWindow.defaultHeaderHeight()` (`ToolWindow.Header.height`).
pub const TOOL_WINDOW_HEADER_HEIGHT: f32 = 41.0;
/// `JBUI.CurrentTheme.ToolWindow.headerLabelLeftRightInsets()`: `insets(0, 12, 0, 16)`.
pub const TOOL_WINDOW_HEADER_LABEL_LEFT: f32 = 12.0;
pub const TOOL_WINDOW_HEADER_LABEL_RIGHT: f32 = 16.0;
/// `JBUI.CurrentTheme.ToolWindow.headerToolbarLeftRightInsets()`: `insets(0, 12, 0, 8)`.
pub const TOOL_WINDOW_HEADER_TOOLBAR_LEFT: f32 = 12.0;
pub const TOOL_WINDOW_HEADER_TOOLBAR_RIGHT: f32 = 8.0;
/// `JBUI.CurrentTheme.ToolWindow.headerTabLeftRightInsets()` in the New UI: `insets(0, 12)`.
pub const TOOL_WINDOW_HEADER_TAB_PADDING: f32 = 12.0;
/// `JBUI.CurrentTheme.ToolWindow.headerTabUnderlineArc()` (`ToolWindow.HeaderTab.underlineArc`).
pub const TOOL_WINDOW_HEADER_TAB_ARC: f32 = 4.0;
/// `JBUI.CurrentTheme.ToolWindow.underlineHeight()` (JBUI.java): `ToolWindow.HeaderTab.underlineHeight`,
/// defaulting to `DefaultTabs.UNDERLINE_HEIGHT` (`DefaultTabs.underlineHeight`, 3). Neither
/// `expUI_light.theme.json` nor `expUI_dark.theme.json` (nor their parents) sets the
/// `HeaderTab.underlineHeight` key (the `underlineHeight: 4` there is `EditorTabs`), so it is 3.
pub const TOOL_WINDOW_TAB_UNDERLINE_HEIGHT: f32 = 3.0;

/// `JBUI.CurrentTheme.Toolbar.defaultStripeToolbarButtonSize()`: 40 x 40.
pub const STRIPE_BUTTON_SIZE: f32 = 40.0;
/// `JBUI.CurrentTheme.Toolbar.defaultStripeToolbarButtonIconSize()`.
pub const STRIPE_BUTTON_ICON_SIZE: f32 = 20.0;
/// The stripe button's selection/hover rounded rectangle: `SquareStripeButtonLook.paintBackground`
/// removes `JBUI.CurrentTheme.Toolbar.stripeToolbarButtonIconPadding` (default `insets(5)`) from
/// the 40x40 button, leaving 30x30.
pub const STRIPE_BUTTON_HIGHLIGHT_SIZE: f32 = 30.0;
/// `SquareStripeButtonLook.getButtonArc()` = `JBUI.CurrentTheme.Toolbar.stripeButtonArc(compact)`
/// = `Button.ToolWindow.arc`, default 12 (8 in compact mode; the expUI themes do not set it).
/// Java arcs are diameters, so the corner radius is half.
pub const STRIPE_BUTTON_HIGHLIGHT_ARC: f32 = 12.0;
/// Stripe width: one button plus 1px separator (`ToolWindowToolbar`'s border).
pub const STRIPE_WIDTH: f32 = STRIPE_BUTTON_SIZE + 1.0;

/// Main toolbar (window header) height: `CustomHeader.kt` `HEADER_HEIGHT_NORMAL`.
pub const MAIN_TOOLBAR_HEIGHT: f32 = 40.0;

/// Status bar height. Upstream derives it from the label font plus widget insets; not ported
/// (IdeaGit estimate: 30; to be checked against the `docs/ui-reference` IntelliJ screenshots).
pub const STATUS_BAR_HEIGHT: f32 = 30.0;
/// Horizontal widget padding: `JBUI.CurrentTheme.StatusBar.hoverInsets()` (`insets(3, 4, 1, 4)`) plus
/// 2px gap (IdeaGit estimate).
pub const STATUS_BAR_WIDGET_PADDING: f32 = 6.0;

/// `LabelPainter` paddings (`JBValue`s, scaled with the UI): see `log` label painting.
pub mod label {
    pub const TOP_TEXT_PADDING: f32 = 1.0;
    pub const BOTTOM_TEXT_PADDING: f32 = 2.0;
    pub const RIGHT_PADDING: f32 = 4.0;
    pub const LEFT_PADDING: f32 = 4.0;
    pub const COMPACT_MIDDLE_PADDING: f32 = 6.0;
    pub const MIDDLE_PADDING: f32 = 12.0;
    pub const ICON_TEXT_PADDING: f32 = 1.0;
    pub const ICON_ADDITIONAL_PADDING: f32 = 3.0;
    pub const LABEL_ARC: f32 = 6.0;
    /// `LabelPainter.MAX_LENGTH`.
    pub const MAX_LENGTH: usize = 22;
    /// `LabelIcon.SIZE`.
    pub const ICON_SIZE_UNITS: f32 = 6.25;
    /// `GraphCommitCellRenderer.DISPLAYED_MESSAGE_PART`.
    pub const DISPLAYED_MESSAGE_PART: f32 = 80.0;
}

/// Default UI font size in the New UI (`UISettings` default font size 13).
pub const UI_FONT_SIZE: f32 = 13.0;
/// Editor/console font size (`EditorFontsConstants` default 13).
pub const MONO_FONT_SIZE: f32 = 13.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_height_never_below_the_new_ui_minimum() {
        assert_eq!(log_row_height(12.0), 26.0);
        assert_eq!(log_row_height(16.0), 26.0);
        assert_eq!(log_row_height(20.5), 28.0);
    }

    #[test]
    fn graph_metrics_scale_with_row_height() {
        assert_eq!(scale_with_row_height(GRAPH_WIDTH_NODE, 22.0), 16.0);
        assert_eq!(scale_with_row_height(GRAPH_WIDTH_NODE, 33.0), 24.0);
    }
}
