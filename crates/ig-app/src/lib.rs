//! IdeaGit's application: windows, actions, the Git tool window (Log and Console) and the log
//! worker. Built on `ig-git` / `ig-log` for Git and `ig-ui-kit` for the IntelliJ New UI look.
//!
//! Every module names the upstream sources it ports or imitates; see also
//! `reference/upstream-map.toml`.

pub mod actions;
pub mod console;
pub mod log;
pub mod logging;
pub mod paths;
pub mod settings;
pub mod strings;
