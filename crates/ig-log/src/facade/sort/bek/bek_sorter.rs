//! Computes the BEK order of the permanent graph.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekSorter.java`.

use crate::api::linear_graph::LinearGraph;
use crate::facade::sort::bek::bek_branch_creator::BekBranchCreator;
use crate::facade::sort::bek::bek_branch_merger::BekBranchMerger;
use crate::facade::sort::sort_index_map::SortIndexMap;
use crate::permanent::graph_layout_impl::GraphLayoutImpl;
use crate::utils::TimestampGetter;

pub fn create_bek_map(
    permanent_graph: &(impl LinearGraph + ?Sized),
    graph_layout: &GraphLayoutImpl,
    timestamp_getter: &(impl TimestampGetter + ?Sized),
) -> SortIndexMap {
    let bek_branch_creator = BekBranchCreator::new(permanent_graph, graph_layout);
    let (branches, edge_restrictions) = bek_branch_creator.get_result();

    let bek_branch_merger = BekBranchMerger::new(
        permanent_graph,
        branches,
        edge_restrictions,
        timestamp_getter,
    );
    let result = bek_branch_merger.get_result();

    debug_assert_eq!(result.len() as i32, permanent_graph.nodes_count());
    SortIndexMap::new(result)
}
