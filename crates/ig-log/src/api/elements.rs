//! Graph elements: nodes and edges of a [`LinearGraph`](crate::api::linear_graph::LinearGraph).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/elements/`
//! (`GraphElement.java`, `GraphNode.java`, `GraphNodeType.java`, `GraphEdge.java`, `GraphEdgeType.java`).

/// Kind of a [`GraphEdge`]. Ordinals match upstream (they are packed into
/// [`EdgeStorage`](crate::collapsing::edge_storage::EdgeStorage)).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GraphEdgeType {
    /// Between visible delegate nodes.
    Usual,
    /// Collapsed fragment.
    Dotted,
    /// Edge to a commit that is not loaded.
    NotLoadCommit,
    DottedArrowUp,
    DottedArrowDown,
}

impl GraphEdgeType {
    /// All values in ordinal order (`GraphEdgeType.values()`).
    pub const VALUES: [GraphEdgeType; 5] = [
        GraphEdgeType::Usual,
        GraphEdgeType::Dotted,
        GraphEdgeType::NotLoadCommit,
        GraphEdgeType::DottedArrowUp,
        GraphEdgeType::DottedArrowDown,
    ];

    /// Normal edges connect two nodes; the others have one node and a target id.
    pub fn is_normal_edge(self) -> bool {
        matches!(self, GraphEdgeType::Usual | GraphEdgeType::Dotted)
    }

    /// Java `Enum.ordinal()`.
    pub fn ordinal(self) -> i32 {
        self as i32
    }
}

/// Kind of a [`GraphNode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GraphNodeType {
    Usual,
    NotLoadCommit,
    Unmatched,
}

/// A node of a linear graph, identified by its index (row) in that graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GraphNode {
    pub node_index: i32,
    pub node_type: GraphNodeType,
}

impl GraphNode {
    /// A [`GraphNodeType::Usual`] node.
    pub fn new(node_index: i32) -> GraphNode {
        GraphNode::with_type(node_index, GraphNodeType::Usual)
    }

    pub fn with_type(node_index: i32, node_type: GraphNodeType) -> GraphNode {
        GraphNode {
            node_index,
            node_type,
        }
    }
}

/// An edge of a linear graph. Normal edges have both node indexes; special edges have one node index
/// and an optional target id. Equality covers all four fields, like upstream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GraphEdge {
    pub up_node_index: Option<i32>,
    pub down_node_index: Option<i32>,
    pub target_id: Option<i32>,
    pub edge_type: GraphEdgeType,
}

impl GraphEdge {
    pub fn new(
        up_node_index: Option<i32>,
        down_node_index: Option<i32>,
        target_id: Option<i32>,
        edge_type: GraphEdgeType,
    ) -> GraphEdge {
        GraphEdge {
            up_node_index,
            down_node_index,
            target_id,
            edge_type,
        }
    }

    /// `GraphEdge.createNormalEdge`: orders the two node indexes so the smaller one is up.
    pub fn create_normal_edge(
        node_index1: i32,
        node_index2: i32,
        edge_type: GraphEdgeType,
    ) -> GraphEdge {
        debug_assert!(
            edge_type.is_normal_edge(),
            "Unexpected edge type: {edge_type:?}"
        );
        GraphEdge::new(
            Some(node_index1.min(node_index2)),
            Some(node_index1.max(node_index2)),
            None,
            edge_type,
        )
    }

    /// `GraphEdge.createEdgeWithTargetId`.
    ///
    /// # Panics
    /// If `edge_type` is a normal edge type (upstream throws `AssertionError`).
    pub fn create_edge_with_target_id(
        node_index: i32,
        target_id: Option<i32>,
        edge_type: GraphEdgeType,
    ) -> GraphEdge {
        match edge_type {
            GraphEdgeType::DottedArrowUp => {
                GraphEdge::new(None, Some(node_index), target_id, edge_type)
            }
            GraphEdgeType::NotLoadCommit | GraphEdgeType::DottedArrowDown => {
                GraphEdge::new(Some(node_index), None, target_id, edge_type)
            }
            _ => panic!("Unexpected edge type: {edge_type:?}"),
        }
    }
}

/// A node or an edge (`GraphElement` marker interface).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GraphElement {
    Node(GraphNode),
    Edge(GraphEdge),
}

impl From<GraphNode> for GraphElement {
    fn from(node: GraphNode) -> Self {
        GraphElement::Node(node)
    }
}

impl From<GraphEdge> for GraphElement {
    fn from(edge: GraphEdge) -> Self {
        GraphElement::Edge(edge)
    }
}
