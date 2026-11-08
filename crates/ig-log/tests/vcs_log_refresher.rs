//! Port of `platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogRefresherTest.kt`.
//!
//! The refresher is synchronous: data packs are published in order on the calling thread, so the
//! upstream cases about blocking, cancellation and batching of queued requests
//! (`test_first_refresh_waits_for_full_log`,
//! `test_two_immediately_consecutive_refreshes_causes_only_one_data_pack_update`) don't apply.
//! The part of `test_reinitialize_makes_refresh_cancelled` that applies (initializing twice
//! publishes nothing more) is covered in `vcs_log_refresher_extra.rs`.

mod common;

use std::sync::{Arc, Mutex};

use common::test_vcs_log_provider::{BRANCH_TYPE, TestVcsLogProvider};
use common::timed_commit_parser::log;
use ig_log::data::{
    Hash, InMemoryStorage, LogProviders, TimedVcsCommit, VcsLogGraphData, VcsLogProvider,
    VcsLogRefresher, VcsLogStorage, VcsRef, VcsRoot, vcs_root,
};
use ig_log::{GraphCommit, PermanentGraph};

const RECENT_COMMITS_COUNT: usize = 2;

struct Fixture {
    project_root: VcsRoot,
    log_provider: Arc<TestVcsLogProvider>,
    storage: Arc<InMemoryStorage>,
    loader: VcsLogRefresher,
    data_packs: Arc<Mutex<Vec<Arc<VcsLogGraphData>>>>,
    commits: Vec<&'static str>,
}

fn set_up() -> Fixture {
    let project_root = vcs_root("/project");
    let log_provider = Arc::new(TestVcsLogProvider::default());
    let commits = vec!["3|-a2|-a1", "2|-a1|-a", "1|-a|-"];
    log_provider.append_history(log(&commits));
    log_provider.add_ref(create_branch_ref(&project_root, "master", "a2"));
    let storage = Arc::new(InMemoryStorage::new());
    let mut providers = LogProviders::new();
    providers.insert(
        project_root.clone(),
        log_provider.clone() as Arc<dyn VcsLogProvider>,
    );
    let mut loader = VcsLogRefresher::new(storage.clone(), providers, RECENT_COMMITS_COUNT);
    let data_packs = Arc::new(Mutex::new(Vec::new()));
    let sink = data_packs.clone();
    loader.set_data_pack_handler(move |pack| sink.lock().unwrap().push(pack.clone()));
    Fixture {
        project_root,
        log_provider,
        storage,
        loader,
        data_packs,
        commits,
    }
}

fn create_branch_ref(root: &VcsRoot, name: &str, commit: &str) -> VcsRef {
    VcsRef::new(Hash::build(commit), name, BRANCH_TYPE, root.clone())
}

impl Fixture {
    /// `dataWaiter.get()`: the next published data pack.
    fn next_data_pack(&self, index: usize) -> Arc<VcsLogGraphData> {
        self.data_packs.lock().unwrap()[index].clone()
    }

    fn published_count(&self) -> usize {
        self.data_packs.lock().unwrap().len()
    }

    /// `initAndWaitForFirstRefresh`.
    fn init_and_wait_for_first_refresh(&mut self) {
        self.loader.initialize();
        assert_eq!(self.published_count(), 2);
        assert!(!self.next_data_pack(0).is_full());
        assert!(self.next_data_pack(1).is_full());
    }

    fn assert_data_pack(&self, expected_log: &[TimedVcsCommit], data_pack: &VcsLogGraphData) {
        let storage: &dyn VcsLogStorage = self.storage.as_ref();
        let get_hash = |index: &i32| storage.get_commit_id(*index).unwrap().hash;
        let actual: Vec<(Hash, Vec<Hash>, i64)> = data_pack
            .permanent_graph()
            .unwrap()
            .all_commits()
            .iter()
            .map(|c| {
                (
                    get_hash(c.id()),
                    c.parents().iter().map(get_hash).collect(),
                    c.timestamp(),
                )
            })
            .collect();
        let expected: Vec<(Hash, Vec<Hash>, i64)> = expected_log
            .iter()
            .map(|c| (c.id().clone(), c.parents().to_vec(), c.timestamp()))
            .collect();
        assert_eq!(actual, expected);
    }
}

#[test]
fn test_initialize_shows_short_history() {
    let mut f = set_up();
    f.loader.initialize();
    let result = f.next_data_pack(0);
    f.assert_data_pack(&log(&f.commits[..RECENT_COMMITS_COUNT]), &result);
    assert!(!result.is_full());
    assert_eq!(f.published_count(), 2);
}

#[test]
fn test_first_refresh_reports_full_history() {
    let mut f = set_up();
    f.loader.initialize();

    let first_data_pack = f.next_data_pack(0);
    f.assert_data_pack(&log(&f.commits[..RECENT_COMMITS_COUNT]), &first_data_pack);

    let full_data_pack = f.next_data_pack(1);
    f.assert_data_pack(&log(&f.commits), &full_data_pack);
    assert_eq!(f.published_count(), 2);
}

#[test]
fn test_refresh_captures_new_commits() {
    let mut f = set_up();
    f.init_and_wait_for_first_refresh();

    let new_commit = "4|-a3|-a2";
    f.log_provider.append_history(log(&[new_commit]));
    f.log_provider.reset_read_first_block_counter();
    let root = f.project_root.clone();
    f.loader.refresh(&[root]);
    let result = f.next_data_pack(2);

    let mut all_commits = vec![new_commit];
    all_commits.extend(f.commits.iter().copied());
    f.assert_data_pack(&log(&all_commits), &result);
    assert_eq!(f.published_count(), 3);
    assert_eq!(f.log_provider.get_read_first_block_counter(), 1);
}

#[test]
fn test_single_refresh_causes_single_data_read() {
    let mut f = set_up();
    f.init_and_wait_for_first_refresh();

    f.log_provider.reset_read_first_block_counter();
    let root = f.project_root.clone();
    f.loader.refresh(&[root]);
    assert_eq!(f.published_count(), 3);
    assert_eq!(
        1,
        f.log_provider.get_read_first_block_counter(),
        "Unexpected first block read count"
    );
}

/// IdeaGit: a refresh with no roots publishes nothing; a refresh for an unknown root gives an
/// error data pack, and the next refresh reloads the full log.
#[test]
fn refresh_edge_cases() {
    let mut f = set_up();
    f.init_and_wait_for_first_refresh();
    f.loader.refresh(&[]);
    assert_eq!(f.published_count(), 2);

    let error = f.loader.refresh(&[vcs_root("/unknown")]);
    assert!(error.load_error().is_some());
    assert!(!error.is_full());
    let root = f.project_root.clone();
    let reloaded = f.loader.refresh(&[root]);
    f.assert_data_pack(&log(&f.commits), &reloaded);
    assert!(reloaded.is_full());
    assert_eq!(f.published_count(), 4);
}
