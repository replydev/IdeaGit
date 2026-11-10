//! Filtering and collapsing: a graph with hidden nodes and dotted edges, the controllers that
//! show it, and the fragment generators behind hover and collapse.
//!
//! Ports `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/`; see each submodule.

pub mod branch_filter_controller;
pub mod collapsed_action_manager;
pub mod collapsed_controller;
pub mod collapsed_graph;
pub mod dotted_filter_edges_generator;
pub mod edge_storage;
pub mod edge_storage_wrapper;
pub mod fragment_generator;
pub mod graph_nodes_visibility;
pub mod linear_fragment_generator;
pub mod tree_set_node_iterator;
