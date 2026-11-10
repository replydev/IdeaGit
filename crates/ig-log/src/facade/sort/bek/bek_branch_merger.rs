//! Merges the BEK branches into one order: repeatedly inserts the prepared part with the oldest
//! timestamp, bottom up.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekBranchMerger.java`.

use crate::api::linear_graph::LinearGraph;
use crate::facade::sort::bek::bek_branch::BekBranch;
use crate::facade::sort::bek::bek_edge_restrictions::BekEdgeRestrictions;
use crate::utils::TimestampGetter;

pub struct BekBranchMerger<'a, G: ?Sized, T: ?Sized> {
    permanent_graph: &'a G,
    bek_branches: Vec<BekBranch>,
    edge_restrictions: BekEdgeRestrictions,
    timestamp_getter: &'a T,
    inverse_result_list: Vec<i32>,
}

impl<'a, G: LinearGraph + ?Sized, T: TimestampGetter + ?Sized> BekBranchMerger<'a, G, T> {
    pub fn new(
        permanent_graph: &'a G,
        bek_branches: Vec<BekBranch>,
        edge_restrictions: BekEdgeRestrictions,
        timestamp_getter: &'a T,
    ) -> Self {
        BekBranchMerger {
            permanent_graph,
            bek_branches,
            edge_restrictions,
            timestamp_getter,
            inverse_result_list: Vec::new(),
        }
    }

    /// Returns whether some branch is not done yet.
    fn prepare_last_parts_for_branches(&mut self) -> bool {
        let mut has_undone_branches = false;
        for bek_branch in &mut self.bek_branches {
            if !bek_branch.is_done() {
                has_undone_branches = true;
                if bek_branch.get_prepare_for_insert_part().is_none() {
                    bek_branch.update_prepare_for_insert_part(
                        self.permanent_graph,
                        self.timestamp_getter,
                        &self.edge_restrictions,
                    );
                }
            }
        }
        has_undone_branches
    }

    fn get_branch_last_part_timestamp(&self, bek_branch: &BekBranch) -> i64 {
        let Some(prepare_for_insert_part) = bek_branch.get_prepare_for_insert_part() else {
            return i64::MAX;
        };

        debug_assert!(!prepare_for_insert_part.is_empty());
        let node_index = prepare_for_insert_part[0];
        self.timestamp_getter.get_timestamp(node_index)
    }

    fn step(&mut self) {
        let mut select_branch = 0;
        for (i, bek_branch) in self.bek_branches.iter().enumerate() {
            if self.get_branch_last_part_timestamp(&self.bek_branches[select_branch])
                > self.get_branch_last_part_timestamp(bek_branch)
            {
                select_branch = i;
            }
        }

        // upstream: LOG.assertTrue, then a NullPointerException
        let prepare_for_insert_part = self.bek_branches[select_branch]
            .get_prepare_for_insert_part()
            .expect("BEK: no branch part can be inserted")
            .to_vec();
        for &inserted_node in &prepare_for_insert_part {
            self.edge_restrictions.remove_restriction(inserted_node);
        }

        self.inverse_result_list
            .extend(prepare_for_insert_part.iter().rev());
        self.bek_branches[select_branch].done_insert_prepared_part();
    }

    pub fn get_result(mut self) -> Vec<i32> {
        while self.prepare_last_parts_for_branches() {
            self.step();
        }

        self.inverse_result_list.reverse();
        self.inverse_result_list
    }
}
