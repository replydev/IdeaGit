//! Depth-first walks.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/DfsUtil.kt`.
//!
//! Upstream quirk kept, early exit of [`DfsWalk::walk`]: if the consumer returns `false` on a start
//! node, the whole walk ends. If it returns `false` on a descendant, only that start node's walk
//! ends, the remaining start nodes are still walked, and the stack is left stale (it keeps the nodes
//! of the interrupted walk, which the next start node's walk then resumes).

use crate::api::linear_graph::{LiteLinearGraph, NodeFilter};
use crate::utils::flags::Flags;

/// `Dfs.NextNode.NODE_NOT_FOUND`: no next node, go back up the stack.
pub const NODE_NOT_FOUND: i32 = -1;
/// `Dfs.NextNode.EXIT`: stop the walk.
pub const EXIT: i32 = -10;

fn walk_with_stack(start: i32, stack: &mut Vec<i32>, mut next_node_fun: impl FnMut(i32) -> i32) {
    stack.push(start);

    while let Some(&top) = stack.last() {
        let next_node = next_node_fun(top);
        if next_node == EXIT {
            return;
        }
        if next_node != NODE_NOT_FOUND {
            stack.push(next_node);
        } else {
            stack.pop();
        }
    }
    stack.clear();
}

/// Walks from `start`: `next_node_fun(top)` returns the node to push, [`NODE_NOT_FOUND`] to pop, or
/// [`EXIT`] to stop.
pub fn walk(start: i32, next_node_fun: impl FnMut(i32) -> i32) {
    walk_with_stack(start, &mut Vec::new(), next_node_fun);
}

/// Depth-first walk from several start nodes, marking visited nodes in `visited`.
pub struct DfsWalk<'a, G: LiteLinearGraph + ?Sized> {
    start_nodes: Vec<i32>,
    graph: &'a G,
    visited: &'a mut dyn Flags,
    stack: Vec<i32>,
}

impl<'a, G: LiteLinearGraph + ?Sized> DfsWalk<'a, G> {
    pub fn new(
        start_nodes: impl IntoIterator<Item = i32>,
        graph: &'a G,
        visited: &'a mut dyn Flags,
    ) -> DfsWalk<'a, G> {
        DfsWalk {
            start_nodes: start_nodes.into_iter().collect(),
            graph,
            visited,
            stack: Vec::new(),
        }
    }

    /// Calls `consumer` for every reachable node (negative start nodes are skipped); `consumer`
    /// returns `false` to stop (see the module docs for the quirk).
    pub fn walk(&mut self, go_down: bool, mut consumer: impl FnMut(i32) -> bool) {
        let filter = if go_down {
            NodeFilter::Down
        } else {
            NodeFilter::Up
        };
        let graph = self.graph;
        for &start in &self.start_nodes {
            if start < 0 {
                continue;
            }
            if self.visited.get(start) {
                continue;
            }
            self.visited.set(start, true);
            if !consumer(start) {
                return;
            }

            let visited = &mut *self.visited;
            walk_with_stack(start, &mut self.stack, |current_node| {
                for down_node in graph.get_nodes(current_node, filter) {
                    if !visited.get(down_node) {
                        visited.set(down_node, true);
                        if !consumer(down_node) {
                            return EXIT;
                        }
                        return down_node;
                    }
                }
                NODE_NOT_FOUND
            });
        }
    }
}
