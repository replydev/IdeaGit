//! Checks that a sorted graph is still topologically sorted (every edge goes down).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/SortChecker.java`.
//!
//! Deviation: upstream's `LOG.error` (which fails tests and only logs in production) is a
//! `debug_assert!`.

use crate::api::linear_graph::LinearGraph;
use crate::utils::linear_graph_utils::{get_down_nodes, get_up_nodes};

pub fn check_linear_graph(linear_graph: &(impl LinearGraph + ?Sized)) {
    let reversed_edge = find_reversed_edge(linear_graph);
    debug_assert!(
        reversed_edge.is_none(),
        "Illegal edge: up node {:?}, downNode {:?}",
        reversed_edge.map(|e| e.0),
        reversed_edge.map(|e| e.1)
    );
}

/// The first edge `(up node, down node)` whose up node is not above its down node.
pub fn find_reversed_edge(linear_graph: &(impl LinearGraph + ?Sized)) -> Option<(i32, i32)> {
    for i in 0..linear_graph.nodes_count() {
        for down_node in get_down_nodes(linear_graph, i) {
            if down_node <= i {
                return Some((i, down_node));
            }
        }

        for up_node in get_up_nodes(linear_graph, i) {
            if up_node >= i {
                return Some((up_node, i));
            }
        }
    }
    None
}
