//! IdeaGit tests for the refresher (not ported from upstream).

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

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
