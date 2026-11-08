//! Reachability queries on the permanent graph: containing branches and "is in branch" sets.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/ReachableNodes.kt`.
//! The shared `visited` flags are guarded by a `Mutex` (upstream: `synchronized`).

use std::collections::HashSet;
use std::sync::Mutex;

use crate::api::linear_graph::LiteLinearGraph;
use crate::api::permanent::VcsLogGraphNodeId;
use crate::utils::dfs_util::DfsWalk;
use crate::utils::flags::{BitSetFlags, Flags};

pub struct ReachableNodes<G> {
    graph: G,
    visited: Mutex<BitSetFlags>,
}

impl<G: LiteLinearGraph> ReachableNodes<G> {
    pub fn new(graph: G) -> ReachableNodes<G> {
        let visited = Mutex::new(BitSetFlags::new(graph.nodes_count()));
        ReachableNodes { graph, visited }
    }

    /// Branch nodes (from `branch_node_indexes`) from which `node_id` can be reached going up.
    pub fn get_containing_branches(
        &self,
        node_id: VcsLogGraphNodeId,
        branch_node_indexes: &HashSet<VcsLogGraphNodeId>,
    ) -> HashSet<VcsLogGraphNodeId> {
        let mut result = HashSet::new();

        self.walk([node_id], false, |node| {
            if branch_node_indexes.contains(&node) {
                result.insert(node);
            }
            true
        });

        result
    }

    /// Calls `consumer` for every node reachable down from `head_ids` (negative ids are skipped).
    pub fn walk_down(
        &self,
        head_ids: impl IntoIterator<Item = VcsLogGraphNodeId>,
        mut consumer: impl FnMut(VcsLogGraphNodeId),
    ) {
        self.walk(head_ids, true, |node| {
            consumer(node);
            true
        });
    }

    pub fn walk(
        &self,
        start_nodes: impl IntoIterator<Item = VcsLogGraphNodeId>,
        go_down: bool,
        consumer: impl FnMut(VcsLogGraphNodeId) -> bool,
    ) {
        let mut visited = self.visited.lock().unwrap_or_else(|e| e.into_inner());
        visited.set_all(false);
        DfsWalk::new(start_nodes, &self.graph, &mut *visited).walk(go_down, consumer);
    }
}
