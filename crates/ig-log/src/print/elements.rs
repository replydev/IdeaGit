//! Concrete print elements produced by [`PrintElementGeneratorImpl`](super::print_element_generator_impl::PrintElementGeneratorImpl).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/elements/`
//! (`PrintElementBase.kt`, `SimplePrintElementImpl.kt`, `EdgePrintElementImpl.kt`,
//! `TerminalEdgePrintElement.java`).
//!
//! Deviation: upstream elements ask the presentation manager for `colorId` / `isSelected` on every
//! access; here both are computed once, when the generator creates the element. Elements are
//! created afresh on every `get_print_elements` call, so they reflect the selection at that time.

use crate::api::elements::{GraphEdge, GraphEdgeType, GraphElement, GraphNode};
use crate::api::printer::PrintElementPresentationManager;
use crate::print_element::{
    EdgePrintElement, EdgePrintElementType, LineStyle, NodePrintElement, PrintElement,
};
use crate::visible_graph::VcsLogVisibleGraphIndex;

/// A node print element (`SimplePrintElementImpl`).
#[derive(Clone, Debug)]
pub struct SimplePrintElementImpl {
    row_index: VcsLogVisibleGraphIndex,
    position_in_current_row: i32,
    graph_node: GraphNode,
    color_id: i32,
    is_selected: bool,
}

impl SimplePrintElementImpl {
    /// Creates the element; color and selection come from `presentation_manager`.
    pub fn new(
        row_index: VcsLogVisibleGraphIndex,
        position_in_current_row: i32,
        graph_node: GraphNode,
        presentation_manager: &dyn PrintElementPresentationManager,
    ) -> SimplePrintElementImpl {
        let mut element = SimplePrintElementImpl {
            row_index,
            position_in_current_row,
            graph_node,
            color_id: presentation_manager.get_color_id(&GraphElement::Node(graph_node)),
            is_selected: false,
        };
        element.is_selected = presentation_manager.is_selected(
            &crate::api::printer::GraphPrintElement::Node(element.clone()),
        );
        element
    }

    pub fn graph_node(&self) -> GraphNode {
        self.graph_node
    }
}

impl PrintElement for SimplePrintElementImpl {
    fn row_index(&self) -> VcsLogVisibleGraphIndex {
        self.row_index
    }
    fn position_in_current_row(&self) -> i32 {
        self.position_in_current_row
    }
    fn color_id(&self) -> i32 {
        self.color_id
    }
    fn is_selected(&self) -> bool {
        self.is_selected
    }
}

impl NodePrintElement for SimplePrintElementImpl {}

/// Upstream equality: same row and position (graph element, color and selection are ignored).
impl PartialEq for SimplePrintElementImpl {
    fn eq(&self, other: &Self) -> bool {
        self.position_in_current_row == other.position_in_current_row
            && self.row_index == other.row_index
    }
}

/// An edge print element (`EdgePrintElementImpl`); a `TerminalEdgePrintElement` is one created by
/// [`EdgePrintElementImpl::terminal`].
#[derive(Clone, Debug)]
pub struct EdgePrintElementImpl {
    row_index: VcsLogVisibleGraphIndex,
    position_in_current_row: i32,
    position_in_other_row: i32,
    edge_print_type: EdgePrintElementType,
    graph_edge: GraphEdge,
    has_arrow: bool,
    line_style: LineStyle,
    color_id: i32,
    is_selected: bool,
    is_terminal: bool,
}

impl EdgePrintElementImpl {
    /// Creates the element; color and selection come from `presentation_manager`.
    pub fn new(
        row_index: VcsLogVisibleGraphIndex,
        position_in_current_row: i32,
        position_in_other_row: i32,
        edge_print_type: EdgePrintElementType,
        graph_edge: GraphEdge,
        has_arrow: bool,
        presentation_manager: &dyn PrintElementPresentationManager,
    ) -> EdgePrintElementImpl {
        let mut element = EdgePrintElementImpl {
            row_index,
            position_in_current_row,
            position_in_other_row,
            edge_print_type,
            graph_edge,
            has_arrow,
            line_style: Self::convert_to_line_style(graph_edge.edge_type),
            color_id: presentation_manager.get_color_id(&GraphElement::Edge(graph_edge)),
            is_selected: false,
            is_terminal: false,
        };
        element.is_selected = presentation_manager.is_selected(
            &crate::api::printer::GraphPrintElement::Edge(element.clone()),
        );
        element
    }

    /// `TerminalEdgePrintElement`: an arrow that ends in this row (other position = own position).
    pub fn terminal(
        row_index: VcsLogVisibleGraphIndex,
        position: i32,
        edge_print_type: EdgePrintElementType,
        graph_edge: GraphEdge,
        presentation_manager: &dyn PrintElementPresentationManager,
    ) -> EdgePrintElementImpl {
        EdgePrintElementImpl {
            is_terminal: true,
            ..EdgePrintElementImpl::new(
                row_index,
                position,
                position,
                edge_print_type,
                graph_edge,
                true,
                presentation_manager,
            )
        }
    }

    /// Whether this is a `TerminalEdgePrintElement` (an arrow ending in this row). The painter
    /// (`SimpleGraphCellPainter`) leaves a gap before a terminal arrow; equality ignores it, as
    /// upstream's `EdgePrintElementImpl.equals` does.
    pub fn is_terminal(&self) -> bool {
        self.is_terminal
    }

    pub fn graph_edge(&self) -> GraphEdge {
        self.graph_edge
    }

    pub fn convert_to_line_style(edge_type: GraphEdgeType) -> LineStyle {
        match edge_type {
            GraphEdgeType::Usual | GraphEdgeType::NotLoadCommit => LineStyle::Solid,
            GraphEdgeType::Dotted
            | GraphEdgeType::DottedArrowUp
            | GraphEdgeType::DottedArrowDown => LineStyle::Dashed,
        }
    }
}

impl PrintElement for EdgePrintElementImpl {
    fn row_index(&self) -> VcsLogVisibleGraphIndex {
        self.row_index
    }
    fn position_in_current_row(&self) -> i32 {
        self.position_in_current_row
    }
    fn color_id(&self) -> i32 {
        self.color_id
    }
    fn is_selected(&self) -> bool {
        self.is_selected
    }
}

impl EdgePrintElement for EdgePrintElementImpl {
    fn position_in_other_row(&self) -> i32 {
        self.position_in_other_row
    }
    fn edge_print_type(&self) -> EdgePrintElementType {
        self.edge_print_type
    }
    fn line_style(&self) -> LineStyle {
        self.line_style
    }
    fn has_arrow(&self) -> bool {
        self.has_arrow
    }
}

/// Upstream equality: positions, row, type and arrow (graph edge, style, color and selection are ignored).
impl PartialEq for EdgePrintElementImpl {
    fn eq(&self, other: &Self) -> bool {
        self.position_in_current_row == other.position_in_current_row
            && self.position_in_other_row == other.position_in_other_row
            && self.row_index == other.row_index
            && self.edge_print_type == other.edge_print_type
            && self.has_arrow == other.has_arrow
    }
}
