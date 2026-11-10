//! Linear BEK ("IntelliJ style"): incoming commits are shown on top of the main branch as if they
//! were rebased, with the merges collapsed into dotted edges.
//!
//! Ports `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/linearBek/`; see each submodule.

pub mod linear_bek_controller;
pub mod linear_bek_graph;
pub mod linear_bek_graph_builder;
