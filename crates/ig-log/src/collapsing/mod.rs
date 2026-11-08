//! Edge storage used by [`LinearGraphWrapper`](crate::facade::linear_graph_wrapper::LinearGraphWrapper).
//!
//! Only `EdgeStorage.java` and `EdgeStorageWrapper.java` of
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/` are ported; the collapsing
//! machinery (`CollapsedGraph`, `CollapsedController`, fragment generators, filters) is Plan 2b.

pub mod edge_storage;
pub mod edge_storage_wrapper;
