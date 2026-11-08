//! The git side of the log: `git log` parsing, reading commits, metadata and refs, ref ordering
//! and the [`GitLogProvider`](provider::GitLogProvider) that feeds `ig_log::data`.
//!
//! Ported from `plugins/git4idea/backend/src/history` (`GitLogParser`, `GitLogRecord`,
//! `GitLogOutputSplitter`, `GitLogRecordBuilder`, `GitLogRecordCollector`, `GitLogUtil`) and
//! `plugins/git4idea/backend/src/log` (`GitLogProvider`, `GitRefManager`). Each module names its
//! upstream files; see also `reference/upstream-map.toml`.
//!
//! This is the only part of `ig-git` that depends on `ig-log`, as upstream `git4idea` depends on
//! the `vcs-log` API: the provider produces `ig_log::data` types directly.

pub mod collector;
pub mod parser;
pub mod provider;
pub mod record;
pub mod ref_manager;
pub mod repository_manager;
pub mod splitter;
pub mod unescape;
pub mod util;
