//! Node id to commit id (and timestamp) mapping of the permanent graph.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentCommitsInfoImpl.kt`
//! and `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/RowsMapping.kt`.
//!
//! Deviation: no `Int`-specialised storage (`isIntegerCase`); `Vec<CommitId>` is already compact.
//! `get_node_id` stays a linear search, like upstream ("todo optimize with special map").

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::api::permanent::{PermanentCommitsInfo, VcsLogGraphNodeId};
use crate::graph_commit::GraphCommit;
use crate::utils::TimestampGetter;

/// Commit ids and timestamps by row (`RowsMapping`).
#[derive(Clone, Debug)]
pub struct RowsMapping<CommitId> {
    commit_id_mapping: Vec<CommitId>,
    timestamps_mapping: Vec<i64>,
}

impl<CommitId: Clone> RowsMapping<CommitId> {
    pub fn new(size: usize) -> RowsMapping<CommitId> {
        RowsMapping {
            commit_id_mapping: Vec::with_capacity(size),
            timestamps_mapping: Vec::with_capacity(size),
        }
    }

    pub fn add(&mut self, commit_id: CommitId, timestamp: i64) {
        self.commit_id_mapping.push(commit_id);
        self.timestamps_mapping.push(timestamp);
    }

    pub fn commit_id_mapping(&self) -> &[CommitId] {
        &self.commit_id_mapping
    }

    pub fn get_commit_id(&self, row: i32) -> CommitId {
        self.commit_id_mapping[row as usize].clone()
    }
}

impl<CommitId> TimestampGetter for RowsMapping<CommitId> {
    fn get_timestamp(&self, row: i32) -> i64 {
        self.timestamps_mapping[row as usize]
    }
}

#[derive(Clone, Debug)]
pub struct PermanentCommitsInfoImpl<CommitId> {
    rows_mapping: RowsMapping<CommitId>,
    not_loaded_commits: HashMap<i32, CommitId>,
}

impl<CommitId: Clone + Eq + Hash + Send + Sync> PermanentCommitsInfoImpl<CommitId> {
    /// `newInstance`: rows in the order of `graph_commits`; `not_loaded_commits` maps synthetic
    /// negative ids to the commits they stand for.
    pub fn new_instance<G: GraphCommit<CommitId>>(
        graph_commits: &[G],
        not_loaded_commits: HashMap<i32, CommitId>,
    ) -> PermanentCommitsInfoImpl<CommitId> {
        let mut rows_mapping = RowsMapping::new(graph_commits.len());
        for commit in graph_commits {
            rows_mapping.add(commit.id().clone(), commit.timestamp());
        }
        PermanentCommitsInfoImpl {
            rows_mapping,
            not_loaded_commits,
        }
    }

    pub fn timestamp_getter(&self) -> &dyn TimestampGetter {
        &self.rows_mapping
    }

    fn get_not_load_node_id(&self, commit_id: &CommitId) -> i32 {
        self.not_loaded_commits
            .iter()
            .find(|(_, value)| *value == commit_id)
            .map(|(&key, _)| key)
            .unwrap_or(-1)
    }

    pub fn convert_to_commit_id_list(
        &self,
        commit_indexes: impl IntoIterator<Item = VcsLogGraphNodeId>,
    ) -> Vec<CommitId> {
        commit_indexes
            .into_iter()
            .map(|node_id| self.get_commit_id(node_id))
            .collect()
    }

    pub fn convert_to_commit_id_set(
        &self,
        commit_indexes: impl IntoIterator<Item = VcsLogGraphNodeId>,
    ) -> HashSet<CommitId> {
        commit_indexes
            .into_iter()
            .map(|node_id| self.get_commit_id(node_id))
            .collect()
    }

    /// `convertToNodeIds(commitIds, skipNotLoadedCommits)`.
    pub fn convert_to_node_ids_skipping(
        &self,
        commit_ids: &HashSet<CommitId>,
        skip_not_loaded_commits: bool,
    ) -> HashSet<VcsLogGraphNodeId> {
        let mut result: HashSet<i32> = self
            .rows_mapping
            .commit_id_mapping()
            .iter()
            .enumerate()
            .filter(|(_, commit_id)| commit_ids.contains(commit_id))
            .map(|(i, _)| i as i32)
            .collect();
        if !skip_not_loaded_commits {
            for (&node_id, commit_id) in &self.not_loaded_commits {
                if commit_ids.contains(commit_id) {
                    result.insert(node_id);
                }
            }
        }
        result
    }

    pub fn contains_all<'a>(&self, commit_ids: impl IntoIterator<Item = &'a CommitId>) -> bool
    where
        CommitId: 'a,
    {
        let mapping = self.rows_mapping.commit_id_mapping();
        commit_ids.into_iter().all(|id| mapping.contains(id))
    }
}

impl<CommitId: Clone + Eq + Hash + Send + Sync> PermanentCommitsInfo<CommitId>
    for PermanentCommitsInfoImpl<CommitId>
{
    /// # Panics
    /// If `node_id` is neither a row nor a known not-loaded id.
    fn get_commit_id(&self, node_id: VcsLogGraphNodeId) -> CommitId {
        if node_id < 0 {
            return self
                .not_loaded_commits
                .get(&node_id)
                .cloned()
                .unwrap_or_else(|| panic!("Unknown not loaded node id: {node_id}"));
        }
        self.rows_mapping.get_commit_id(node_id)
    }

    fn try_get_commit_id(&self, node_id: VcsLogGraphNodeId) -> Option<CommitId> {
        if node_id < 0 {
            return self.not_loaded_commits.get(&node_id).cloned();
        }
        self.rows_mapping
            .commit_id_mapping()
            .get(node_id as usize)
            .cloned()
    }

    fn get_timestamp(&self, node_id: VcsLogGraphNodeId) -> i64 {
        if node_id < 0 {
            return 0;
        }
        self.rows_mapping.get_timestamp(node_id)
    }

    /// The node id assigned to `commit_id`:
    /// - `0..nodes_count`: the row of a loaded commit;
    /// - `<= -2`: the synthetic id of a commit referenced as a parent but not loaded;
    /// - `-1`: unknown to this permanent graph.
    fn get_node_id(&self, commit_id: &CommitId) -> VcsLogGraphNodeId {
        if let Some(index) = self
            .rows_mapping
            .commit_id_mapping()
            .iter()
            .position(|id| id == commit_id)
        {
            return index as i32;
        }
        self.get_not_load_node_id(commit_id)
    }

    fn convert_to_node_ids(&self, commit_ids: &HashSet<CommitId>) -> HashSet<VcsLogGraphNodeId> {
        self.convert_to_node_ids_skipping(commit_ids, false)
    }
}
