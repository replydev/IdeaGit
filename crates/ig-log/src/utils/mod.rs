//! Graph utilities, plus the [`TimestampGetter`] interface.
//!
//! `TimestampGetter` is ported from
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/TimestampGetter.java`.
//!
//! Not ported (not needed by the `Normal` graph): `BfsUtil.kt`, `GraphUtil.kt`, `IntHashSetFlags.kt`,
//! `UnsignedBitSet.java`, `IntList.java` and the `IntToIntMap` family (`utils/impl/*IntToIntMap.java`,
//! `PredicateFlags.java`); they arrive with collapsing, BEK and filters in Plan 2b.

pub mod bounded_cache;
pub mod dfs_util;
pub mod flags;
pub mod int_int_multi_map;
pub mod java_sort;
pub mod linear_graph_utils;
pub mod normal_edge;

/// Timestamp of the commit in a row (`TimestampGetter`).
pub trait TimestampGetter {
    fn get_timestamp(&self, index: i32) -> i64;
}
