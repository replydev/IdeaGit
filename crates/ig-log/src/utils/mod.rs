//! Graph utilities, plus the [`TimestampGetter`] interface.
//!
//! `TimestampGetter` is ported from
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/TimestampGetter.java`.

pub mod bfs_util;
pub mod bounded_cache;
pub mod dfs_util;
pub mod flags;
pub mod graph_util;
pub mod int_hash_set_flags;
pub mod int_int_multi_map;
pub mod int_list;
pub mod int_to_int_map;
pub mod java_sort;
pub mod linear_graph_utils;
pub mod list_int_to_int_map;
pub mod normal_edge;
pub mod permanent_list_int_to_int_map;
pub mod predicate_flags;
pub mod tree_int_to_int_map;
pub mod unsigned_bit_set;

/// Timestamp of the commit in a row (`TimestampGetter`).
pub trait TimestampGetter {
    fn get_timestamp(&self, index: i32) -> i64;
}
