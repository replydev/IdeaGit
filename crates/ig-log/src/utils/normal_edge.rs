//! A normal edge as a pair of node indexes.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/NormalEdge.java`.

/// Up and down node indexes of a normal edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NormalEdge {
    pub up: i32,
    pub down: i32,
}

impl NormalEdge {
    pub fn create(up: i32, down: i32) -> NormalEdge {
        NormalEdge { up, down }
    }
}
