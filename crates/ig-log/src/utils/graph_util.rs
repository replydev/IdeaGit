//! Reachability and ancestry queries on graphs.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/GraphUtil.kt`.
//!
//! Deviations:
//! - Bug fix: [`get_reachable_matching_nodes`] of an empty graph without start or matched nodes
//!   returns an empty set. Upstream calls `UnsignedBitSet.set(0, -1, true)`, which throws, so an
//!   unfiltered `CollapsedController` over an empty graph (an empty repository) cannot be built.
//! - Kotlin's two `exclusiveNodes` overloads are [`lite_exclusive_nodes`] (`LiteLinearGraph`) and
//!   [`exclusive_nodes`] (`LinearGraph`); `IntSet`s are `HashSet<i32>`.

use std::collections::HashSet;

use crate::api::linear_graph::{LinearGraph, LiteLinearGraph, NodeFilter};
use crate::utils::bfs_util::{BfsQueue, BfsWalk};
use crate::utils::dfs_util::{DfsWalk, EXIT, NODE_NOT_FOUND, walk};
use crate::utils::flags::{BitSetFlags, Flags};
use crate::utils::int_hash_set_flags::IntHashSetFlags;
use crate::utils::linear_graph_utils::as_lite_linear_graph;
use crate::utils::unsigned_bit_set::UnsignedBitSet;

/// Nodes reachable from `start_nodes` by down edges, or all nodes if `start_nodes` is `None`.
pub fn get_reachable_nodes(
    graph: &(impl LinearGraph + ?Sized),
    start_nodes: Option<&HashSet<i32>>,
) -> UnsignedBitSet {
    get_reachable_matching_nodes(graph, start_nodes, None)
}

/// Matching nodes reachable from `start_nodes` by down edges, or all matching nodes if
/// `start_nodes` is `None`. `matched_nodes == None` means every node matches.
pub fn get_reachable_matching_nodes(
    graph: &(impl LinearGraph + ?Sized),
    start_nodes: Option<&HashSet<i32>>,
    matched_nodes: Option<&HashSet<i32>>,
) -> UnsignedBitSet {
    let mut visibility = UnsignedBitSet::new();
    let Some(start_nodes) = start_nodes else {
        match matched_nodes {
            // deviation: upstream throws for an empty graph (set(0, -1))
            None if graph.nodes_count() > 0 => {
                visibility.set_range(0, graph.nodes_count() - 1, true)
            }
            None => {}
            Some(matched_nodes) => {
                for &matched_id in matched_nodes {
                    visibility.set(matched_id, true);
                }
            }
        }
        return visibility;
    };

    let lite = as_lite_linear_graph(graph);
    let mut visited = BitSetFlags::new(graph.nodes_count());
    DfsWalk::new(start_nodes.iter().copied(), &lite, &mut visited).walk(true, |node| {
        if matched_nodes.is_none_or(|matched| matched.contains(&node)) {
            visibility.set(node, true);
        }
        true
    });
    visibility
}

/// Whether `lower_node` is an ancestor of `upper_node`.
pub fn is_ancestor(
    graph: &(impl LiteLinearGraph + ?Sized),
    lower_node: i32,
    upper_node: i32,
) -> bool {
    let mut visited = BitSetFlags::with_default(graph.nodes_count(), false);

    let mut result = false;
    walk(lower_node, |current_node| {
        visited.set(current_node, true);

        if current_node == upper_node {
            result = true;
            return EXIT;
        }
        if current_node > upper_node {
            for next_node in graph.get_nodes(current_node, NodeFilter::Up) {
                if !visited.get(next_node) {
                    return next_node;
                }
            }
        }

        NODE_NOT_FOUND
    });

    result
}

/// A parent of `start_node` on the path to `end_node`; the first parent if there is no path.
pub fn get_corresponding_parent(
    graph: &(impl LiteLinearGraph + ?Sized),
    start_node: i32,
    end_node: i32,
    visited: &mut dyn Flags,
) -> i32 {
    let candidates = graph.get_nodes(start_node, NodeFilter::Down);
    if candidates.len() == 1 {
        return candidates[0];
    }
    if candidates.contains(&end_node) {
        return end_node;
    }

    let mut bfs_walks: Vec<_> = candidates
        .iter()
        .map(|&candidate| BfsWalk::new(candidate, graph, true))
        .collect();

    visited.set_all(false);
    loop {
        for walk in &mut bfs_walks {
            if walk.step(visited, |_| true).contains(&end_node) {
                return walk.start;
            }
        }
        bfs_walks.retain(|walk| !walk.is_finished());
        if bfs_walks.is_empty() {
            break;
        }
    }

    candidates[0]
}

/// `visited1` of `subgraphDifference`: also counts nodes the second walk visited or queued.
struct Visited1<'a, Q: BfsQueue> {
    data: &'a mut IntHashSetFlags,
    visited2: &'a BitSetFlags,
    queue2: &'a Q,
}

impl<Q: BfsQueue> Flags for Visited1<'_, Q> {
    fn size(&self) -> i32 {
        self.data.size()
    }

    fn get(&self, index: i32) -> bool {
        self.data.get(index) || self.visited2.get(index) || self.queue2.contains(index)
    }

    fn set(&mut self, index: i32, value: bool) {
        self.data.set(index, value);
    }

    fn set_all(&mut self, value: bool) {
        self.data.set_all(value);
    }
}

/// Nodes reachable from `node1` but not from `node2`.
///
/// The set has no defined iteration order: callers must not depend on it (sort if needed).
pub fn subgraph_difference(
    graph: &(impl LinearGraph + ?Sized),
    node1: i32,
    node2: i32,
) -> HashSet<i32> {
    let lite_linear_graph = as_lite_linear_graph(graph);

    let mut visited2 = BitSetFlags::new(graph.nodes_count());
    let mut bfs_walk2 = BfsWalk::new(node2, &lite_linear_graph, true);
    let mut visited1 = IntHashSetFlags::new(graph.nodes_count());
    let mut bfs_walk1 = BfsWalk::new(node1, &lite_linear_graph, true);

    let mut max1 = bfs_walk1.current_nodes().max_node().unwrap_or(i32::MIN);
    let mut min2 = bfs_walk2.current_nodes().min_node().unwrap_or(i32::MAX);
    while !bfs_walk1.is_finished() {
        if max1 < min2 {
            let mut flags = Visited1 {
                data: &mut visited1,
                visited2: &visited2,
                queue2: bfs_walk2.current_nodes(),
            };
            let next = bfs_walk1.step(&mut flags, |_| true);
            max1 = max1.max(next.into_iter().max().unwrap_or(i32::MIN));
        } else {
            bfs_walk2.step(&mut visited2, |_| true);
            min2 = bfs_walk2.current_nodes().min_node().unwrap_or(i32::MAX);
        }
    }

    visited1.data
}

/// Nodes reachable only from `head_node` and not from other heads (`isHead`), on a lite graph.
///
/// The set has no defined iteration order: callers must not depend on it (sort if needed).
pub fn lite_exclusive_nodes(
    graph: &(impl LiteLinearGraph + ?Sized),
    head_node: i32,
    is_head: impl Fn(i32) -> bool,
) -> HashSet<i32> {
    let mut result = HashSet::new();
    let mut visited = BitSetFlags::new(graph.nodes_count());
    BfsWalk::new(head_node, graph, true).walk(&mut visited, |it| {
        let up_nodes = graph.get_nodes(it, NodeFilter::Up);
        if (up_nodes.is_empty() || up_nodes.iter().all(|up| result.contains(up)))
            && (it == head_node || !is_head(it))
        {
            result.insert(it);
            true
        } else {
            false
        }
    });
    result
}

/// Nodes reachable only from `head_node` and not from other heads (`isHead`).
///
/// The set has no defined iteration order: callers must not depend on it (sort if needed).
pub fn exclusive_nodes(
    graph: &(impl LinearGraph + ?Sized),
    head_node: i32,
    is_head: impl Fn(i32) -> bool,
) -> HashSet<i32> {
    lite_exclusive_nodes(&as_lite_linear_graph(graph), head_node, is_head)
}
