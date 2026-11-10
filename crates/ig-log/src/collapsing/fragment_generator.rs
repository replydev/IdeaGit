//! Fragments between "red" nodes: the nodes between two nodes, the nearest red node, and the
//! green (collapsible) fragment around a node.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/FragmentGenerator.java`.

use std::collections::HashSet;

use crate::api::linear_graph::{LiteLinearGraph, NodeFilter};
use crate::collapsing::tree_set_node_iterator::TreeSetNodeIterator;

/// A fragment of non-red nodes between up to two red nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenFragment {
    up_red_node: Option<i32>,
    down_red_node: Option<i32>,
    middle_green_nodes: HashSet<i32>,
}

impl GreenFragment {
    pub fn up_red_node(&self) -> Option<i32> {
        self.up_red_node
    }

    pub fn down_red_node(&self) -> Option<i32> {
        self.down_red_node
    }

    pub fn middle_green_nodes(&self) -> &HashSet<i32> {
        &self.middle_green_nodes
    }
}

pub struct FragmentGenerator<'g, G: ?Sized, R> {
    graph: &'g G,
    red_nodes: R,
}

impl<'g, G: LiteLinearGraph + ?Sized, R: Fn(i32) -> bool> FragmentGenerator<'g, G, R> {
    pub fn new(graph: &'g G, red_nodes: R) -> FragmentGenerator<'g, G, R> {
        FragmentGenerator { graph, red_nodes }
    }

    /// Nodes on paths from `up_node` down to `down_node` (`strict`: without the two ends).
    pub fn get_middle_nodes(&self, up_node: i32, down_node: i32, strict: bool) -> HashSet<i32> {
        let mut down_walk = self.get_walk_nodes(up_node, false, |integer| integer > down_node);
        let up_walk = self.get_walk_nodes(down_node, true, |integer| integer < up_node);

        down_walk.retain(|node| up_walk.contains(node));
        if strict {
            down_walk.remove(&up_node);
            down_walk.remove(&down_node);
        }
        down_walk
    }

    pub fn get_near_red_node(
        &self,
        start_node: i32,
        mut max_walk_size: i32,
        is_up: bool,
    ) -> Option<i32> {
        if (self.red_nodes)(start_node) {
            return Some(start_node);
        }

        let mut walker = TreeSetNodeIterator::new(start_node, is_up);
        while walker.not_empty() {
            let next = walker.pop();

            if (self.red_nodes)(next) {
                return Some(next);
            }

            if max_walk_size < 0 {
                return None;
            }
            max_walk_size -= 1;

            walker.add_all(&self.get_nodes(next, is_up));
        }

        None
    }

    pub fn get_green_fragment_for_collapse(
        &self,
        start_node: i32,
        max_walk_size: i32,
    ) -> GreenFragment {
        if (self.red_nodes)(start_node) {
            return GreenFragment {
                up_red_node: None,
                down_red_node: None,
                middle_green_nodes: HashSet::new(),
            };
        }
        let up_red_node = self.get_near_red_node(start_node, max_walk_size, true);
        let down_red_node = self.get_near_red_node(start_node, max_walk_size, false);

        let up_part = match up_red_node {
            Some(up_red_node) => self.get_middle_nodes(up_red_node, start_node, false),
            None => self.get_walk_nodes(start_node, true, create_stop_function(max_walk_size)),
        };

        let down_part = match down_red_node {
            Some(down_red_node) => self.get_middle_nodes(start_node, down_red_node, false),
            None => self.get_walk_nodes(start_node, false, create_stop_function(max_walk_size)),
        };

        let middle_nodes = up_part
            .union(&down_part)
            .copied()
            .filter(|&i| Some(i) != up_red_node && Some(i) != down_red_node)
            .collect();
        GreenFragment {
            up_red_node,
            down_red_node,
            middle_green_nodes: middle_nodes,
        }
    }

    fn get_walk_nodes(
        &self,
        start_node: i32,
        is_up: bool,
        mut stop_function: impl FnMut(i32) -> bool,
    ) -> HashSet<i32> {
        let mut walk_nodes = HashSet::new();

        let mut walker = TreeSetNodeIterator::new(start_node, is_up);
        while walker.not_empty() {
            let next = walker.pop();
            if !stop_function(next) {
                walk_nodes.insert(next);
                walker.add_all(&self.get_nodes(next, is_up));
            }
        }

        walk_nodes
    }

    fn get_nodes(&self, node_index: i32, is_up: bool) -> Vec<i32> {
        self.graph.get_nodes(node_index, NodeFilter::filter(is_up))
    }
}

/// Stops after `max_node_count` nodes.
fn create_stop_function(max_node_count: i32) -> impl FnMut(i32) -> bool {
    let mut count = max_node_count;
    move |_| {
        count -= 1;
        count < 0
    }
}
