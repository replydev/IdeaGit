//! Commit graph core for IdeaGit: a port of IntelliJ's VCS Log graph
//! (`platform/vcs-log/graph` and `platform/vcs-log/graph-api`).
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.
//!
//! Layout mirrors upstream packages: the top-level modules are the public `graph-api`
//! (`com.intellij.vcs.log.graph`), [`api`] is the internal graph API
//! (`com.intellij.vcs.log.graph.api`), and [`permanent`], [`mod@print`], [`facade`], [`collapsing`]
//! and [`utils`] are the `impl` packages. Upstream names are kept in snake_case.
//!
//! Conventions: node indexes, node ids and counts are `i32`, like upstream, because not-loaded
//! commits have negative ids. Upstream exceptions are panics; upstream `assert` statements (disabled
//! in production) are `debug_assert!`s.
//!
//! Scope (Plan 2): building the permanent graph and showing it unfiltered with `SortType::Normal`.
//! BEK sorting, linear BEK, first-parent, branch and text filters and collapsing are Plan 2b.
//!
//! # Example
//!
//! ```
//! use std::collections::HashSet;
//!
//! use ig_log::print::graph_color_getter::GraphColorGetterByNodeFactory;
//! use ig_log::{GraphCommitImpl, Options, PermanentGraph, PermanentGraphImpl, VisibleGraph};
//!
//! // c (newest) -> b -> a
//! let commits = vec![
//!     GraphCommitImpl::create_commit("c".to_string(), vec!["b".to_string()], 3),
//!     GraphCommitImpl::create_commit("b".to_string(), vec!["a".to_string()], 2),
//!     GraphCommitImpl::create_commit("a".to_string(), vec![], 1),
//! ];
//! let factory = GraphColorGetterByNodeFactory::new(|_: &String, layout_index| layout_index);
//! let permanent = PermanentGraphImpl::new_instance(
//!     &commits,
//!     &factory,
//!     |a: &String, b: &String| a.cmp(b),
//!     &HashSet::new(),
//! );
//! let visible = permanent
//!     .create_visible_graph(Options::DEFAULT, None, None)
//!     .unwrap();
//!
//! let mut ids = Vec::new();
//! for row in 0..visible.visible_commit_count() {
//!     let info = visible.get_row_info(row);
//!     assert!(info.print_elements().len() >= 1);
//!     ids.push(info.commit());
//! }
//! assert_eq!(ids, ["c", "b", "a"]);
//! ```

pub mod actions;
pub mod api;
pub mod collapsing;
pub mod data;
pub mod facade;
pub mod graph_color_manager;
pub mod graph_commit;
pub mod linear_bek;
pub mod permanent;
pub mod permanent_graph;
pub mod print;
pub mod print_element;
pub mod utils;
pub mod visible_graph;

pub use actions::{ActionController, GraphAction, GraphActionType, GraphAnswer};
pub use api::printer::GraphPrintElement;
pub use facade::permanent_graph_impl::PermanentGraphImpl;
pub use facade::visible_graph_impl::VisibleGraphImpl;
pub use graph_color_manager::GraphColorManager;
pub use graph_commit::{GraphCommit, GraphCommitImpl};
pub use permanent_graph::{Options, PermanentGraph, SortType, UnsupportedGraphOptions};
pub use print_element::{EdgePrintElement, NodePrintElement, PrintElement};
pub use visible_graph::{RowInfo, RowType, VisibleGraph};

// Compile-time guard for the UI threading model: the permanent graph is built on a background
// thread and shared, while the visible graph is owned by the UI thread (moved there once built).
const _: fn() = || {
    fn is_send<T: Send>() {}
    fn is_send_sync<T: Send + Sync>() {}
    is_send_sync::<PermanentGraphImpl<String>>();
    is_send::<VisibleGraphImpl<String>>();
    is_send::<GraphAnswer<String>>();
    is_send::<GraphPrintElement>();
};
