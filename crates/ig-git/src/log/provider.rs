//! The git log provider: recent commits with refs, all commits, metadata and the current user.
//!
//! Port of the data-reading parts of `plugins/git4idea/backend/src/log/GitLogProvider.kt`,
//! `plugins/git4idea/backend/src/log/VcsLogObjectsFactoryExt.kt` (`createBranchesRefs`) and
//! `plugins/git4idea/backend/src/GitUserRegistry.java` with `GitConfigUtil.getValue`.
//!
//! Deviations:
//! - Repository state comes from [`GitRepositoryManager`], not from IDE-wide repository objects
//!   updated by file-system events. Explicit-update contract: callers (or a future watcher) call
//!   [`GitRepositoryManager::update`] to pick up branch or HEAD changes; the state is also
//!   reread when refs are refreshed, and a root without commits is reread automatically on
//!   every read until it has some.
//! - Branch refs are created in name order (upstream: `HashSet` order); refs and commits gathered
//!   in sets upstream keep their first-seen order.
//! - Not ported: the experimental refs collection (`GitLogExperimentalProvider`), filters
//!   (`getCommitsMatchingFilter`), full details, containing branches, refresh subscriptions,
//!   `resolveReference`, submodule root correction and the debug-only data validation.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ig_log::data::{
    DetailedLogData, Hash, LogData, Requirements, TimedVcsCommit, VcsCommitMetadata, VcsException,
    VcsLogProvider, VcsLogRefManager, VcsRef, VcsRefsContainer, VcsRoot, VcsUser,
    sort_by_date_topo_order,
};

use crate::command::GitCommand;
use crate::error::GitError;
use crate::log::ref_manager::{GitRefManager, HEAD, LOCAL_BRANCH, REMOTE_BRANCH, TAG};
use crate::log::repository_manager::{GitRepoInfo, GitRepositoryManager};
use crate::log::util::{
    LOG_ALL, RefsByNameAndType, collect_metadata, collect_metadata_with_params, get_all_tags,
    read_timed_commits,
};
use crate::runner::Git;

/// `GitConfigUtil.USER_NAME` / `USER_EMAIL`.
const USER_NAME: &str = "user.name";
const USER_EMAIL: &str = "user.email";

fn vcs_error(e: GitError) -> VcsException {
    VcsException(e.to_string())
}

/// The git [`VcsLogProvider`] (`GitLogProvider`). One provider serves all git roots.
pub struct GitLogProvider {
    git: Arc<Git>,
    repository_manager: Arc<GitRepositoryManager>,
    ref_manager: Arc<dyn VcsLogRefManager>,
    users: Mutex<std::collections::HashMap<PathBuf, VcsUser>>,
}

impl GitLogProvider {
    pub fn new(git: Arc<Git>) -> GitLogProvider {
        GitLogProvider::with_repository_manager(Arc::new(GitRepositoryManager::new(git)))
    }

    pub fn with_repository_manager(
        repository_manager: Arc<GitRepositoryManager>,
    ) -> GitLogProvider {
        GitLogProvider {
            git: repository_manager.git().clone(),
            ref_manager: Arc::new(GitRefManager::new(repository_manager.clone())),
            repository_manager,
            users: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn repository_manager(&self) -> &Arc<GitRepositoryManager> {
        &self.repository_manager
    }

    /// `readCurrentTagNames`.
    fn read_current_tag_names(&self, root: &Path) -> Result<HashSet<String>, VcsException> {
        Ok(get_all_tags(&self.git, root)
            .map_err(vcs_error)?
            .into_iter()
            .collect())
    }

    /// `loadSomeCommitsOnTaggedBranches`: up to `commit_count` commits from each tag.
    fn load_some_commits_on_tagged_branches(
        &self,
        root: &VcsRoot,
        commit_count: usize,
        unmatched_tags: &[String],
    ) -> Result<DetailedLogData, VcsException> {
        let max_count = format!("--max-count={commit_count}");
        let mut refs: Vec<VcsRef> = Vec::new();
        let mut seen_refs: HashSet<VcsRef> = HashSet::new();
        let mut commits: Vec<VcsCommitMetadata> = Vec::new();
        let mut seen_commits = HashSet::new();
        for tag in unmatched_tags {
            let log_data = collect_metadata_with_params(&self.git, root, &[&max_count, tag])
                .map_err(vcs_error)?;
            for r in log_data.refs {
                if seen_refs.insert(r.clone()) {
                    refs.push(r);
                }
            }
            for c in log_data.commits {
                if seen_commits.insert(c.clone()) {
                    commits.push(c);
                }
            }
        }
        Ok(DetailedLogData { commits, refs })
    }

    /// `GitUserRegistry.readCurrentUser`.
    fn read_current_user(&self, root: &Path) -> Option<VcsUser> {
        let user_name = self
            .get_config_value(root, USER_NAME)
            .filter(|n| !n.is_empty())?;
        let user_email = self.get_config_value(root, USER_EMAIL).unwrap_or_default();
        Some(VcsUser::create_user(&user_name, &user_email))
    }

    /// `GitConfigUtil.getValue`: `git config --null --get <key>`; `None` if unset.
    fn get_config_value(&self, root: &Path, key: &str) -> Option<String> {
        let mut handler = self.git.handler(root, GitCommand::CONFIG);
        handler.set_silent(true);
        handler.add_parameters(["--null", "--get", key]);
        let result = self.git.run_command(handler);
        if !result.success() {
            return None;
        }
        let output = result.output_as_joined_string();
        output.find('\0').map(|pos| output[..pos].to_string())
    }
}

/// Converts a validated git hash to the log hash type.
fn to_log_hash(h: &crate::hash::Hash) -> Hash {
    Hash::try_build(h.as_str()).expect("validated by ig_git::hash::Hash")
}

/// `createBranchesRefs`: local and remote branches and HEAD of the repository state.
fn create_branches_refs(info: &GitRepoInfo, root: &VcsRoot) -> Vec<VcsRef> {
    let mut locals: Vec<_> = info.state.local_branches.iter().collect();
    locals.sort_by(|a, b| a.0.name().cmp(b.0.name()));
    let mut remotes: Vec<_> = info
        .state
        .remote_branches
        .iter()
        .map(|(b, h)| (b.name(), h))
        .collect();
    remotes.sort();
    let mut refs: Vec<VcsRef> = locals
        .into_iter()
        .map(|(branch, hash)| {
            VcsRef::new(to_log_hash(hash), branch.name(), LOCAL_BRANCH, root.clone())
        })
        .collect();
    refs.extend(
        remotes
            .into_iter()
            .map(|(name, hash)| VcsRef::new(to_log_hash(hash), &name, REMOTE_BRANCH, root.clone())),
    );
    if let Some(revision) = &info.state.current_revision {
        refs.push(VcsRef::new(
            to_log_hash(revision),
            "HEAD",
            HEAD,
            root.clone(),
        ));
    }
    refs
}

impl VcsLogProvider for GitLogProvider {
    fn read_first_block(
        &self,
        root: &VcsRoot,
        requirements: &Requirements<'_>,
    ) -> Result<DetailedLogData, VcsException> {
        if self
            .repository_manager
            .get_repository_unless_fresh(root)
            .is_none()
        {
            return Ok(DetailedLogData::default());
        }

        // need to query more to sort them manually; this doesn't affect performance: it is equal
        // for -1000 and -2000
        let commit_count = requirements.commit_count * 2;
        let max_count = format!("--max-count={commit_count}");
        // NB: not specifying --tags, because it introduces great slowdown if there are many tags,
        // but makes sense only if there are heads without branch or HEAD labels (rare case).
        // Such cases are partially handled below.
        let params = ["HEAD", "--branches", "--remotes", &max_count];

        let (refresh, is_refresh_refs) = requirements
            .ex
            .as_ref()
            .map_or((false, false), |ex| (ex.refresh, ex.refresh_refs));

        let data = collect_metadata_with_params(&self.git, root, &params).map_err(vcs_error)?;

        let safe_refs = data.refs;
        let mut all_refs = RefsByNameAndType::from_refs(safe_refs.iter().cloned());
        if is_refresh_refs {
            let repository = self.repository_manager.update(root);
            for branch in create_branches_refs(&repository, root) {
                all_refs.add(branch);
            }
        }

        let mut all_details: Vec<VcsCommitMetadata> = data.commits;
        if let (true, true, Some(ex)) = (refresh, is_refresh_refs, requirements.ex.as_ref()) {
            // on refresh: get new tags, which point to commits not from the first block; then get
            // history, walking down just from these tags. On init: just ignore such tagged-only
            // branches. The price for speed-up.
            let current_tag_names = self.read_current_tag_names(root)?;
            add_old_still_existing_tags(&mut all_refs, &current_tag_names, ex.previous_refs);

            let mut seen: HashSet<VcsCommitMetadata> = HashSet::new();
            all_details.retain(|c| seen.insert(c.clone()));

            let previous_tags: HashSet<&str> = ex.previous_refs.tags().map(|r| r.name()).collect();
            let safe_tags: HashSet<&str> = safe_refs
                .iter()
                .filter(|r| r.ref_type() == TAG)
                .map(|r| r.name())
                .collect();
            let mut new_unmatched_tags: Vec<String> = current_tag_names
                .iter()
                .filter(|t| !previous_tags.contains(t.as_str()) && !safe_tags.contains(t.as_str()))
                .cloned()
                .collect();
            new_unmatched_tags.sort();

            if !new_unmatched_tags.is_empty() {
                let commits_from_tags = self.load_some_commits_on_tagged_branches(
                    root,
                    commit_count,
                    &new_unmatched_tags,
                )?;
                for commit in commits_from_tags.commits {
                    if seen.insert(commit.clone()) {
                        all_details.push(commit);
                    }
                }
                for r in commits_from_tags.refs {
                    all_refs.add(r);
                }
            }
        }

        let mut sorted_commits = sort_by_date_topo_order(all_details);
        sorted_commits.truncate(requirements.commit_count);
        Ok(DetailedLogData {
            commits: sorted_commits,
            refs: all_refs.into_vec(),
        })
    }

    fn read_all_hashes(
        &self,
        root: &VcsRoot,
        commit_consumer: &mut dyn FnMut(TimedVcsCommit),
    ) -> Result<LogData, VcsException> {
        if self
            .repository_manager
            .get_repository_unless_fresh(root)
            .is_none()
        {
            return Ok(LogData::default());
        }
        let mut parameters: Vec<&str> = LOG_ALL.to_vec();
        parameters.push("--date-order");

        let mut users = HashSet::new();
        let mut refs = HashSet::new();
        read_timed_commits(
            &self.git,
            root,
            &[],
            &parameters,
            Some(&mut |user| {
                users.insert(user);
            }),
            Some(&mut |vcs_ref| {
                refs.insert(vcs_ref);
            }),
            commit_consumer,
        )
        .map_err(vcs_error)?;
        Ok(LogData { refs, users })
    }

    fn read_metadata(
        &self,
        root: &VcsRoot,
        hashes: &[String],
        consumer: &mut dyn FnMut(VcsCommitMetadata),
    ) -> Result<(), VcsException> {
        collect_metadata(&self.git, root, hashes, consumer).map_err(vcs_error)
    }

    fn reference_manager(&self) -> Arc<dyn VcsLogRefManager> {
        self.ref_manager.clone()
    }

    /// `GitUserRegistry.getOrReadUser`: cached once read.
    fn get_current_user(&self, root: &VcsRoot) -> Result<Option<VcsUser>, VcsException> {
        let mut users = self.users.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(user) = users.get(&**root) {
            return Ok(Some(user.clone()));
        }
        let user = self.read_current_user(root);
        if let Some(user) = &user {
            users.insert(root.to_path_buf(), user.clone());
        }
        Ok(user)
    }

    fn get_current_branch(&self, root: &VcsRoot) -> Option<String> {
        let repository = self
            .repository_manager
            .get_repository_for_root_quick(root)?;
        match repository.current_branch_name() {
            None if repository.state.current_revision.is_some() => Some("HEAD".to_string()),
            name => name.map(str::to_string),
        }
    }

    fn is_full_hash(&self, root: &VcsRoot, hash: &str) -> bool {
        let Some(repository) = self.repository_manager.get_repository_for_root_quick(root) else {
            return false;
        };
        hash.len() == repository.full_hash_length && hash.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

/// `addOldStillExistingTags`.
fn add_old_still_existing_tags(
    all_refs: &mut RefsByNameAndType,
    current_tags: &HashSet<String>,
    previous_refs: &dyn VcsRefsContainer,
) {
    for vcs_ref in previous_refs.all_refs() {
        if !all_refs.contains(vcs_ref) && current_tags.contains(vcs_ref.name()) {
            all_refs.add(vcs_ref.clone());
        }
    }
}
