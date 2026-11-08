//! IdeaGit integration tests: loading one or several repositories into the log data pack and
//! refreshing it after new commits, branches, tags and resets.
//!
//! A refresh must give the same log as a fresh full load, through the joiner (no second
//! `readAllHashes`).

mod common;

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use common::git;
use common::log_helpers::{commit_at, init_repo_at, provider};
use ig_git::log::provider::GitLogProvider;
use ig_git::log::ref_manager::{LOCAL_BRANCH, TAG};
use ig_log::data::{
    DetailedLogData, InMemoryStorage, LogData, LogProviders, Requirements, TimedVcsCommit,
    VcsCommitMetadata, VcsException, VcsLogGraphData, VcsLogProvider, VcsLogRefManager,
    VcsLogRefresher, VcsLogStorage, VcsRefsContainer, VcsRoot, VcsUser, vcs_root,
};
use ig_log::{GraphCommit, Options, PermanentGraph, VisibleGraph};

/// Counts the full-log reads of a provider.
struct CountingProvider {
    inner: GitLogProvider,
    read_all_hashes_count: AtomicUsize,
    read_first_block_count: AtomicUsize,
}

impl CountingProvider {
    fn new() -> Arc<CountingProvider> {
        Arc::new(CountingProvider {
            inner: provider(),
            read_all_hashes_count: AtomicUsize::new(0),
            read_first_block_count: AtomicUsize::new(0),
        })
    }
}

impl VcsLogProvider for CountingProvider {
    fn read_first_block(
        &self,
        root: &VcsRoot,
        requirements: &Requirements<'_>,
    ) -> Result<DetailedLogData, VcsException> {
        self.read_first_block_count.fetch_add(1, Ordering::SeqCst);
        self.inner.read_first_block(root, requirements)
    }
    fn read_all_hashes(
        &self,
        root: &VcsRoot,
        consumer: &mut dyn FnMut(TimedVcsCommit),
    ) -> Result<LogData, VcsException> {
        self.read_all_hashes_count.fetch_add(1, Ordering::SeqCst);
        self.inner.read_all_hashes(root, consumer)
    }
    fn read_metadata(
        &self,
        root: &VcsRoot,
        hashes: &[String],
        consumer: &mut dyn FnMut(VcsCommitMetadata),
    ) -> Result<(), VcsException> {
        self.inner.read_metadata(root, hashes, consumer)
    }
    fn reference_manager(&self) -> Arc<dyn VcsLogRefManager> {
        self.inner.reference_manager()
    }
    fn get_current_user(&self, root: &VcsRoot) -> Result<Option<VcsUser>, VcsException> {
        self.inner.get_current_user(root)
    }
    fn get_current_branch(&self, root: &VcsRoot) -> Option<String> {
        self.inner.get_current_branch(root)
    }
}

/// Commits as (root, hash, parent hashes, timestamp) and refs as (root, name, type, hash).
type Snapshot = (
    Vec<(String, String, Vec<String>, i64)>,
    BTreeSet<(String, String, String, String)>,
);

fn snapshot(pack: &VcsLogGraphData, storage: &dyn VcsLogStorage) -> Snapshot {
    assert!(pack.load_error().is_none(), "{pack:?}");
    let id = |index: &i32| storage.get_commit_id(*index).unwrap();
    let commits = pack
        .permanent_graph()
        .map(|graph| {
            graph
                .all_commits()
                .iter()
                .map(|c| {
                    let commit = id(c.id());
                    let parents = c.parents().iter().map(|p| id(p).hash.as_string()).collect();
                    (
                        commit.root.display().to_string(),
                        commit.hash.as_string(),
                        parents,
                        c.timestamp(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let refs = pack
        .refs_model()
        .all_refs()
        .map(|r| {
            (
                r.root().display().to_string(),
                r.name().to_string(),
                r.ref_type().name().to_string(),
                r.commit_hash().as_string(),
            )
        })
        .collect();
    (commits, refs)
}

struct Log {
    storage: Arc<InMemoryStorage>,
    refresher: VcsLogRefresher,
    provider: Arc<CountingProvider>,
    published: Arc<Mutex<Vec<bool>>>,
}

fn new_log(roots: &[&VcsRoot], recent_commit_count: usize) -> Log {
    let provider = CountingProvider::new();
    let storage = Arc::new(InMemoryStorage::new());
    let mut providers = LogProviders::new();
    for root in roots {
        providers.insert((*root).clone(), provider.clone() as Arc<dyn VcsLogProvider>);
    }
    let mut refresher = VcsLogRefresher::new(storage.clone(), providers, recent_commit_count);
    let published = Arc::new(Mutex::new(Vec::new()));
    let sink = published.clone();
    refresher.set_data_pack_handler(move |pack| sink.lock().unwrap().push(pack.is_full()));
    Log {
        storage,
        refresher,
        provider,
        published,
    }
}

/// The log as a fresh refresher loads it.
fn full_load_snapshot(roots: &[&VcsRoot]) -> Snapshot {
    let mut log = new_log(roots, 1000);
    let pack = log.refresher.initialize();
    snapshot(&pack, log.storage.as_ref())
}

fn assert_refresh_matches_full_load(log: &mut Log, roots: &[&VcsRoot], refreshed: &[&VcsRoot]) {
    let full_reads = log.provider.read_all_hashes_count.load(Ordering::SeqCst);
    let first_blocks = log.provider.read_first_block_count.load(Ordering::SeqCst);
    let refreshed: Vec<VcsRoot> = refreshed.iter().map(|r| (*r).clone()).collect();
    let pack = log.refresher.refresh(&refreshed);
    assert!(pack.is_full());
    assert_eq!(
        log.provider.read_all_hashes_count.load(Ordering::SeqCst),
        full_reads,
        "the refresh was joined, not reloaded"
    );
    assert!(
        log.provider.read_first_block_count.load(Ordering::SeqCst) > first_blocks,
        "a new block was read and joined"
    );
    assert_eq!(
        snapshot(&pack, log.storage.as_ref()),
        full_load_snapshot(roots)
    );
}

/// A history of `count` commits on master, one per 10 seconds from `start`.
fn linear_history(dir: &Path, start: i64, count: usize) -> i64 {
    let mut t = start;
    for i in 0..count {
        t += 10;
        commit_at(dir, &format!("f{i}.txt"), t);
    }
    t
}

#[test]
fn multi_repo_initial_load() {
    let tmp = tempfile::tempdir().unwrap();
    let a = init_repo_at(&tmp.path().join("a"), 1000);
    let b = init_repo_at(&tmp.path().join("b"), 1005);
    // interleaved timestamps
    for i in 0..5 {
        commit_at(&a, &format!("a{i}.txt"), 1010 + 20 * i);
        commit_at(&b, &format!("b{i}.txt"), 1020 + 20 * i);
    }
    git(&a, &["tag", "v1", "HEAD~2"]);
    git(&b, &["branch", "feature", "HEAD~1"]);

    let mut log = new_log(&[&a, &b], 3);
    let pack = log.refresher.initialize();
    assert_eq!(
        *log.published.lock().unwrap(),
        [false, true],
        "first block, then the full log"
    );

    let (commits, refs) = snapshot(&pack, log.storage.as_ref());
    assert_eq!(commits.len(), 12);
    let timestamps: Vec<i64> = commits.iter().map(|c| c.3).collect();
    let mut sorted = timestamps.clone();
    sorted.sort_by(|x, y| y.cmp(x));
    assert_eq!(timestamps, sorted, "repositories are joined by commit time");
    let roots: HashSet<&str> = commits.iter().map(|c| c.0.as_str()).collect();
    assert_eq!(roots.len(), 2);
    assert!(refs.contains(&(
        a.display().to_string(),
        "v1".into(),
        TAG.name().into(),
        commits_of(&commits, &a)[2].clone()
    )));
    assert!(
        refs.iter().any(|r| r.0 == b.display().to_string()
            && r.1 == "feature"
            && r.2 == LOCAL_BRANCH.name())
    );
    assert_eq!(pack.refs_model().refs_by_root().len(), 2);

    // the head of each repository is a branch head of the graph with its best ref
    let graph = pack.permanent_graph().unwrap();
    let a_head = log
        .storage
        .get_commit_index(&ig_log::data::Hash::build(&commits_of(&commits, &a)[0]), &a);
    let best = pack.refs_model().get_ref_for_head_commit(a_head).unwrap();
    assert_eq!(best.name(), "master");
    assert!(graph.get_containing_branches(&a_head).contains(&a_head));

    let visible = graph
        .create_visible_graph(Options::DEFAULT, None, None)
        .unwrap();
    assert_eq!(visible.visible_commit_count(), 12);
}

fn commits_of(commits: &[(String, String, Vec<String>, i64)], root: &VcsRoot) -> Vec<String> {
    commits
        .iter()
        .filter(|c| c.0 == root.display().to_string())
        .map(|c| c.1.clone())
        .collect()
}

#[test]
fn refresh_after_new_commit_is_joined() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("repo");
    let root = init_repo_at(&dir, 1000);
    let t = linear_history(&dir, 1000, 10);

    let mut log = new_log(&[&root], 3);
    log.refresher.initialize();
    assert_eq!(log.provider.read_all_hashes_count.load(Ordering::SeqCst), 1);

    commit_at(&dir, "new.txt", t + 10);
    assert_refresh_matches_full_load(&mut log, &[&root], &[&root]);
    assert_eq!(
        log.provider.read_first_block_count.load(Ordering::SeqCst),
        2
    );
}

#[test]
fn refresh_after_new_branch_and_tags_is_joined() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("repo");
    let root = init_repo_at(&dir, 1000);
    let mut t = linear_history(&dir, 1000, 10);

    let mut log = new_log(&[&root], 3);
    log.refresher.initialize();

    // a new branch with its own commits, merged back
    git(&dir, &["checkout", "-q", "-b", "feature", "HEAD~3"]);
    t += 10;
    commit_at(&dir, "feature1.txt", t);
    t += 10;
    commit_at(&dir, "feature2.txt", t);
    git(&dir, &["checkout", "-q", "master"]);
    let date = format!("@{} +0000", t + 10);
    let out = common::git_output_with_env(
        &dir,
        &["merge", "-q", "--no-ff", "feature", "-m", "merge feature"],
        &[("GIT_AUTHOR_DATE", &date), ("GIT_COMMITTER_DATE", &date)],
    );
    assert!(out.status.success());
    assert_refresh_matches_full_load(&mut log, &[&root], &[&root]);

    // a tag on an old commit, outside the first block
    git(&dir, &["tag", "old-tag", "HEAD~8"]);
    assert_refresh_matches_full_load(&mut log, &[&root], &[&root]);

    // a new branch pointing at an existing commit
    git(&dir, &["branch", "pointer", "HEAD~5"]);
    assert_refresh_matches_full_load(&mut log, &[&root], &[&root]);
}

#[test]
fn refresh_with_tagged_only_branch_is_joined() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("repo");
    let root = init_repo_at(&dir, 1000);
    let t = linear_history(&dir, 1000, 6);

    let mut log = new_log(&[&root], 3);
    log.refresher.initialize();

    // commits reachable only from a tag (`createTaggedBranch`)
    let head = common::log_helpers::last(&dir);
    commit_at(&dir, "c.txt", t + 10);
    commit_at(&dir, "d.txt", t + 20);
    git(&dir, &["tag", "poor-tag"]);
    git(&dir, &["reset", "-q", "--hard", &head]);
    assert_refresh_matches_full_load(&mut log, &[&root], &[&root]);
}

#[test]
fn refresh_after_reset_removes_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("repo");
    let root = init_repo_at(&dir, 1000);
    linear_history(&dir, 1000, 10);

    let mut log = new_log(&[&root], 3);
    log.refresher.initialize();

    git(&dir, &["reset", "-q", "--hard", "HEAD~2"]);
    assert_refresh_matches_full_load(&mut log, &[&root], &[&root]);
}

#[test]
fn refresh_of_one_root_in_multi_repo_log() {
    let tmp = tempfile::tempdir().unwrap();
    let a = init_repo_at(&tmp.path().join("a"), 1000);
    let b = init_repo_at(&tmp.path().join("b"), 1001);
    let t = linear_history(&a, 1000, 5);
    linear_history(&b, 1001, 5);

    let mut log = new_log(&[&a, &b], 3);
    log.refresher.initialize();

    let blocks = log.provider.read_first_block_count.load(Ordering::SeqCst);
    commit_at(&a, "new.txt", t + 100);
    git(&a, &["tag", "v2"]);
    assert_refresh_matches_full_load(&mut log, &[&a, &b], &[&a]);
    assert_eq!(
        log.provider.read_first_block_count.load(Ordering::SeqCst),
        blocks + 1,
        "only root a was re-read"
    );
}

#[test]
fn empty_root_in_multi_repo_gets_first_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let a = init_repo_at(&tmp.path().join("a"), 1000);
    linear_history(&a, 1000, 5);
    let b_dir = tmp.path().join("b");
    std::fs::create_dir_all(&b_dir).unwrap();
    // `git init` without commits, on master (via `symbolic-ref`, `init -b` needs git 2.28)
    git(&b_dir, &["init", "-q"]);
    git(&b_dir, &["symbolic-ref", "HEAD", "refs/heads/master"]);
    let b = vcs_root(&b_dir);

    let mut log = new_log(&[&a, &b], 3);
    log.refresher.initialize();
    // one full read per root
    let full_reads = log.provider.read_all_hashes_count.load(Ordering::SeqCst);
    assert_eq!(full_reads, 2);

    commit_at(&b_dir, "b0.txt", 2000);
    let pack = log.refresher.refresh(std::slice::from_ref(&b));
    // Matches upstream joiner semantics: the saved log of `b` is empty, so there is nothing to
    // join the new commits to, and the refresh falls back to a full reload.
    assert_eq!(
        log.provider.read_all_hashes_count.load(Ordering::SeqCst),
        full_reads + 2,
        "reloaded: every root is read again"
    );
    assert_eq!(
        snapshot(&pack, log.storage.as_ref()),
        full_load_snapshot(&[&a, &b])
    );
}

#[test]
fn refresh_beyond_the_recent_block_reloads_everything() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("repo");
    let root = init_repo_at(&dir, 1000);
    let t = linear_history(&dir, 1000, 3);

    let mut log = new_log(&[&root], 2);
    log.refresher.initialize();

    // 30 new commits: more than 2 and 5 * 2 recent commits, so the join fails twice
    linear_history(&dir, t, 30);
    let pack = log.refresher.refresh(std::slice::from_ref(&root));
    assert_eq!(
        log.provider.read_all_hashes_count.load(Ordering::SeqCst),
        2,
        "reloaded"
    );
    // 1 from initialize plus 2 join attempts (the recent block, then the larger one)
    assert_eq!(
        log.provider.read_first_block_count.load(Ordering::SeqCst),
        3,
        "initialize and two failed joins"
    );
    assert_eq!(
        snapshot(&pack, log.storage.as_ref()),
        full_load_snapshot(&[&root])
    );
}
