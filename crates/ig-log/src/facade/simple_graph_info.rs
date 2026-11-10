//! A standalone graph of a range of visible rows, with its own layout and commit mapping.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/SimpleGraphInfo.java`.
//!
//! Upstream quirk kept: the branch node ids become node indexes of the visible graph the info was
//! built from, not of the new graph.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;

use crate::api::GraphLayout;
use crate::api::linear_graph::{LinearGraph, LiteLinearGraph, NodeFilter};
use crate::api::permanent::{PermanentCommitsInfo, PermanentGraphInfo, VcsLogGraphNodeId};
use crate::graph_commit::{GraphCommit, GraphCommitImpl};
use crate::permanent::graph_layout_impl::GraphLayoutImpl;
use crate::permanent::permanent_commits_info_impl::RowsMapping;
use crate::permanent::permanent_linear_graph_builder::PermanentLinearGraphBuilder;
use crate::permanent::permanent_linear_graph_impl::PermanentLinearGraphImpl;
use crate::utils::TimestampGetter;
use crate::utils::java_sort;
use crate::utils::linear_graph_utils::{as_lite_linear_graph, convert_ids_to_node_indexes};

pub struct SimpleGraphInfo<CommitId> {
    linear_graph: Arc<PermanentLinearGraphImpl>,
    graph_layout: GraphLayoutImpl,
    branch_node_ids: HashSet<VcsLogGraphNodeId>,
    commits_info: SimpleCommitsInfo<CommitId>,
}

impl<CommitId: Clone + Eq + Hash + Send + Sync> SimpleGraphInfo<CommitId> {
    /// The rows `visible_row - visible_range..` of `linear_graph` (no more than
    /// `2 * visible_range` of them).
    pub fn build(
        linear_graph: &dyn LinearGraph,
        old_layout: &dyn GraphLayout,
        permanent_commits_info: &dyn PermanentCommitsInfo<CommitId>,
        permanent_graph_size: i32,
        branch_node_ids: &HashSet<VcsLogGraphNodeId>,
        visible_row: i32,
        visible_range: i32,
    ) -> SimpleGraphInfo<CommitId> {
        // saturating: Java wraps for a huge `visible_range`; the clamping below makes saturation sensible
        let start = 0.max(visible_row.saturating_sub(visible_range));
        // no more than 2*visibleRange commits
        let end = linear_graph
            .nodes_count()
            .min(start.saturating_add(visible_range.saturating_mul(2)));
        let lite = as_lite_linear_graph(linear_graph);

        let mut rows_mapping = RowsMapping::new((end - start).max(0) as usize);
        let mut graph_commits = Vec::with_capacity((end - start).max(0) as usize);
        for row in start..end {
            let node_id = linear_graph.get_node_id(row);
            let commit = permanent_commits_info.get_commit_id(node_id);
            let parents: Vec<CommitId> = lite
                .get_nodes(row, NodeFilter::Down)
                .into_iter()
                .filter(|&row1| row1 >= start && row1 < end)
                .map(|row1| permanent_commits_info.get_commit_id(linear_graph.get_node_id(row1)))
                .collect();
            let timestamp = permanent_commits_info.get_timestamp(node_id);
            rows_mapping.add(commit.clone(), timestamp);
            graph_commits.push(GraphCommitImpl::create_commit(commit, parents, timestamp));
        }

        let new_linear_graph = PermanentLinearGraphBuilder::new_instance(&graph_commits).build();
        let new_lite = as_lite_linear_graph(&new_linear_graph);

        let mut layout_indexes = vec![0; graph_commits.len()];
        let mut head_node_indexes = Vec::new();

        let commit_id_to_integer =
            reverse_commit_id_map(permanent_commits_info, permanent_graph_size);
        for row in start..end {
            let commit_id = graph_commits[(row - start) as usize].id();
            // fastutil's getInt returns 0 for a missing key
            let layout_index = old_layout
                .get_layout_index(commit_id_to_integer.get(commit_id).copied().unwrap_or(0));
            layout_indexes[(row - start) as usize] = layout_index;
            if new_lite.get_nodes(row - start, NodeFilter::Up).is_empty() {
                head_node_indexes.push(row - start);
            }
        }

        java_sort::sort_by(&mut head_node_indexes, |&a, &b| {
            layout_indexes[a as usize].cmp(&layout_indexes[b as usize])
        });
        let new_layout = GraphLayoutImpl::new(layout_indexes, head_node_indexes);

        let nodes_count = new_linear_graph.nodes_count();
        SimpleGraphInfo {
            linear_graph: Arc::new(new_linear_graph),
            graph_layout: new_layout,
            branch_node_ids: convert_ids_to_node_indexes(
                linear_graph,
                branch_node_ids.iter().copied(),
            ),
            commits_info: SimpleCommitsInfo {
                rows_mapping,
                nodes_count,
            },
        }
    }
}

fn reverse_commit_id_map<CommitId: Clone + Eq + Hash>(
    permanent_commits_info: &dyn PermanentCommitsInfo<CommitId>,
    size: i32,
) -> HashMap<CommitId, i32> {
    (0..size)
        .map(|i| (permanent_commits_info.get_commit_id(i), i))
        .collect()
}

/// The `PermanentCommitsInfo` of a [`SimpleGraphInfo`]: rows of the new graph.
struct SimpleCommitsInfo<CommitId> {
    rows_mapping: RowsMapping<CommitId>,
    nodes_count: i32,
}

impl<CommitId: Clone + Eq + Hash + Send + Sync> PermanentCommitsInfo<CommitId>
    for SimpleCommitsInfo<CommitId>
{
    fn get_commit_id(&self, node_id: VcsLogGraphNodeId) -> CommitId {
        self.rows_mapping.get_commit_id(node_id)
    }

    fn try_get_commit_id(&self, node_id: VcsLogGraphNodeId) -> Option<CommitId> {
        usize::try_from(node_id)
            .ok()
            .and_then(|row| self.rows_mapping.commit_id_mapping().get(row).cloned())
    }

    fn get_timestamp(&self, node_id: VcsLogGraphNodeId) -> i64 {
        self.rows_mapping.get_timestamp(node_id)
    }

    fn get_node_id(&self, commit_id: &CommitId) -> VcsLogGraphNodeId {
        (0..self.nodes_count)
            .find(|&id| self.rows_mapping.get_commit_id(id) == *commit_id)
            .unwrap_or(-1)
    }

    fn convert_to_node_ids(&self, commit_ids: &HashSet<CommitId>) -> HashSet<VcsLogGraphNodeId> {
        (0..self.nodes_count)
            .filter(|&id| commit_ids.contains(&self.rows_mapping.get_commit_id(id)))
            .collect()
    }
}

impl<CommitId: Clone + Eq + Hash + Send + Sync> PermanentGraphInfo<CommitId>
    for SimpleGraphInfo<CommitId>
{
    fn permanent_commits_info(&self) -> &dyn PermanentCommitsInfo<CommitId> {
        &self.commits_info
    }

    fn linear_graph(&self) -> Arc<dyn LinearGraph> {
        self.linear_graph.clone()
    }

    fn permanent_graph_layout(&self) -> &dyn GraphLayout {
        &self.graph_layout
    }

    fn branch_node_ids(&self) -> &HashSet<VcsLogGraphNodeId> {
        &self.branch_node_ids
    }
}
