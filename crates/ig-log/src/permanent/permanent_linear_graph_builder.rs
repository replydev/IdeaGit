//! Builds the [`PermanentLinearGraphImpl`] from a topologically sorted commit list.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentLinearGraphBuilder.java`.
//!
//! Deviations:
//! - Upstream bug fixed: for an edge to a commit that is not loaded, upstream fills the *first*
//!   unset edge slot of the child node, regardless of which parent the slot belongs to. When a
//!   commit has several unloaded parents and they are processed out of parent order, the edges (and
//!   therefore `PermanentGraph.allCommits` parents) get the wrong targets. Here the slot is found
//!   by parent position, like for loaded parents.
//! - Unloaded parents with the same smallest child node are processed in the order they were first
//!   seen (upstream: `HashMap` iteration order), so their synthetic ids are deterministic.

use std::collections::HashMap;
use std::hash::Hash;

use crate::graph_commit::GraphCommit;
use crate::permanent::duplicate_parent_fixer::{FixedGraphCommit, fix_duplicate_parent_commits};
use crate::permanent::permanent_linear_graph_impl::PermanentLinearGraphImpl;
use crate::utils::flags::{BitSetFlags, Flags};

/// Child nodes waiting for a parent commit, plus the order in which the parent was first seen.
struct UpNodes {
    first_seen: usize,
    up_nodes: Vec<i32>,
}

pub struct PermanentLinearGraphBuilder<'a, CommitId: Clone> {
    commits: Vec<FixedGraphCommit<'a, CommitId>>,
    simple_nodes: BitSetFlags,
    nodes_count: i32,
    node_to_edge_index: Vec<i32>,
    long_edges: Vec<i32>,
    /// downCommitId -> up node indexes
    up_adjacent_nodes: HashMap<CommitId, UpNodes>,
    seen_counter: usize,
}

impl<'a, CommitId: Eq + Hash + Clone> PermanentLinearGraphBuilder<'a, CommitId> {
    /// `newInstance`: fixes duplicate parents and finds the "simple" nodes (whose only parent is the
    /// next commit).
    pub fn new_instance<G: GraphCommit<CommitId>>(
        graph_commits: &'a [G],
    ) -> PermanentLinearGraphBuilder<'a, CommitId> {
        let graph_commits = fix_duplicate_parent_commits(graph_commits);
        let mut simple_nodes = BitSetFlags::new(graph_commits.len() as i32);

        let mut long_edges_count = 0;
        for (node_index, commit) in graph_commits.iter().enumerate() {
            let next_commit_hash_index = graph_commits.get(node_index + 1).map(|next| next.id());

            let parents = commit.parents();
            if parents.len() == 1 && Some(&parents[0]) == next_commit_hash_index {
                simple_nodes.set(node_index as i32, true);
            } else {
                long_edges_count += parents.len();
            }
        }

        let nodes_count = simple_nodes.size();
        PermanentLinearGraphBuilder {
            commits: graph_commits,
            simple_nodes,
            nodes_count,
            node_to_edge_index: vec![0; nodes_count as usize + 1],
            long_edges: vec![0; 2 * long_edges_count],
            up_adjacent_nodes: HashMap::new(),
            seen_counter: 0,
        }
    }

    fn add_underdone_edge(&mut self, up_node_index: i32, down_commit_id: &CommitId) {
        if let Some(entry) = self.up_adjacent_nodes.get_mut(down_commit_id) {
            entry.up_nodes.push(up_node_index);
            return;
        }
        self.seen_counter += 1;
        self.up_adjacent_nodes.insert(
            down_commit_id.clone(),
            UpNodes {
                first_seen: self.seen_counter,
                up_nodes: vec![up_node_index],
            },
        );
    }

    /// Index in `long_edges` of the down edge from `up_node_index` to its parent `down_commit_id`.
    fn find_underdone_edge(&self, up_node_index: i32, down_commit_id: &CommitId) -> Option<usize> {
        let end = self.node_to_edge_index[up_node_index as usize + 1] as usize;
        let parent_hash_indices = self.commits[up_node_index as usize].parents();
        parent_hash_indices
            .iter()
            .position(|parent| parent == down_commit_id)
            .map(|i| end - (parent_hash_indices.len() - i))
    }

    fn fix_underdone_edge(
        &mut self,
        up_node_index: i32,
        down_node_index: i32,
        down_commit_id: &CommitId,
    ) {
        let Some(edge_index) = self.find_underdone_edge(up_node_index, down_commit_id) else {
            panic!(
                "Not found underdone edges for node: {up_node_index}. Adjacent down node: {down_node_index}"
            );
        };
        if self.long_edges[edge_index] == -1 {
            self.long_edges[edge_index] = down_node_index;
        } else {
            panic!("Edge was set early!. Up node: {up_node_index}, down node: {down_node_index}");
        }
    }

    fn do_step(&mut self, node_index: i32) {
        let commit_id = self.commits[node_index as usize].commit_id();

        let up_nodes = self
            .up_adjacent_nodes
            .remove(commit_id)
            .map(|entry| entry.up_nodes)
            .unwrap_or_default();

        let mut edge_index = self.node_to_edge_index[node_index as usize] as usize;
        for up_node_index in up_nodes {
            self.fix_underdone_edge(up_node_index, node_index, commit_id);
            self.long_edges[edge_index] = up_node_index;
            edge_index += 1;
        }

        // down nodes
        if !self.simple_nodes.get(node_index) {
            let parents = self.commits[node_index as usize].parents_cow();
            for down_commit_id in parents.iter() {
                self.add_underdone_edge(node_index, down_commit_id);
                self.long_edges[edge_index] = -1;
                edge_index += 1;
            }
        }

        self.node_to_edge_index[node_index as usize + 1] = edge_index as i32;
    }

    /// `fixUnderdoneEdgeForNotLoadCommit`, with the slot chosen by parent position (see module docs).
    fn fix_underdone_edge_for_not_load_commit(
        &mut self,
        up_node_index: i32,
        not_load_id: i32,
        not_load_commit: &CommitId,
    ) {
        match self.find_underdone_edge(up_node_index, not_load_commit) {
            Some(edge_index) if self.long_edges[edge_index] == -1 => {
                self.long_edges[edge_index] = not_load_id;
            }
            _ => panic!("Not found underdone edge to not load commit for node: {up_node_index}"),
        }
    }

    fn fix_underdone_edges(&mut self, mut not_loaded_commit_to_id: impl FnMut(&CommitId) -> i32) {
        let up_adjacent_nodes = std::mem::take(&mut self.up_adjacent_nodes);
        let mut commit_ids: Vec<(&CommitId, &UpNodes)> = up_adjacent_nodes.iter().collect();
        commit_ids.sort_by_key(|(_, entry)| {
            (
                entry.up_nodes.iter().min().copied().unwrap_or(i32::MAX),
                entry.first_seen,
            )
        });
        for (not_load_commit, entry) in commit_ids {
            let not_load_id = not_loaded_commit_to_id(not_load_commit);
            for &up_node_index in &entry.up_nodes {
                self.fix_underdone_edge_for_not_load_commit(
                    up_node_index,
                    not_load_id,
                    not_load_commit,
                );
            }
        }
    }

    /// Builds the graph; `not_loaded_commit_to_id` assigns ids (`<= -2`) to parents that are not in
    /// the commit list, in order of their first child node.
    pub fn build_with(
        mut self,
        not_loaded_commit_to_id: impl FnMut(&CommitId) -> i32,
    ) -> PermanentLinearGraphImpl {
        for node_index in 0..self.nodes_count {
            self.do_step(node_index);
        }

        self.fix_underdone_edges(not_loaded_commit_to_id);

        PermanentLinearGraphImpl::new(self.simple_nodes, self.node_to_edge_index, self.long_edges)
    }

    /// `build()`: every unloaded parent gets id `i32::MIN`.
    pub fn build(self) -> PermanentLinearGraphImpl {
        self.build_with(|_| i32::MIN)
    }
}
