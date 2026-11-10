//! Finds the merge fragments of the BEK graph and collapses them into dotted edges.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/linearBek/LinearBekGraphBuilder.java`.
//!
//! Deviations:
//! - The edge and magic-set queues are [`JavaPriorityQueue`]s, so edges that tie on their down
//!   node are polled in upstream's order.
//! - `GraphEdgeComparator` orders two edges without a down node by a deterministic hash (upstream:
//!   `hashCode` differences, where the edge type's hash is an identity hash). Such edges never
//!   reach the queue, which only holds normal edges.
//! - `MergeFragment` collapses its tail edges and tails in ascending order (upstream: fastutil hash
//!   order); the resulting edges do not depend on it.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashSet};

use crate::api::EdgeFilter;
use crate::api::GraphLayout;
use crate::api::elements::{GraphEdge, GraphEdgeType};
use crate::api::linear_graph::LinearGraph;
use crate::linear_bek::linear_bek_graph::LinearBekGraph;
use crate::print::print_element_generator_impl::LONG_EDGE_SIZE;
use crate::utils::int_int_multi_map::IntIntMultiMap;
use crate::utils::java_priority_queue::JavaPriorityQueue;
use crate::utils::linear_graph_utils::{get_down_nodes, get_edge};

pub const MAX_BLOCK_SIZE: i32 = 200;
const MAGIC_SET_SIZE: i32 = LONG_EDGE_SIZE;

pub struct LinearBekGraphBuilder<'a> {
    graph_layout: &'a dyn GraphLayout,
    linear_bek_graph: &'a LinearBekGraph,
}

impl<'a> LinearBekGraphBuilder<'a> {
    pub fn new(
        bek_graph: &'a LinearBekGraph,
        graph_layout: &'a dyn GraphLayout,
    ) -> LinearBekGraphBuilder<'a> {
        LinearBekGraphBuilder {
            graph_layout,
            linear_bek_graph: bek_graph,
        }
    }

    /// Collapses every merge fragment, bottom up; returns the merges that were collapsed.
    pub fn collapse_all(&self) -> HashSet<i32> {
        let mut collapsed_merges = HashSet::new();
        for i in (0..self.linear_bek_graph.graph().nodes_count()).rev() {
            if let Some(fragment) = self.get_fragment(i) {
                fragment.collapse(self.linear_bek_graph);
                collapsed_merges.insert(fragment.get_parent());
            }
        }
        collapsed_merges
    }

    pub fn collapse_fragment(&self, merge_commit: i32) -> Option<MergeFragment> {
        let fragment = self.get_fragment(merge_commit)?;
        fragment.collapse(self.linear_bek_graph);
        Some(fragment)
    }

    pub fn get_fragment(&self, merge_commit: i32) -> Option<MergeFragment> {
        let mut down_nodes = get_down_nodes(self.linear_bek_graph, merge_commit);
        down_nodes.sort_unstable();
        if down_nodes.len() != 2 {
            return None;
        }

        self.get_fragment_of(down_nodes[1], down_nodes[0], merge_commit)
    }

    fn get_fragment_of(
        &self,
        left_child: i32,
        right_child: i32,
        parent: i32,
    ) -> Option<MergeFragment> {
        let mut fragment = MergeFragment::new(parent, left_child, right_child);

        let left_li = self.graph_layout.get_layout_index(left_child);
        let right_li = self.graph_layout.get_layout_index(right_child);
        let mut rows_count = 1;
        let mut block_size = 1;

        let mut queue = JavaPriorityQueue::new(graph_edge_comparator);
        queue.add_all(
            self.linear_bek_graph
                .get_adjacent_edges(right_child, EdgeFilter::NormalDown),
        );

        let mut magic_set: Option<HashSet<i32>> = None;

        while let Some(next_edge) = queue.poll() {
            let next = next_edge.down_node_index;
            // can not happen
            let up_node_index = next_edge.up_node_index.expect("edge without an up node");
            let Some(next) = next else {
                fragment.add_tail(up_node_index);
                continue; // allow very long edges down
            };

            if next == left_child {
                // found first child
                fragment.add_tail(up_node_index);
                fragment.set_merge_with_old_commit(true);
            } else if next == right_child + rows_count {
                // all is fine, continuing
                rows_count += 1;
                block_size += 1;
                queue.add_all(
                    self.linear_bek_graph
                        .get_adjacent_edges(next, EdgeFilter::NormalDown),
                );
                fragment.add_body(up_node_index);
            } else if next > right_child + rows_count && next < left_child {
                rows_count = next - right_child + 1;
                block_size += 1;
                queue.add_all(
                    self.linear_bek_graph
                        .get_adjacent_edges(next, EdgeFilter::NormalDown),
                );
                fragment.add_body(up_node_index);
            } else if next > left_child {
                let li = self.graph_layout.get_layout_index(next);
                if left_li > right_li && !fragment.is_merge_with_old_commit() {
                    if next > left_child + MAGIC_SET_SIZE {
                        return None;
                    }
                    let magic_set =
                        magic_set.get_or_insert_with(|| self.calculate_magic_set(left_child));

                    if magic_set.contains(&next) {
                        fragment.add_tail_edge(up_node_index, next);
                    } else {
                        return None;
                    }
                } else if (li > left_li && li < right_li) || li == left_li {
                    fragment.add_tail_edge(up_node_index, next);
                } else if li >= right_li {
                    return None;
                } else if next > left_child + MAGIC_SET_SIZE {
                    if !fragment.has_tail_edge(up_node_index) && !fragment.is_body(up_node_index) {
                        return None;
                    }
                } else {
                    let magic_set =
                        magic_set.get_or_insert_with(|| self.calculate_magic_set(left_child));

                    if magic_set.contains(&next) {
                        fragment.add_tail_edge(up_node_index, next);
                    } else {
                        return None;
                    }
                }
            }

            if block_size >= MAX_BLOCK_SIZE {
                return None;
            }
        }

        if fragment.get_tails().is_empty() {
            return None; // this can happen if we ran into initial import
        }

        Some(fragment)
    }

    fn calculate_magic_set(&self, node: i32) -> HashSet<i32> {
        let mut magic_set = HashSet::with_capacity(MAGIC_SET_SIZE as usize);

        let down_nodes_of = |n: i32| {
            self.linear_bek_graph
                .get_adjacent_edges(n, EdgeFilter::NormalDown)
                .into_iter()
                .map(graph_edge_to_down_node)
        };
        let mut magic_queue = JavaPriorityQueue::new(|a: &Option<i32>, b: &Option<i32>| a.cmp(b));
        magic_queue.add_all(down_nodes_of(node));
        while let Some(i) = magic_queue.poll() {
            // a normal down edge always has a down node
            let i = i.expect("normal edge without a down node");
            if i > node + MAGIC_SET_SIZE {
                break;
            }
            magic_set.insert(i);
            magic_queue.add_all(down_nodes_of(i));
        }
        magic_set
    }
}

/// A merge with its two children and the nodes in between (the "body" of the merged branch and
/// its "tails", the nodes from which it reaches the left child or below).
#[derive(Clone, Debug)]
pub struct MergeFragment {
    parent: i32,
    left_child: i32,
    right_child: i32,

    merge_with_old_commit: bool,
    tail_edges: IntIntMultiMap,
    block_body: HashSet<i32>,
    tails: HashSet<i32>,
}

impl MergeFragment {
    fn new(parent: i32, left_child: i32, right_child: i32) -> MergeFragment {
        MergeFragment {
            parent,
            left_child,
            right_child,
            merge_with_old_commit: false,
            tail_edges: IntIntMultiMap::new(),
            block_body: HashSet::new(),
            tails: HashSet::new(),
        }
    }

    pub fn is_merge_with_old_commit(&self) -> bool {
        self.merge_with_old_commit
    }

    pub fn set_merge_with_old_commit(&mut self, merge_with_old_commit: bool) {
        self.merge_with_old_commit = merge_with_old_commit;
    }

    pub fn add_tail(&mut self, tail: i32) {
        if !self.block_body.contains(&tail) {
            self.tails.insert(tail);
        }
    }

    pub fn add_tail_edge(&mut self, up_node_index: i32, down_node_index: i32) {
        if !self.block_body.contains(&up_node_index) {
            self.tails.insert(up_node_index);
            self.tail_edges.put_value(up_node_index, down_node_index);
        }
    }

    pub fn add_body(&mut self, body: i32) {
        self.block_body.insert(body);
    }

    pub fn get_tails(&self) -> &HashSet<i32> {
        &self.tails
    }

    pub fn get_tails_and_body(&self) -> HashSet<i32> {
        self.block_body.union(&self.tails).copied().collect()
    }

    pub fn get_all_nodes(&self) -> HashSet<i32> {
        let mut nodes = HashSet::new();
        nodes.insert(self.parent);
        nodes.insert(self.left_child);
        nodes.insert(self.right_child);
        nodes.extend(self.get_tails_and_body());
        nodes
    }

    pub fn collapse(&self, graph: &LinearBekGraph) {
        let up_node_indexes: BTreeSet<i32> = self.tail_edges.keys().into_iter().collect();
        for up_node_index in up_node_indexes {
            for &down_node_index in self.tail_edges.get(up_node_index) {
                remove_edge(graph, up_node_index, down_node_index);
            }
        }

        let tails: BTreeSet<i32> = self.tails.iter().copied().collect();
        for tail in tails {
            if !get_down_nodes(graph, tail).contains(&self.left_child) {
                add_edge(graph, tail, self.left_child);
            } else {
                replace_edge(graph, tail, self.left_child);
            }
        }
        remove_edge(graph, self.parent, self.left_child);
    }

    pub fn get_parent(&self) -> i32 {
        self.parent
    }

    pub fn has_tail_edge(&self, index: i32) -> bool {
        !self.tail_edges.get(index).is_empty()
    }

    pub fn is_body(&self, index: i32) -> bool {
        self.block_body.contains(&index)
    }

    pub fn get_left_child(&self) -> i32 {
        self.left_child
    }
}

fn dotted_edge(up: i32, down: i32) -> GraphEdge {
    GraphEdge::new(Some(up), Some(down), None, GraphEdgeType::Dotted)
}

fn add_edge(graph: &LinearBekGraph, up: i32, down: i32) {
    graph.dotted_edges().create_edge(&dotted_edge(up, down));
}

/// # Panics
/// If there is no edge between `up` and `down` (upstream: an assertion, then a
/// `NullPointerException`); the fragment search only collects existing edges.
fn remove_edge(graph: &LinearBekGraph, up: i32, down: i32) {
    if graph.dotted_edges().has_edge(up, down) {
        graph.dotted_edges().remove_edge(&dotted_edge(up, down));
        graph.hidden_edges().create_edge(&dotted_edge(up, down));
    } else {
        let edge = get_edge(&**graph.graph(), up, down)
            .unwrap_or_else(|| panic!("No edge between {up} and {down}"));
        graph.hidden_edges().create_edge(&edge);
    }
}

/// # Panics
/// See [`remove_edge`].
fn replace_edge(graph: &LinearBekGraph, up: i32, down: i32) {
    if !graph.dotted_edges().has_edge(up, down) {
        let edge = get_edge(&**graph.graph(), up, down)
            .unwrap_or_else(|| panic!("No edge between {up} and {down}"));
        graph.hidden_edges().create_edge(&edge);
        graph.dotted_edges().create_edge(&dotted_edge(up, down));
    }
}

/// `GraphEdgeComparator`: by down node, edges without a down node last.
fn graph_edge_comparator(e1: &GraphEdge, e2: &GraphEdge) -> Ordering {
    match (e1.down_node_index, e2.down_node_index) {
        (None, None) => java_hash_code(e1).cmp(&java_hash_code(e2)),
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(d1), Some(d2)) => d1.cmp(&d2),
    }
}

/// `GraphEdge.hashCode`, with the type's ordinal for its (identity) hash.
fn java_hash_code(edge: &GraphEdge) -> i32 {
    let mut result = edge.up_node_index.unwrap_or(0);
    result = result
        .wrapping_mul(31)
        .wrapping_add(edge.down_node_index.unwrap_or(0));
    result = result
        .wrapping_mul(31)
        .wrapping_add(edge.target_id.unwrap_or(0));
    result
        .wrapping_mul(31)
        .wrapping_add(edge.edge_type.ordinal())
}

/// `GraphEdgeToDownNode`.
fn graph_edge_to_down_node(graph_edge: GraphEdge) -> Option<i32> {
    graph_edge.down_node_index
}
