//! Port of `plugins/git4idea/backend/src/repo/GitRepositoryReader.kt` (branch state, HEAD and
//! repository state from `.git` files, with a `git for-each-ref` fallback for reftable repositories).
//!
//! Not yet ported: hooks info and shallow detection (needed by the commit/push plans).

use std::collections::HashMap;

use crate::branch::{GitLocalBranch, GitRemoteBranch, ORIGIN_HEAD, parse_remote_branch};
use crate::command::GitCommand;
use crate::config::GitRemote;
use crate::hash::Hash;
use crate::refs::{
    REFS_HEADS_PREFIX, REFS_REMOTES_PREFIX, get_target, parse_branches_line, read_from_refs_files,
    resolve_refs,
};
use crate::repo_files::{GitRepositoryFiles, try_load_file};
use crate::runner::Git;

/// Upstream `Repository.State`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryState {
    Normal,
    Merging,
    Rebasing,
    Detached,
    /// Cherry-pick in progress.
    Grafting,
    Reverting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitBranchState {
    pub current_revision: Option<String>,
    pub current_branch: Option<GitLocalBranch>,
    pub state: RepositoryState,
    pub local_branches: HashMap<GitLocalBranch, Hash>,
    pub remote_branches: HashMap<GitRemoteBranch, Hash>,
}

enum HeadInfo {
    Unknown,
    Branch(GitLocalBranch),
    DetachedHead(Hash),
}

struct GitBranches {
    local: HashMap<GitLocalBranch, Hash>,
    remote: HashMap<GitRemoteBranch, Hash>,
}

const DETACHED_HEAD: &str = "detached HEAD";

pub struct GitRepositoryReader<'a> {
    files: &'a GitRepositoryFiles,
    git: &'a Git,
}

impl<'a> GitRepositoryReader<'a> {
    pub fn new(files: &'a GitRepositoryFiles, git: &'a Git) -> Self {
        Self { files, git }
    }

    pub fn read_state(&self, remotes: &[GitRemote]) -> GitBranchState {
        let head = self.read_head();
        let (branches, head_info) = match head {
            // Registry `git.read.branches.from.disk` is on by default upstream.
            Some(head_info) => (self.read_branches(remotes), head_info),
            None => {
                // reftable format: ask git
                let (branches, from_git) = self.read_branches_from_git(remotes);
                let head_info = match from_git {
                    HeadInfo::Unknown => match self.resolve_head_revision() {
                        Some(hash) => HeadInfo::DetachedHead(hash),
                        None => HeadInfo::Unknown,
                    },
                    other => other,
                };
                (branches, head_info)
            }
        };

        let state = self.read_repository_state(matches!(head_info, HeadInfo::Branch(_)));
        let (current_branch, current_revision) = self.parse_head_state(head_info, state, &branches);
        GitBranchState {
            current_revision,
            current_branch,
            state,
            local_branches: branches.local,
            remote_branches: branches.remote,
        }
    }

    fn parse_head_state(
        &self,
        head_info: HeadInfo,
        state: RepositoryState,
        branches: &GitBranches,
    ) -> (Option<GitLocalBranch>, Option<String>) {
        if let HeadInfo::Branch(branch) = head_info {
            let current = fix_current_branch_case(branch, branches);
            let revision = branches.local.get(&current).map(|h| h.as_str().to_string());
            return (Some(current), revision);
        }
        let current_branch = if state == RepositoryState::Rebasing {
            self.find_rebase_branch()
                .map(|b| fix_current_branch_case(b, branches))
        } else {
            None
        };
        match head_info {
            HeadInfo::DetachedHead(hash) => (current_branch, Some(hash.as_str().to_string())),
            _ => (current_branch, None),
        }
    }

    fn read_repository_state(&self, is_on_branch: bool) -> RepositoryState {
        if self.files.merge_head_file().exists() {
            return RepositoryState::Merging;
        }
        if self.files.rebase_apply_dir().exists() || self.files.rebase_merge_dir().exists() {
            return RepositoryState::Rebasing;
        }
        if !is_on_branch {
            return RepositoryState::Detached;
        }
        if self.files.cherry_pick_head().exists() {
            return RepositoryState::Grafting;
        }
        if self.files.revert_head().exists() {
            return RepositoryState::Reverting;
        }
        if let Some(state) = self.sequencer_todo_based_state() {
            return state;
        }
        RepositoryState::Normal
    }

    /// Upstream `GitSequencerTodoReader`: cherry-pick/revert of several commits can be in progress
    /// with `CHERRY_PICK_HEAD`/`REVERT_HEAD` missing.
    fn sequencer_todo_based_state(&self) -> Option<RepositoryState> {
        let content = std::fs::read_to_string(self.files.sequencer_todo_file()).ok()?;
        if content.starts_with("pick") {
            Some(RepositoryState::Grafting)
        } else if content.starts_with("revert") {
            Some(RepositoryState::Reverting)
        } else {
            None
        }
    }

    fn find_rebase_branch(&self) -> Option<GitLocalBranch> {
        let name = [self.files.rebase_apply_dir(), self.files.rebase_merge_dir()]
            .iter()
            .filter(|dir| dir.exists())
            .find_map(|dir| try_load_file(&dir.join("head-name")).ok())?;
        (name != DETACHED_HEAD).then(|| GitLocalBranch::new(&name))
    }

    /// `None` means the repository uses the reftable format.
    fn read_head(&self) -> Option<HeadInfo> {
        let Ok(content) = try_load_file(&self.files.head_file()) else {
            return Some(HeadInfo::Unknown);
        };
        if content == "ref: refs/heads/.invalid" {
            return None;
        }
        if let Some(hash) = Hash::parse(&content) {
            return Some(HeadInfo::DetachedHead(hash));
        }
        match get_target(&content) {
            Some(target) => Some(HeadInfo::Branch(GitLocalBranch::new(&target))),
            None => Some(HeadInfo::Unknown),
        }
    }

    fn read_branches(&self, remotes: &[GitRemote]) -> GitBranches {
        // packed-refs first, so loose refs override them
        let mut data: HashMap<String, String> = HashMap::new();
        if let Ok(content) = try_load_file(&self.files.packed_refs()) {
            for line in content.lines() {
                if let Some((name, hash)) = parse_branches_line(line) {
                    data.insert(name, hash);
                }
            }
        }
        data.extend(read_from_refs_files(
            &self.files.refs_heads_dir(),
            REFS_HEADS_PREFIX,
        ));
        data.extend(read_from_refs_files(
            &self.files.refs_remotes_dir(),
            REFS_REMOTES_PREFIX,
        ));
        create_branches_from_data(remotes, resolve_refs(&data))
    }

    fn read_branches_from_git(&self, remotes: &[GitRemote]) -> (GitBranches, HeadInfo) {
        let root = self.files.root_dir();
        let mut h = self.git.handler(root, GitCommand::FOR_EACH_REF);
        h.add_parameters([
            "refs/heads/**",
            "refs/remotes/**",
            "--no-color",
            "--format=%(refname)\t%(objectname)\t%(HEAD)",
        ]);
        let result = self.git.run_command(h);
        if !result.success() {
            return (
                GitBranches {
                    local: HashMap::new(),
                    remote: HashMap::new(),
                },
                HeadInfo::Unknown,
            );
        }
        let mut resolved: HashMap<String, Hash> = HashMap::new();
        let mut head: Option<HeadInfo> = None;
        for line in &result.output {
            let mut parts = line.split('\t');
            let (Some(name), Some(hash)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Some(hash) = Hash::parse(hash) else {
                continue;
            };
            if parts.next() == Some("*") {
                head = Some(HeadInfo::Branch(GitLocalBranch::new(name)));
            }
            resolved.insert(name.to_string(), hash);
        }
        let branches = create_branches_from_data(remotes, resolved);
        if let Some(head) = head {
            return (branches, head);
        }
        if branches.local.is_empty() {
            // fresh repository: no commits yet, but HEAD names a branch
            if let Some(current) = self.resolve_current_branch() {
                return (
                    GitBranches {
                        local: HashMap::new(),
                        remote: branches.remote,
                    },
                    HeadInfo::Branch(current),
                );
            }
        }
        (branches, HeadInfo::Unknown)
    }

    fn resolve_current_branch(&self) -> Option<GitLocalBranch> {
        let mut h = self.git.handler(self.files.root_dir(), GitCommand::BRANCH);
        h.add_parameters(["--show-current", "--no-color"]);
        let output = self.git.run_command(h).output_or_error().ok()?;
        let output = output.trim();
        (!output.is_empty()).then(|| GitLocalBranch::new(output))
    }

    fn resolve_head_revision(&self) -> Option<Hash> {
        let mut h = self
            .git
            .handler(self.files.root_dir(), GitCommand::REV_PARSE);
        h.add_parameters(["--verify", "HEAD^{commit}"]);
        let output = self.git.run_command(h).output_or_error().ok()?;
        let output = output.trim();
        if output.len() == 40 || output.len() == 64 {
            Hash::parse(output)
        } else {
            None
        }
    }
}

fn fix_current_branch_case(branch: GitLocalBranch, branches: &GitBranches) -> GitLocalBranch {
    if branches.local.contains_key(&branch) || cfg!(target_os = "linux") {
        return branch;
    }
    branches
        .local
        .keys()
        .find(|b| b.name().eq_ignore_ascii_case(branch.name()))
        .cloned()
        .unwrap_or(branch)
}

fn create_branches_from_data(remotes: &[GitRemote], data: HashMap<String, Hash>) -> GitBranches {
    let mut local = HashMap::new();
    let mut remote = HashMap::new();
    for (ref_name, hash) in data {
        if ref_name.starts_with(REFS_HEADS_PREFIX) {
            local.insert(GitLocalBranch::new(&ref_name), hash);
        } else if ref_name.starts_with(REFS_REMOTES_PREFIX) {
            let branch = parse_remote_branch(&ref_name, remotes);
            if branch.name() == ORIGIN_HEAD {
                continue; // fake "origin/HEAD" reference
            }
            remote.insert(branch, hash);
        }
    }
    GitBranches { local, remote }
}
