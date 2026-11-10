//! Linear fragments: the part of the graph between a node and the next node all of whose paths
//! meet again (what a hover highlights and a click collapses).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/LinearFragmentGenerator.java`.
//!
//! Deviation: the gray nodes of `getFragment` are a `BTreeSet` (ascending order; Java iterates its
//! `HashSet<Integer>` in ascending order for small node indexes). The fragment found does not
//! depend on that order.

use std::collections::{BTreeSet, HashSet};

use crate::api::elements::GraphElement;
use crate::api::linear_graph::{LiteLinearGraph, NodeFilter};
use crate::utils::linear_graph_utils::as_normal_edge;

const SHORT_FRAGMENT_MAX_SIZE: usize = 10;
const MAX_SEARCH_SIZE: usize = 10;

/// A fragment between two node indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphFragment {
    pub up_node_index: i32,
    pub down_node_index: i32,
}

impl GraphFragment {
    pub fn new(up_node_index: i32, down_node_index: i32) -> GraphFragment {
        GraphFragment {
            up_node_index,
            down_node_index,
        }
    }

    pub fn create(start_node: i32, end_node: i32) -> GraphFragment {
        GraphFragment::new(start_node, end_node)
    }
}

pub struct LinearFragmentGenerator<'g, G: ?Sized> {
    linear_graph: &'g G,
    pinned_nodes: HashSet<i32>,
}

impl<'g, G: LiteLinearGraph + ?Sized> LinearFragmentGenerator<'g, G> {
    pub fn new(linear_graph: &'g G, pinned_nodes: HashSet<i32>) -> LinearFragmentGenerator<'g, G> {
        LinearFragmentGenerator {
            linear_graph,
            pinned_nodes,
        }
    }

    fn up_nodes(&self, node: i32) -> Vec<i32> {
        self.linear_graph.get_nodes(node, NodeFilter::Up)
    }

    fn down_nodes(&self, node: i32) -> Vec<i32> {
        self.linear_graph.get_nodes(node, NodeFilter::Down)
    }

    pub fn get_relative_fragment(&self, element: &GraphElement) -> Option<GraphFragment> {
        let (mut up_node_index, down_node_index) = match element {
            GraphElement::Node(node) => (node.node_index, node.node_index),
            GraphElement::Edge(edge) => {
                let graph_edge = as_normal_edge(edge)?;
                (graph_edge.up, graph_edge.down)
            }
        };

        for _ in 0..MAX_SEARCH_SIZE {
            let graph_fragment = self.get_down_fragment(up_node_index);
            if let Some(graph_fragment) = graph_fragment
                && graph_fragment.down_node_index >= down_node_index
            {
                return Some(graph_fragment);
            }

            let up_nodes = self.up_nodes(up_node_index);
            if up_nodes.len() != 1 {
                break;
            }
            up_node_index = up_nodes[0];
        }

        None
    }

    pub fn get_down_fragment(&self, upper_visible_node_index: i32) -> Option<GraphFragment> {
        self.get_fragment(upper_visible_node_index, true)
    }

    pub fn get_up_fragment(&self, lower_node_index: i32) -> Option<GraphFragment> {
        self.get_fragment(lower_node_index, false)
    }

    pub fn get_long_down_fragment(&self, row_index: i32) -> Option<GraphFragment> {
        self.get_long_fragment_bounded(self.get_down_fragment(row_index), i32::MAX)
    }

    pub fn get_long_fragment(&self, element: &GraphElement) -> Option<GraphFragment> {
        self.get_long_fragment_bounded(self.get_relative_fragment(element), i32::MAX)
    }

    /// For hover.
    pub fn get_part_long_fragment(&self, element: &GraphElement) -> Option<GraphFragment> {
        self.get_long_fragment_bounded(self.get_relative_fragment(element), 500)
    }

    fn get_long_fragment_bounded(
        &self,
        start_fragment: Option<GraphFragment>,
        bound: i32,
    ) -> Option<GraphFragment> {
        let start_fragment = start_fragment?;

        let mut max_down = start_fragment.down_node_index;
        while let Some(short_fragment) = self.get_down_fragment(max_down) {
            if self.pinned_nodes.contains(&max_down) {
                break;
            }
            max_down = short_fragment.down_node_index;
            if max_down - start_fragment.down_node_index > bound {
                break;
            }
        }

        let mut max_up = start_fragment.up_node_index;
        while let Some(short_fragment) = self.get_up_fragment(max_up) {
            if self.pinned_nodes.contains(&max_up) {
                break;
            }
            max_up = short_fragment.up_node_index;
            if start_fragment.up_node_index - max_up > bound {
                break;
            }
        }

        if max_up != start_fragment.up_node_index || max_down != start_fragment.down_node_index {
            return Some(GraphFragment::new(max_up, max_down));
        }
        // start fragment is Simple
        if self.down_nodes(start_fragment.up_node_index).len() != 1 {
            return Some(start_fragment);
        }
        None
    }

    fn get_fragment(&self, start_node: i32, is_down: bool) -> Option<GraphFragment> {
        let get_next_nodes = |node| {
            if is_down {
                self.down_nodes(node)
            } else {
                self.up_nodes(node)
            }
        };
        let get_prev_nodes = |node| {
            if is_down {
                self.up_nodes(node)
            } else {
                self.down_nodes(node)
            }
        };
        let this_node_cant_be_in_middle = &self.pinned_nodes;

        let mut black_nodes = HashSet::new();
        black_nodes.insert(start_node);

        let mut gray_nodes: BTreeSet<i32> = get_next_nodes(start_node).into_iter().collect();

        let mut end_node = -1;
        while black_nodes.len() < SHORT_FRAGMENT_MAX_SIZE {
            let mut next_black_node = -1;
            for &gray_node in &gray_nodes {
                if get_prev_nodes(gray_node)
                    .iter()
                    .all(|prev| black_nodes.contains(prev))
                {
                    next_black_node = gray_node;
                    break;
                }
            }

            if next_black_node == -1 {
                return None;
            }

            if gray_nodes.len() == 1 {
                end_node = next_black_node;
                break;
            }

            let next_gray_nodes = get_next_nodes(next_black_node);
            if next_gray_nodes.is_empty() || this_node_cant_be_in_middle.contains(&next_black_node)
            {
                return None;
            }

            black_nodes.insert(next_black_node);
            gray_nodes.remove(&next_black_node);
            gray_nodes.extend(next_gray_nodes);
        }

        if end_node != -1 {
            Some(if is_down {
                GraphFragment::create(start_node, end_node)
            } else {
                GraphFragment::create(end_node, start_node)
            })
        } else {
            None
        }
    }
}
