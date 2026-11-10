//! A branch of the BEK sort: a run of nodes inserted into the result in parts, bottom up.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekBranch.java`.

use crate::api::linear_graph::LinearGraph;
use crate::facade::sort::bek::bek_edge_restrictions::BekEdgeRestrictions;
use crate::utils::TimestampGetter;
use crate::utils::linear_graph_utils::get_down_nodes;

const MAX_BLOCK_SIZE: usize = 20;
const MAX_DELTA_TIME: i64 = 60 * 60 * 24 * 3 * 1000;
const SMALL_DELTA_TIME: i64 = 60 * 60 * 4 * 1000;

#[derive(Clone, Debug)]
pub struct BekBranch {
    node_indexes: Vec<i32>,
    no_insert_size: usize,
    /// Start of the prepared part; the part is `node_indexes[start..no_insert_size]`.
    prepare_for_insert_part: Option<usize>,
}

impl BekBranch {
    pub fn new(node_indexes: Vec<i32>) -> BekBranch {
        let no_insert_size = node_indexes.len();
        BekBranch {
            node_indexes,
            no_insert_size,
            prepare_for_insert_part: None,
        }
    }

    pub fn update_prepare_for_insert_part(
        &mut self,
        permanent_graph: &(impl LinearGraph + ?Sized),
        timestamp_getter: &(impl TimestampGetter + ?Sized),
        edge_restrictions: &BekEdgeRestrictions,
    ) {
        debug_assert!(self.prepare_for_insert_part.is_none());
        let current_node = self.node_indexes[self.no_insert_size - 1];

        if edge_restrictions.has_restriction(current_node) {
            return;
        }

        let mut prev_index = self.no_insert_size - 1;
        while prev_index > 0 {
            let up_node = self.node_indexes[prev_index - 1];
            let down_node = self.node_indexes[prev_index];

            // for correct topological order
            if edge_restrictions.has_restriction(up_node) {
                break;
            }

            // upNode is mergeCommit
            let down_nodes = get_down_nodes(permanent_graph, up_node);
            if down_nodes.len() > 1 && down_nodes.contains(&down_node) {
                prev_index -= 1;
                continue;
            }

            // division
            if !down_nodes.contains(&down_node) {
                break;
            }

            let delta = (timestamp_getter.get_timestamp(up_node)
                - timestamp_getter.get_timestamp(down_node))
            .abs();

            // long time between commits
            if delta > MAX_DELTA_TIME {
                break;
            }

            // if block so long
            if (prev_index as i64) < self.no_insert_size as i64 - MAX_BLOCK_SIZE as i64
                && delta > SMALL_DELTA_TIME
            {
                break;
            }
            prev_index -= 1;
        }

        self.prepare_for_insert_part = Some(prev_index);
    }

    pub fn get_prepare_for_insert_part(&self) -> Option<&[i32]> {
        self.prepare_for_insert_part
            .map(|start| &self.node_indexes[start..self.no_insert_size])
    }

    pub fn done_insert_prepared_part(&mut self) {
        let start = self
            .prepare_for_insert_part
            .take()
            .expect("no prepared part to insert");
        self.no_insert_size = start;
    }

    pub fn is_done(&self) -> bool {
        self.no_insert_size == 0
    }
}
