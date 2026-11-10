//! BEK ("IntelliSort", the "Standard" sort): commits of a merged branch directly below the merge.
//!
//! Ports `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/`; see each
//! submodule. Upstream's `LOG.assertTrue` checks are `debug_assert!`s.

pub mod bek_branch;
pub mod bek_branch_creator;
pub mod bek_branch_merger;
pub mod bek_edge_restrictions;
pub mod bek_sorter;
