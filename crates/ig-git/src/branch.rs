//! Branch references. Port of `GitLocalBranch.kt`, `GitStandardRemoteBranch.kt`,
//! `GitSvnRemoteBranch.kt` (shared module) and `GitUtil.parseRemoteBranch`.
//!
//! Deviation: names compare case-sensitively; upstream compares case-insensitively on
//! case-insensitive file systems. `GitRepositoryReader` handles the current-branch case fix.

use crate::config::GitRemote;
use crate::refs::{REFS_HEADS_PREFIX, REFS_REMOTES_PREFIX, strip_refs_prefix};

/// Upstream `GitUtil.ORIGIN_HEAD`.
pub const ORIGIN_HEAD: &str = "origin/HEAD";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GitLocalBranch {
    name: String,
}

impl GitLocalBranch {
    /// Accepts either `main` or `refs/heads/main`.
    pub fn new(name: &str) -> Self {
        Self {
            name: strip_refs_prefix(name).to_string(),
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn full_name(&self) -> String {
        format!("{REFS_HEADS_PREFIX}{}", self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GitRemoteBranch {
    /// e.g. remote `origin`, name at remote `main` → `origin/main`.
    Standard {
        remote: String,
        name_at_remote: String,
    },
    /// git-svn style `refs/remotes/<branch>` without a remote prefix.
    Svn { full_ref: String },
}

impl GitRemoteBranch {
    pub fn name(&self) -> String {
        match self {
            Self::Standard {
                remote,
                name_at_remote,
            } => format!("{remote}/{name_at_remote}"),
            Self::Svn { full_ref } => strip_refs_prefix(full_ref).to_string(),
        }
    }
    pub fn full_name(&self) -> String {
        match self {
            Self::Standard { .. } => format!("{REFS_REMOTES_PREFIX}{}", self.name()),
            Self::Svn { full_ref } => full_ref.clone(),
        }
    }
}

/// Upstream `GitUtil.parseRemoteBranch`: the remote is the shortest `/`-delimited prefix that names a
/// known remote; if none matches, the part before the first `/`.
pub fn parse_remote_branch(full_branch_name: &str, remotes: &[GitRemote]) -> GitRemoteBranch {
    let std_name = strip_refs_prefix(full_branch_name);
    let Some(first_slash) = std_name.find('/') else {
        return GitRemoteBranch::Svn {
            full_ref: full_branch_name.to_string(),
        };
    };
    let mut slash = Some(first_slash);
    while let Some(s) = slash {
        let remote_name = &std_name[..s];
        if remotes.iter().any(|r| r.name == remote_name) {
            return GitRemoteBranch::Standard {
                remote: remote_name.to_string(),
                name_at_remote: std_name[s + 1..].to_string(),
            };
        }
        slash = std_name[s + 1..].find('/').map(|i| s + 1 + i);
    }
    GitRemoteBranch::Standard {
        remote: std_name[..first_slash].to_string(),
        name_at_remote: std_name[first_slash + 1..].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_branch_names() {
        let b = GitLocalBranch::new("refs/heads/feature/x");
        assert_eq!(b.name(), "feature/x");
        assert_eq!(b.full_name(), "refs/heads/feature/x");
    }

    #[test]
    fn remote_branch_parsing() {
        let remotes = vec![GitRemote::new("origin"), GitRemote::new("team/a")];
        assert_eq!(
            parse_remote_branch("refs/remotes/origin/main", &remotes),
            GitRemoteBranch::Standard {
                remote: "origin".into(),
                name_at_remote: "main".into()
            }
        );
        assert_eq!(
            parse_remote_branch("refs/remotes/team/a/feat", &remotes).name(),
            "team/a/feat"
        );
        let b = parse_remote_branch("refs/remotes/team/a/feat", &remotes);
        assert!(matches!(b, GitRemoteBranch::Standard { ref remote, .. } if remote == "team/a"));
        let unknown = parse_remote_branch("refs/remotes/gone/x/y", &remotes);
        assert_eq!(
            unknown,
            GitRemoteBranch::Standard {
                remote: "gone".into(),
                name_at_remote: "x/y".into()
            }
        );
        assert_eq!(
            parse_remote_branch("refs/remotes/trunk", &remotes).full_name(),
            "refs/remotes/trunk"
        );
    }
}
