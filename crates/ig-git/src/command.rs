//! Port of `plugins/git4idea/backend/src/commands/GitCommand.java`.

/// How a command interacts with the per-project command lock (upstream `GitCommand.LockingPolicy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockingPolicy {
    Read,
    /// Commands with non-mandatory write side effects (e.g. `git status`) that can run without
    /// locking the index when `GIT_OPTIONAL_LOCKS=0` is supported.
    ReadOptionalLocking,
    Write,
}

/// A git command name plus its locking policy. Write commands exclude each other but not reads.
/// The policy can differ per invocation (e.g. `stash list` is read-only): use
/// [`GitCommand::read_locking_command`] for that, with care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitCommand {
    name: &'static str,
    locking: LockingPolicy,
}

impl GitCommand {
    const fn read(name: &'static str) -> Self {
        Self {
            name,
            locking: LockingPolicy::Read,
        }
    }
    const fn read_optional(name: &'static str) -> Self {
        Self {
            name,
            locking: LockingPolicy::ReadOptionalLocking,
        }
    }
    const fn write(name: &'static str) -> Self {
        Self {
            name,
            locking: LockingPolicy::Write,
        }
    }

    pub const ADD: Self = Self::write("add");
    pub const BLAME: Self = Self::read("blame");
    pub const BRANCH: Self = Self::read("branch");
    pub const FOR_EACH_REF: Self = Self::read("for-each-ref");
    pub const CAT_FILE: Self = Self::read("cat-file");
    pub const CHECKOUT: Self = Self::write("checkout");
    pub const CHECK_ATTR: Self = Self::read("check-attr");
    pub const CHECK_IGNORE: Self = Self::read("check-ignore");
    pub const COMMIT: Self = Self::write("commit");
    pub const COMMIT_TREE: Self = Self::write("commit-tree");
    pub const CONFIG: Self = Self::read("config");
    pub const CHERRY: Self = Self::read("cherry");
    pub const CHERRY_PICK: Self = Self::write("cherry-pick");
    /// Write, but can't interfere with any other command, so treated as read (as upstream).
    pub const CLONE: Self = Self::read("clone");
    pub const DIFF: Self = Self::read("diff");
    /// Read: fetch doesn't modify the index.
    pub const FETCH: Self = Self::read("fetch");
    pub const INIT: Self = Self::write("init");
    pub const LOG: Self = Self::read("log");
    pub const SHORTLOG: Self = Self::read("shortlog");
    pub const LS_FILES: Self = Self::read_optional("ls-files");
    pub const LS_TREE: Self = Self::read("ls-tree");
    pub const LS_REMOTE: Self = Self::read("ls-remote");
    pub const MERGE: Self = Self::write("merge");
    pub const MERGE_BASE: Self = Self::read("merge-base");
    pub const MERGE_TREE: Self = Self::read("merge-tree");
    pub const MV: Self = Self::write("mv");
    pub const NOTES: Self = Self::read("notes");
    pub const PULL: Self = Self::write("pull");
    /// Read: push doesn't modify the index. We still benefit from COMMIT & Co being write
    /// commands, which prevents HEAD from moving during a push.
    pub const PUSH: Self = Self::read("push");
    pub const REBASE: Self = Self::write("rebase");
    pub const REMOTE: Self = Self::read("remote");
    pub const RESET: Self = Self::write("reset");
    pub const RESTORE: Self = Self::write("restore");
    pub const REVERT: Self = Self::write("revert");
    pub const REV_LIST: Self = Self::read("rev-list");
    pub const REV_PARSE: Self = Self::read("rev-parse");
    pub const REF_LOG: Self = Self::read("reflog");
    pub const RM: Self = Self::write("rm");
    pub const SHOW: Self = Self::read("show");
    pub const STASH: Self = Self::write("stash");
    pub const STATUS: Self = Self::read_optional("status");
    /// Write in the submodule, not in the current root (the submodule's parent).
    pub const SUBMODULE: Self = Self::write("submodule");
    pub const TAG: Self = Self::read("tag");
    pub const UPDATE_INDEX: Self = Self::write("update-index");
    pub const UPDATE_REF: Self = Self::write("update-ref");
    pub const HASH_OBJECT: Self = Self::write("hash-object");
    pub const HOOK: Self = Self::write("hook");
    pub const VERSION: Self = Self::read("version");
    pub const WORKTREE: Self = Self::read("worktree");

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn locking_policy(&self) -> LockingPolicy {
        self.locking
    }

    /// Same command, but forced to read locking (upstream `readLockingCommand`).
    pub fn read_locking_command(self) -> Self {
        Self {
            locking: LockingPolicy::Read,
            ..self
        }
    }

    /// Same command, but forced to write locking (upstream `writeLockingCommand`).
    pub fn write_locking_command(self) -> Self {
        Self {
            locking: LockingPolicy::Write,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policies_match_upstream() {
        assert_eq!(GitCommand::COMMIT.locking_policy(), LockingPolicy::Write);
        assert_eq!(GitCommand::FETCH.locking_policy(), LockingPolicy::Read);
        assert_eq!(
            GitCommand::STATUS.locking_policy(),
            LockingPolicy::ReadOptionalLocking
        );
        assert_eq!(GitCommand::STATUS.name(), "status");
        assert_eq!(
            GitCommand::FETCH.write_locking_command().locking_policy(),
            LockingPolicy::Write
        );
    }

    #[test]
    fn locking_overrides_keep_the_name() {
        let list = GitCommand::STASH.read_locking_command();
        assert_eq!(list.locking_policy(), LockingPolicy::Read);
        assert_eq!(list.name(), "stash");
        assert_eq!(GitCommand::PUSH.locking_policy(), LockingPolicy::Read);
        assert_eq!(GitCommand::CLONE.locking_policy(), LockingPolicy::Read);
        assert_eq!(
            GitCommand::LS_FILES.locking_policy(),
            LockingPolicy::ReadOptionalLocking
        );
    }
}
