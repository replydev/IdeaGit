//! Elements of the visible commit graph, as drawn by the UI.
//!
//! Ported from `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/PrintElement.kt`,
//! `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/NodePrintElement.java` and
//! `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/EdgePrintElement.kt`.

use crate::visible_graph::VcsLogVisibleGraphIndex;

/// Element in the visible commit graph.
pub trait PrintElement {
    fn row_index(&self) -> VcsLogVisibleGraphIndex;
    fn position_in_current_row(&self) -> i32;
    fn color_id(&self) -> i32;
    fn is_selected(&self) -> bool;
}

/// How a node is painted (`NodePrintElement.Type`, experimental upstream).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum NodePrintElementType {
    #[default]
    Fill,
    Outline,
    OutlineAndFill,
}

/// A commit node.
pub trait NodePrintElement: PrintElement {
    fn node_type(&self) -> NodePrintElementType {
        NodePrintElementType::Fill
    }
}

/// Direction of an edge part, relative to the row it is drawn in (`EdgePrintElement.Type`).
/// Ordinals matter for the test printer: `Up` = 0, `Down` = 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EdgePrintElementType {
    Up,
    Down,
}

impl EdgePrintElementType {
    pub fn ordinal(self) -> i32 {
        self as i32
    }

    /// Upstream enum constant name.
    pub fn name(self) -> &'static str {
        match self {
            EdgePrintElementType::Up => "UP",
            EdgePrintElementType::Down => "DOWN",
        }
    }
}

/// `EdgePrintElement.LineStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
}

impl LineStyle {
    /// Upstream enum constant name.
    pub fn name(self) -> &'static str {
        match self {
            LineStyle::Solid => "SOLID",
            LineStyle::Dashed => "DASHED",
            LineStyle::Dotted => "DOTTED",
        }
    }
}

/// Half of an edge drawn in one row, going up or down to `position_in_other_row`.
pub trait EdgePrintElement: PrintElement {
    fn position_in_other_row(&self) -> i32;
    fn edge_print_type(&self) -> EdgePrintElementType;
    fn line_style(&self) -> LineStyle;
    fn has_arrow(&self) -> bool;
}
