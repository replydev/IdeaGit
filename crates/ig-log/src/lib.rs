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

pub mod actions;
pub mod api;
pub mod collapsing;
pub mod facade;
pub mod graph_color_manager;
pub mod graph_commit;
pub mod permanent;
pub mod permanent_graph;
pub mod print;
pub mod print_element;
pub mod utils;
pub mod visible_graph;
