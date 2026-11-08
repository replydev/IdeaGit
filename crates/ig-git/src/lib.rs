//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod branch;
pub mod command;
pub mod config;
pub mod console;
pub mod error;
pub mod handler;
pub mod hash;
pub mod log;
pub mod natural_comparator;
pub mod reader;
pub mod refs;
pub mod repo_files;
pub mod roots;
pub mod runner;
pub mod status;
pub mod version;
