//! Printing API: color getters, the print element generator and the presentation manager.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/printer/`
//! (`GraphColorGetter.kt`, `GraphPrintElement.kt`, `PrintElementGenerator.java`,
//! `PrintElementPresentationManager.java`).
//!
//! Deviation: `GraphPrintElement` is an enum over the two concrete print element types instead of
//! an interface; UI code matches on it.

use std::sync::Arc;

use crate::api::elements::GraphElement;
use crate::api::permanent::{PermanentGraphInfo, VcsLogGraphNodeId};
use crate::print::elements::{EdgePrintElementImpl, SimplePrintElementImpl};
use crate::print_element::PrintElement;
use crate::visible_graph::VcsLogVisibleGraphIndex;

/// Generates color ids for graph nodes.
pub trait GraphColorGetter: Send + Sync {
    /// Color id for `node_id` (a [`PermanentCommitsInfo`](crate::api::permanent::PermanentCommitsInfo)
    /// node id) and `layout_index` (the fragment the node belongs to, see
    /// [`GraphLayout::get_layout_index`](crate::api::GraphLayout::get_layout_index)).
    fn get_node_color(&self, node_id: VcsLogGraphNodeId, layout_index: i32) -> i32;
}

/// A factory for [`GraphColorGetter`].
pub trait GraphColorGetterFactory<CommitId> {
    /// `permanent_graph_info` is used for node id to commit id conversion, layout indexes, etc.
    fn create_color_getter(
        &self,
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    ) -> Arc<dyn GraphColorGetter>;
}

/// A print element together with the graph element it was created for.
#[derive(Clone, Debug)]
pub enum GraphPrintElement {
    Node(SimplePrintElementImpl),
    Edge(EdgePrintElementImpl),
}

impl GraphPrintElement {
    pub fn graph_element(&self) -> GraphElement {
        match self {
            GraphPrintElement::Node(node) => GraphElement::Node(node.graph_node()),
            GraphPrintElement::Edge(edge) => GraphElement::Edge(edge.graph_edge()),
        }
    }

    /// `CascadeController.convertPrintElement`: this print element standing for `graph_element`
    /// (a node for a node, an edge for an edge; `None` otherwise).
    pub fn with_graph_element(&self, graph_element: GraphElement) -> Option<GraphPrintElement> {
        match (self, graph_element) {
            (GraphPrintElement::Node(node), GraphElement::Node(graph_node)) => {
                Some(GraphPrintElement::Node(node.with_graph_node(graph_node)))
            }
            (GraphPrintElement::Edge(edge), GraphElement::Edge(graph_edge)) => {
                Some(GraphPrintElement::Edge(edge.with_graph_edge(graph_edge)))
            }
            _ => None,
        }
    }

    fn as_print_element(&self) -> &dyn PrintElement {
        match self {
            GraphPrintElement::Node(node) => node,
            GraphPrintElement::Edge(edge) => edge,
        }
    }
}

impl PrintElement for GraphPrintElement {
    fn row_index(&self) -> VcsLogVisibleGraphIndex {
        self.as_print_element().row_index()
    }
    fn position_in_current_row(&self) -> i32 {
        self.as_print_element().position_in_current_row()
    }
    fn color_id(&self) -> i32 {
        self.as_print_element().color_id()
    }
    fn is_selected(&self) -> bool {
        self.as_print_element().is_selected()
    }
}

/// Upstream `equals`: nodes equal nodes at the same place, edges equal edges with the same
/// positions, direction and arrow; a node never equals an edge.
impl PartialEq for GraphPrintElement {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (GraphPrintElement::Node(a), GraphPrintElement::Node(b)) => a == b,
            (GraphPrintElement::Edge(a), GraphPrintElement::Edge(b)) => a == b,
            _ => false,
        }
    }
}

/// Produces the print elements of a visible row.
pub trait PrintElementGenerator {
    fn get_print_elements(&self, visible_row: VcsLogVisibleGraphIndex) -> Vec<GraphPrintElement>;
}

/// Decides colors and selection of print elements.
pub trait PrintElementPresentationManager {
    fn is_selected(&self, print_element: &GraphPrintElement) -> bool;

    fn get_color_id(&self, element: &GraphElement) -> i32;
}
