//! IdeaGit tests for the refresher (not ported from upstream).

mod common;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use common::test_vcs_log_provider::{BRANCH_TYPE, TestVcsLogProvider};
use common::timed_commit_parser::log;
use ig_log::data::{
    Hash, InMemoryStorage, LogProviders, VcsLogGraphData, VcsLogRefresher, VcsLogStorage, VcsRef,
    VcsRoot, vcs_root,
};
use ig_log::{GraphCommit, PermanentGraph};

fn branch(hash: &str, name: &str, root: &VcsRoot) -> VcsRef {
    VcsRef::new(Hash::build(hash), name, BRANCH_TYPE, root.clone())
}

fn new_refresher(
    provider: &Arc<TestVcsLogProvider>,
    root: &VcsRoot,
    recent_commit_count: usize,
) -> VcsLogRefresher {
    let storage: Arc<dyn VcsLogStorage> = Arc::new(InMemoryStorage::new());
    let mut providers: LogProviders = BTreeMap::new();
    providers.insert(root.clone(), provider.clone());
    VcsLogRefresher::new(storage, providers, recent_commit_count)
}

/// `id|-parents` of every commit of the pack, in row order.
fn dump(pack: &VcsLogGraphData, storage: &dyn VcsLogStorage) -> Vec<String> {
    let name = |index| storage.get_commit_id(index).unwrap().hash.as_string();
    pack.permanent_graph()
        .expect("full data pack")
        .all_commits()
        .iter()
        .map(|c| {
            let parents: Vec<String> = c.parents().iter().map(|&p| name(p)).collect();
            format!("{}|-{}", name(*c.id()), parents.join(" "))
        })
        .collect()
}

/// The joiner alone turns this refresh into a log with `d` twice (once from the first block, once
/// from the saved log below it); `check_commits` must make the refresher reload everything.
#[test]
fn refresh_reloads_when_joined_log_has_duplicate_ids() {
    let root = vcs_root("/repo");
    let provider = Arc::new(TestVcsLogProvider::default());
    provider.set_history(log(&["20|-c|-", "10|-d|-e", "3|-e|-"]));
    provider.add_ref(branch("c", "master", &root));
    provider.add_ref(branch("d", "feature", &root));

    let mut refresher = new_refresher(&provider, &root, 2);
    let initial = refresher.initialize();
    assert_eq!(
        dump(&initial, refresher.storage().as_ref()),
        ["c|-", "d|-e", "e|-"]
    );

    // `d` gets a new parent `f`, which sits on `c`; the first block is `[d, f]`
    provider.set_history(log(&["10|-d|-f", "9|-f|-c", "20|-c|-", "3|-e|-"]));
    let published = refresher.refresh(std::slice::from_ref(&root));
    let actual = dump(&published, refresher.storage().as_ref());

    let mut unique = actual.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        actual.len(),
        "duplicate commits in {actual:?}"
    );

    let mut fresh_refresher = new_refresher(&provider, &root, 2);
    let fresh = fresh_refresher.initialize();
    assert_eq!(actual, dump(&fresh, fresh_refresher.storage().as_ref()));
}

/// Packs published by a refresher, in order.
fn collect_packs(refresher: &mut VcsLogRefresher) -> Arc<Mutex<Vec<Arc<VcsLogGraphData>>>> {
    let packs = Arc::new(Mutex::new(Vec::new()));
    let sink = packs.clone();
    refresher.set_data_pack_handler(move |pack| sink.lock().unwrap().push(pack.clone()));
    packs
}

fn fulls(packs: &Mutex<Vec<Arc<VcsLogGraphData>>>) -> Vec<bool> {
    packs.lock().unwrap().iter().map(|p| p.is_full()).collect()
}

fn history_provider() -> (VcsRoot, Arc<TestVcsLogProvider>) {
    let root = vcs_root("/repo");
    let provider = Arc::new(TestVcsLogProvider::default());
    provider.append_history(log(&["3|-c|-b", "2|-b|-a", "1|-a|-"]));
    provider.add_ref(branch("c", "master", &root));
    (root, provider)
}

/// Port of the applicable part of upstream `test_reinitialize_makes_refresh_cancelled`: initializing
/// again does not publish a first-block pack over the full one.
#[test]
fn second_initialize_publishes_nothing() {
    let (root, provider) = history_provider();
    let mut refresher = new_refresher(&provider, &root, 2);
    let packs = collect_packs(&mut refresher);
    let full = refresher.initialize();
    assert_eq!(fulls(&packs), [false, true]);

    refresher.refresh(std::slice::from_ref(&root));
    assert_eq!(packs.lock().unwrap().len(), 3);

    let again = refresher.initialize();
    assert_eq!(
        packs.lock().unwrap().len(),
        3,
        "no pack from the 2nd initialize"
    );
    assert!(again.is_full());
    assert!(Arc::ptr_eq(&again, refresher.current_data_pack()));
    assert_eq!(
        dump(&again, refresher.storage().as_ref()),
        dump(&full, refresher.storage().as_ref())
    );
}

/// `refresh` before `initialize` initializes: first block, full log, nothing more.
#[test]
fn refresh_before_initialize_initializes() {
    let (root, provider) = history_provider();
    let mut refresher = new_refresher(&provider, &root, 2);
    let packs = collect_packs(&mut refresher);
    let result = refresher.refresh(std::slice::from_ref(&root));
    assert!(result.is_full());
    assert_eq!(fulls(&packs), [false, true]);
    assert_eq!(
        dump(&result, refresher.storage().as_ref()),
        ["c|-b", "b|-a", "a|-"]
    );
    refresher.initialize();
    assert_eq!(packs.lock().unwrap().len(), 2);
}

/// The first block (1 commit) of the refresh doesn't reach the log; reading 5x more does.
#[test]
fn retry_with_five_times_more_commits_succeeds() {
    let (root, provider) = history_provider();
    let mut refresher = new_refresher(&provider, &root, 1);
    let packs = collect_packs(&mut refresher);
    refresher.initialize();

    provider.append_history(log(&["5|-e|-d", "4|-d|-c"]));
    provider.reset_read_first_block_counter();
    let all_hashes_before = provider.get_read_all_hashes_counter();
    let published = refresher.refresh(std::slice::from_ref(&root));

    assert_eq!(provider.get_read_first_block_counter(), 2);
    assert_eq!(
        provider.get_read_all_hashes_counter(),
        all_hashes_before,
        "no full reload"
    );
    assert!(published.is_full());
    assert_eq!(packs.lock().unwrap().len(), 3);

    let mut fresh_refresher = new_refresher(&provider, &root, 1);
    let fresh = fresh_refresher.initialize();
    assert_eq!(
        dump(&published, refresher.storage().as_ref()),
        dump(&fresh, fresh_refresher.storage().as_ref())
    );
    assert_eq!(
        dump(&published, refresher.storage().as_ref()),
        ["e|-d", "d|-c", "c|-b", "b|-a", "a|-"]
    );
}

/// Upstream does `rootsToRefresh.toSet()`.
#[test]
fn duplicate_roots_are_read_once() {
    let (root, provider) = history_provider();
    let mut refresher = new_refresher(&provider, &root, 2);
    refresher.initialize();
    provider.reset_read_first_block_counter();
    refresher.refresh(&[root.clone(), root.clone()]);
    assert_eq!(provider.get_read_first_block_counter(), 1);
}

/// An `Err` while loading the first block gives an error pack, then the initial refresh loads the
/// full log (upstream: `VcsLogGraphData.Error`, then the queued request reloads everything).
#[test]
fn initialize_survives_first_block_error() {
    let (root, provider) = history_provider();
    let mut refresher = new_refresher(&provider, &root, 2);
    let packs = collect_packs(&mut refresher);
    provider.fail_next_read_first_block();
    let result = refresher.initialize();
    {
        let packs = packs.lock().unwrap();
        assert_eq!(packs.len(), 2);
        assert!(packs[0].load_error().is_some() && !packs[0].is_full());
        assert!(packs[1].load_error().is_none() && packs[1].is_full());
    }
    assert_eq!(
        dump(&result, refresher.storage().as_ref()),
        ["c|-b", "b|-a", "a|-"]
    );
}

/// A provider error during a refresh gives an error pack; the next refresh reloads the full log.
#[test]
fn refresh_error_gives_error_pack_without_full_reload() {
    let (root, provider) = history_provider();
    let mut refresher = new_refresher(&provider, &root, 2);
    let packs = collect_packs(&mut refresher);
    refresher.initialize();
    let reads = provider.get_read_all_hashes_counter();

    provider.fail_next_read_first_block();
    let error = refresher.refresh(std::slice::from_ref(&root));
    assert!(error.load_error().is_some() && !error.is_full());
    assert_eq!(
        provider.get_read_all_hashes_counter(),
        reads,
        "no full reload"
    );
    assert_eq!(packs.lock().unwrap().len(), 3);

    let recovered = refresher.refresh(std::slice::from_ref(&root));
    assert!(recovered.is_full() && recovered.load_error().is_none());
    assert_eq!(provider.get_read_all_hashes_counter(), reads + 1);
}

#[test]
fn recent_details_consumer_receives_first_block_on_initialize() {
    let (root, provider) = history_provider();
    let mut refresher = new_refresher(&provider, &root, 2);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    refresher.set_recent_details_consumer(move |root, details| {
        let ids: Vec<String> = details.iter().map(|d| d.id().as_string()).collect();
        sink.lock().unwrap().push((root.clone(), ids));
    });
    refresher.initialize();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0], (root, vec!["c".to_string(), "b".to_string()]));
}
