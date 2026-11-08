//! Building the permanent graph: the linear graph of all commits, its layout and commit mapping.
//!
//! Ports `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/`; see each submodule.

pub mod duplicate_parent_fixer;
pub mod graph_layout_builder;
pub mod graph_layout_impl;
pub mod permanent_commits_info_impl;
pub mod permanent_linear_graph_builder;
pub mod permanent_linear_graph_impl;
