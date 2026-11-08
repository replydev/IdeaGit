//! The graph facade: the permanent graph, visible graphs and the controller chain behind them.
//!
//! Ports the `Normal`-sort subset of `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/`;
//! see each submodule. Not ported yet (Plan 2b): `CascadeController.kt`, `FilteredController.kt`,
//! `FirstParentController.kt`, `GraphModificationUtil.kt`, `SimpleGraphInfo.java` and `sort/**`.

pub mod base_controller;
pub mod graph_changes;
pub mod linear_graph_controller;
pub mod linear_graph_wrapper;
pub mod permanent_graph_impl;
pub mod print_element_presentation_manager_impl;
pub mod reachable_nodes;
pub mod visible_graph_impl;
