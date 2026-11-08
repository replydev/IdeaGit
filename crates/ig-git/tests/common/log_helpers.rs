//! Helpers for the log tests: the upstream `GitExecutor`/`GitTestUtil` functions they use
//! (`tac`, `last`, `readAllRefs` with `RefParser`).

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use ig_git::log::provider::GitLogProvider;
use ig_git::log::ref_manager::get_ref_type;
use ig_git::refs::strip_refs_prefix;
use ig_log::data::{Hash, VcsRef, VcsRoot, VcsUser, vcs_root};

use super::{git, git_output_with_env, runner};

/// The identity of the commits made by [`git`].
pub fn default_user() -> VcsUser {
    VcsUser::create_user("Test", "test@example.com")
}

/// `git init` on branch `master` (via `symbolic-ref`, `init -b` needs git 2.28) with one commit (the upstream test repositories use `master`).
pub fn init_master_repo(dir: &Path) -> VcsRoot {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q"]);
    git(dir, &["symbolic-ref", "HEAD", "refs/heads/master"]);
    tac(dir, "initial.txt");
    vcs_root(dir)
}

/// `last`: the hash of HEAD.
pub fn last(dir: &Path) -> String {
    git(dir, &["rev-parse", "HEAD"]).trim().to_string()
}

/// `tac`: touch (with random content), add and commit; returns the new HEAD.
pub fn tac(dir: &Path, file: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let content = format!("content{}", COUNTER.fetch_add(1, Ordering::Relaxed));
    std::fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", &format!("Touched {file}")]);
    last(dir)
}

/// A provider for the test repositories (isolated git).
pub fn provider() -> GitLogProvider {
    GitLogProvider::new(Arc::new(runner()))
}

/// `readAllRefs`: refs from `git log --branches --tags --no-walk --format=%H%d --decorate=full`,
/// parsed like `RefParser.parseCommitRefs`.
pub fn read_all_refs(dir: &Path, root: &VcsRoot) -> HashSet<VcsRef> {
    let output = git(
        dir,
        &[
            "log",
            "--branches",
            "--tags",
            "--no-walk",
            "--format=%H%d",
            "--decorate=full",
        ],
    );
    let mut result = HashSet::new();
    for line in output.lines() {
        let Some(first_space_index) = line.find(' ') else {
            continue;
        };
        let hash = Hash::build(&line[..first_space_index]);
        let ref_paths = &line[first_space_index + 2..line.len() - 1];
        for long_ref_path in ref_paths.split(", ").flat_map(|s| s.split(" -> ")) {
            let name = long_ref_path.strip_prefix("tag: ").unwrap_or(long_ref_path);
            let ref_type = get_ref_type(name);
            result.insert(VcsRef::new(
                hash.clone(),
                strip_refs_prefix(name),
                ref_type,
                root.clone(),
            ));
        }
    }
    result
}

/// Commits a change of `file` with author and committer date `epoch_seconds`; returns the hash.
pub fn commit_at(dir: &Path, file: &str, epoch_seconds: i64) -> String {
    std::fs::write(dir.join(file), format!("{file} at {epoch_seconds}")).unwrap();
    git(dir, &["add", file]);
    let date = format!("@{epoch_seconds} +0000");
    let out = git_output_with_env(
        dir,
        &["commit", "-q", "-m", &format!("{file} at {epoch_seconds}")],
        &[("GIT_AUTHOR_DATE", &date), ("GIT_COMMITTER_DATE", &date)],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    last(dir)
}

/// `git init` on branch `master` (see [`init_master_repo`]) with a first commit at `epoch_seconds`.
pub fn init_repo_at(dir: &Path, epoch_seconds: i64) -> VcsRoot {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q"]);
    git(dir, &["symbolic-ref", "HEAD", "refs/heads/master"]);
    commit_at(dir, "initial.txt", epoch_seconds);
    vcs_root(dir)
}
