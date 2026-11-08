//! VCS-agnostic log data: commits, references, commit-id interning, sorting and joining, the data
//! pack (permanent graph plus references) and the refresher that loads it.
//!
//! Ported from `platform/vcs-log/api` (`Hash`, `TimedVcsCommit`, `VcsCommitMetadata`, `VcsRef`,
//! `VcsRefType`, `VcsUser`, `VcsLogProvider`, `VcsLogRefManager`, ...) and
//! `platform/vcs-log/impl/src/com/intellij/vcs/log/{data,graph,impl}`. Each module names its
//! upstream files; see also `reference/upstream-map.toml`.
//!
//! A VCS plugs in by implementing [`VcsLogProvider`]; [`VcsLogRefresher`] turns what providers read
//! into a [`VcsLogGraphData`] whose permanent graph uses [`VcsLogStorage`] indexes as commit ids.

pub mod commit;
pub mod graph_color_manager_impl;
pub mod graph_data;
pub mod hash;
pub mod joiner;
pub mod multi_repo_joiner;
pub mod provider;
pub mod refresher;
pub mod refs_model;
pub mod root;
pub mod sorter;
pub mod storage;
pub mod vcs_ref;
pub mod vcs_user;

pub use commit::{CommitId, TimedVcsCommit, VcsCommitMetadata};
pub use graph_color_manager_impl::{GraphColorManagerImpl, HeadCommitsComparator};
pub use graph_data::{LogGraphCommit, LogProviders, VcsLogGraphData};
pub use hash::Hash;
pub use joiner::JoinError;
pub use provider::{
    DetailedLogData, LogData, Requirements, RequirementsEx, VcsException, VcsLogProvider,
    VcsLogRefManager,
};
pub use refresher::VcsLogRefresher;
pub use refs_model::{RefsModel, RootRefsModel};
pub use root::{VcsRoot, compare_roots, vcs_root};
pub use sorter::sort_by_date_topo_order;
pub use storage::{InMemoryStorage, VcsLogCommitStorageIndex, VcsLogStorage};
pub use vcs_ref::{VcsRef, VcsRefType, VcsRefsContainer, VcsRefsList};
pub use vcs_user::VcsUser;
