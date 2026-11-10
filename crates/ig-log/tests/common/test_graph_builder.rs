//! A small DSL for building linear graphs in tests.
//!
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/TestGraphBuilder.kt`
//! (`BaseTestGraphBuilder`, `TestGraphBuilder`, `graph { }`). Kotlin's `1(2, 3.dot)` becomes
//! `b.node(1, [u(2), dot(3)])`; `2.UNM(3)` becomes `b.node(unm(2), [u(3)])`. The
//! [`test_graph!`](crate::common::test_graph_builder::test_graph) macro accepts the Kotlin syntax
//! itself (`test_graph! { 1(2, 3.dot) 2.UNM(3) 3() }`). Also ports `asTestGraphString`.

use std::collections::HashMap;

use ig_log::api::EdgeFilter;
use ig_log::api::elements::{GraphEdge, GraphEdgeType, GraphElement, GraphNode, GraphNodeType};
use ig_log::api::linear_graph::LinearGraph;
use ig_log::utils::java_sort;
use ig_log::utils::linear_graph_utils::{is_edge_down, is_edge_up};

use super::str_utils::graph_element_comparator;

#[derive(Clone, Copy, Debug)]
pub struct SimpleEdge {
    pub to_node: Option<i32>,
    pub edge_type: GraphEdgeType,
}

#[derive(Clone, Copy, Debug)]
pub struct SimpleNode {
    pub node_id: i32,
    pub node_type: GraphNodeType,
}

impl From<i32> for SimpleNode {
    fn from(node_id: i32) -> Self {
        SimpleNode {
            node_id,
            node_type: GraphNodeType::Usual,
        }
    }
}

impl From<i32> for SimpleEdge {
    fn from(to_node: i32) -> Self {
        u(to_node)
    }
}

/// `Int.U`
pub fn usual(node_id: i32) -> SimpleNode {
    SimpleNode::from(node_id)
}
/// `Int.UNM`
pub fn unm(node_id: i32) -> SimpleNode {
    SimpleNode {
        node_id,
        node_type: GraphNodeType::Unmatched,
    }
}
/// `Int.NOT_LOAD`
pub fn not_load_node(node_id: i32) -> SimpleNode {
    SimpleNode {
        node_id,
        node_type: GraphNodeType::NotLoadCommit,
    }
}

fn edge(to_node: Option<i32>, edge_type: GraphEdgeType) -> SimpleEdge {
    SimpleEdge { to_node, edge_type }
}
/// `Int.u`
pub fn u(to_node: i32) -> SimpleEdge {
    edge(Some(to_node), GraphEdgeType::Usual)
}
/// `Int.dot`
pub fn dot(to_node: i32) -> SimpleEdge {
    edge(Some(to_node), GraphEdgeType::Dotted)
}
/// `Int?.up_dot`
pub fn up_dot(to_node: Option<i32>) -> SimpleEdge {
    edge(to_node, GraphEdgeType::DottedArrowUp)
}
/// `Int?.down_dot`
pub fn down_dot(to_node: Option<i32>) -> SimpleEdge {
    edge(to_node, GraphEdgeType::DottedArrowDown)
}
/// `Int?.not_load`
pub fn not_load(to_node: Option<i32>) -> SimpleEdge {
    edge(to_node, GraphEdgeType::NotLoadCommit)
}

struct NodeWithEdges {
    node_id: i32,
    edges: Vec<SimpleEdge>,
    node_type: GraphNodeType,
}

#[derive(Default)]
pub struct TestGraphBuilder {
    nodes: Vec<NodeWithEdges>,
}

impl TestGraphBuilder {
    /// `node(edges...)`
    pub fn node<E: Into<SimpleEdge>>(
        &mut self,
        node: impl Into<SimpleNode>,
        edges: impl IntoIterator<Item = E>,
    ) {
        let node = node.into();
        self.nodes.push(NodeWithEdges {
            node_id: node.node_id,
            edges: edges.into_iter().map(Into::into).collect(),
            node_type: node.node_type,
        });
    }

    pub fn done(self) -> TestLinearGraph {
        TestLinearGraph::new(self.nodes)
    }
}

/// `graph { ... }` with the Kotlin DSL's syntax: `0(1, 2.dot)`, `2.UNM(3)`, `4.NOT_LOAD()`, `3()`.
/// Edges are `n` / `n.u` (usual) and `n.dot` (dotted).
#[allow(unused_macros)]
macro_rules! test_graph {
    ($($nodes:tt)*) => {{
        #[allow(unused_mut)]
        let mut builder = $crate::common::test_graph_builder::TestGraphBuilder::default();
        $crate::common::test_graph_builder::test_graph_nodes!(builder; $($nodes)*);
        builder.done()
    }};
}
#[allow(unused_imports)]
pub(crate) use test_graph;

#[allow(unused_macros)]
macro_rules! test_graph_nodes {
    ($b:ident;) => {};
    ($b:ident; $id:literal . UNM ($($edges:tt)*) $($rest:tt)*) => {
        $b.node(
            $crate::common::test_graph_builder::unm($id),
            $crate::common::test_graph_builder::test_graph_edges!($($edges)*),
        );
        $crate::common::test_graph_builder::test_graph_nodes!($b; $($rest)*);
    };
    ($b:ident; $id:literal . NOT_LOAD ($($edges:tt)*) $($rest:tt)*) => {
        $b.node(
            $crate::common::test_graph_builder::not_load_node($id),
            $crate::common::test_graph_builder::test_graph_edges!($($edges)*),
        );
        $crate::common::test_graph_builder::test_graph_nodes!($b; $($rest)*);
    };
    ($b:ident; $id:literal ($($edges:tt)*) $($rest:tt)*) => {
        $b.node(
            $id,
            $crate::common::test_graph_builder::test_graph_edges!($($edges)*),
        );
        $crate::common::test_graph_builder::test_graph_nodes!($b; $($rest)*);
    };
}
#[allow(unused_imports)]
pub(crate) use test_graph_nodes;

#[allow(unused_macros)]
macro_rules! test_graph_edges {
    () => {
        ::std::vec::Vec::<$crate::common::test_graph_builder::SimpleEdge>::new()
    };
    ($($id:literal $(. $kind:ident)?),+ $(,)?) => {
        ::std::vec![$($crate::common::test_graph_builder::test_graph_edges!(@edge $id $(. $kind)?)),+]
    };
    (@edge $id:literal) => {
        $crate::common::test_graph_builder::u($id)
    };
    (@edge $id:literal . u) => {
        $crate::common::test_graph_builder::u($id)
    };
    (@edge $id:literal . dot) => {
        $crate::common::test_graph_builder::dot($id)
    };
}
#[allow(unused_imports)]
pub(crate) use test_graph_edges;

/// `graph { ... }`
pub fn graph(f: impl FnOnce(&mut TestGraphBuilder)) -> TestLinearGraph {
    let mut builder = TestGraphBuilder::default();
    f(&mut builder);
    builder.done()
}

pub struct TestLinearGraph {
    nodes: Vec<GraphNode>,
    node_index_to_id: HashMap<i32, i32>,
    node_id_to_index: HashMap<i32, i32>,
    edges: HashMap<i32, Vec<GraphEdge>>,
}

impl TestLinearGraph {
    fn new(build_nodes: Vec<NodeWithEdges>) -> TestLinearGraph {
        let mut node_index_to_id = HashMap::new();
        let nodes = build_nodes
            .iter()
            .enumerate()
            .map(|(index, it)| {
                node_index_to_id.insert(index as i32, it.node_id);
                GraphNode::with_type(index as i32, it.node_type)
            })
            .collect();
        let node_id_to_index: HashMap<i32, i32> =
            node_index_to_id.iter().map(|(&k, &v)| (v, k)).collect();

        // create edges
        let mut edges: HashMap<i32, Vec<GraphEdge>> = HashMap::new();
        for node in &build_nodes {
            let node_index = node_id_to_index[&node.node_id];
            for simple_edge in &node.edges {
                let edge_type = simple_edge.edge_type;

                if edge_type.is_normal_edge() {
                    let another_node_index = simple_edge
                        .to_node
                        .and_then(|to| node_id_to_index.get(&to).copied())
                        .unwrap_or_else(|| {
                            panic!(
                                "Graph is incorrect. Node {} has {edge_type:?} edge to not existed node: {:?}",
                                node.node_id, simple_edge.to_node
                            )
                        });
                    let graph_edge =
                        GraphEdge::create_normal_edge(another_node_index, node_index, edge_type);
                    edges.entry(node_index).or_default().push(graph_edge);
                    edges
                        .entry(another_node_index)
                        .or_default()
                        .push(graph_edge);
                } else {
                    edges.entry(node_index).or_default().push(
                        GraphEdge::create_edge_with_target_id(
                            node_index,
                            simple_edge.to_node,
                            edge_type,
                        ),
                    );
                }
            }
        }

        TestLinearGraph {
            nodes,
            node_index_to_id,
            node_id_to_index,
            edges,
        }
    }
}

impl LinearGraph for TestLinearGraph {
    fn nodes_count(&self) -> i32 {
        self.nodes.len() as i32
    }

    fn get_node_id(&self, node_index: i32) -> i32 {
        self.node_index_to_id[&node_index]
    }

    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        self.edges
            .get(&node_index)
            .into_iter()
            .flatten()
            .filter(|it| {
                if it.edge_type.is_normal_edge() {
                    (is_edge_up(it, node_index) && filter.up_normal())
                        || (is_edge_down(it, node_index) && filter.down_normal())
                } else {
                    filter.special()
                }
            })
            .copied()
            .collect()
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        self.nodes[node_index as usize]
    }

    fn get_node_index(&self, node_id: i32) -> Option<i32> {
        self.node_id_to_index.get(&node_id).copied()
    }
}

/// `LinearGraph.assertEdge`: the edge is consistent with `node_index` and the other end's edges.
fn assert_edge(graph: &(impl LinearGraph + ?Sized), node_index: i32, edge: &GraphEdge) {
    if edge.edge_type.is_normal_edge() {
        if Some(node_index) == edge.up_node_index {
            assert!(
                graph
                    .get_adjacent_edges(edge.down_node_index.unwrap(), EdgeFilter::NormalUp)
                    .contains(edge)
            );
        } else {
            assert_eq!(Some(node_index), edge.down_node_index);
            assert!(
                graph
                    .get_adjacent_edges(edge.up_node_index.unwrap(), EdgeFilter::NormalDown)
                    .contains(edge)
            );
        }
    } else {
        match edge.edge_type {
            GraphEdgeType::NotLoadCommit | GraphEdgeType::DottedArrowDown => {
                assert_eq!(Some(node_index), edge.up_node_index);
                assert_eq!(None, edge.down_node_index);
            }
            GraphEdgeType::DottedArrowUp => {
                assert_eq!(Some(node_index), edge.down_node_index);
                assert_eq!(None, edge.up_node_index);
            }
            _ => {}
        }
    }
}

/// `LinearGraph.asTestGraphString(sorted)`: the graph in the `graph { }` DSL's notation, one node
/// per line (`id.UNM(edges)`), with the down edges of each node.
pub fn as_test_graph_string(graph: &(impl LinearGraph + ?Sized), sorted: bool) -> String {
    let mut s = String::new();
    for node_index in 0..graph.nodes_count() {
        let node = graph.get_graph_node(node_index);
        s.push_str(&graph.get_node_id(node_index).to_string());
        assert_eq!(
            node_index,
            node.node_index,
            "nodeIndex: {node_index}, but for node with this index(nodeId: {}) nodeIndex: {}",
            graph.get_node_id(node_index),
            node.node_index
        );
        match node.node_type {
            GraphNodeType::Unmatched => s.push_str(".UNM"),
            GraphNodeType::NotLoadCommit => s.push_str(".NOT_LOAD"),
            GraphNodeType::Usual => {}
        }

        // edges
        let mut adjacent_edges = graph.get_adjacent_edges(node_index, EdgeFilter::All);
        if sorted {
            let comparator = graph_element_comparator();
            let mut elements: Vec<GraphElement> =
                adjacent_edges.into_iter().map(GraphElement::Edge).collect();
            java_sort::sort_by(&mut elements, |a, b| comparator.ordering(a, b));
            adjacent_edges = elements
                .into_iter()
                .map(|element| match element {
                    GraphElement::Edge(edge) => edge,
                    GraphElement::Node(_) => unreachable!(),
                })
                .collect();
        }

        s.push('(');
        let edges: Vec<String> = adjacent_edges
            .iter()
            .filter_map(|it| {
                assert_edge(graph, node_index, it);
                if it.up_node_index == Some(node_index) {
                    let start_id = if it.edge_type.is_normal_edge() {
                        graph.get_node_id(it.down_node_index.unwrap()).to_string()
                    } else if let Some(target_id) = it.target_id {
                        target_id.to_string()
                    } else {
                        "null".to_string()
                    };

                    Some(match it.edge_type {
                        GraphEdgeType::Usual => start_id,
                        GraphEdgeType::Dotted => format!("{start_id}.dot"),
                        GraphEdgeType::DottedArrowUp => format!("{start_id}.up_dot"),
                        GraphEdgeType::DottedArrowDown => format!("{start_id}.down_dot"),
                        GraphEdgeType::NotLoadCommit => format!("{start_id}.not_load"),
                    })
                } else {
                    None
                }
            })
            .collect();
        s.push_str(&edges.join(", "));

        s.push(')');
        s.push('\n');
    }
    s
}
