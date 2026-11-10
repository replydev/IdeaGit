//! Sorted views of the permanent graph: the sort index map, the BEK ("IntelliSort") sorter and the
//! controller showing the graph in a sorted order.
//!
//! Ports `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/`; see each
//! submodule.

pub mod bek;
pub mod sort_checker;
pub mod sort_index_map;
pub mod sorted_base_controller;
