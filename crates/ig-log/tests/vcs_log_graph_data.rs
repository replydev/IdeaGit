//! IdeaGit tests for building the data pack: storage-index commit ids, branch head order
//! (`HeadCommitsComparator`), branch colors (`GraphColorManagerImpl`), the commit order check of
//! the refresher, and the node-id index of `PermanentCommitsInfoImpl`.

mod common;

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use common::test_vcs_log_provider::{ByName, MockRefManager, TestVcsLogProvider};
use common::timed_commit_parser::log;
use ig_log::api::GraphLayout;
use ig_log::api::permanent::PermanentCommitsInfo;
use ig_log::data::graph_color_manager_impl::{DEFAULT_COLOR, java_string_hash_code};
use ig_log::data::graph_data::build_data;
use ig_log::data::refresher::{check_commits, compact_commit};
use ig_log::data::{
    Hash, HeadCommitsComparator, InMemoryStorage, LogProviders, RootRefsModel, VcsLogProvider,
    VcsLogRefManager, VcsLogRefresher, VcsLogStorage, VcsRef, VcsRefType, vcs_root,
};
use ig_log::graph_color_manager::GraphColorManager;
use ig_log::permanent::permanent_commits_info_impl::PermanentCommitsInfoImpl;
use ig_log::{GraphCommitImpl, PermanentGraph};

const BRANCH: VcsRefType = VcsRefType::new("BRANCH", true);
const TAG: VcsRefType = VcsRefType::new("TAG", false);

#[test]
fn heads_are_ordered_by_their_best_ref_and_colored_by_its_name() {
    let root = vcs_root("/r");
    let storage: Arc<dyn VcsLogStorage> = Arc::new(InMemoryStorage::new());
    // two branches from a1: b2 (refs "zoo" and "alpha") and c2 (ref "main"), d2 without refs
    let commits = log(&["5|-b2|-a1", "4|-c2|-a1", "3|-d2|-a1", "1|-a1|-"]);
    let graph_commits: Vec<_> = commits
        .iter()
        .map(|c| compact_commit(storage.as_ref(), c, &root))
        .collect();
    let r = |hash: &str, name: &str, t: VcsRefType| {
        VcsRef::new(Hash::build(hash), name, t, root.clone())
    };
    let refs = RootRefsModel::create(
        [
            r("b2", "zoo", BRANCH),
            r("b2", "alpha", BRANCH),
            r("c2", "main", BRANCH),
            r("a1", "v1", TAG),
        ],
        storage.as_ref(),
    );
    // only the reference manager of the provider is used
    let provider = Arc::new(TestVcsLogProvider::default());
    let mut providers = LogProviders::new();
    providers.insert(root.clone(), provider as Arc<dyn VcsLogProvider>);
    let mut pack = build_data(
        &graph_commits,
        BTreeMap::from([(root.clone(), Arc::new(refs))]),
        &providers,
        &storage,
        true,
    );
    assert_eq!(pack.commits_count(), 4);

    // the test provider's manager compares all refs equal: use a by-name manager instead
    let refs_model = pack.refs_model().clone();
    let managers = BTreeMap::from([(root.clone(), Arc::new(ByName) as Arc<dyn VcsLogRefManager>)]);
    let refs_model = Arc::new(ig_log::data::RefsModel::create(
        refs_model.refs_by_root().clone(),
        storage.clone(),
        managers.clone(),
    ));
    let index = |h: &str| storage.get_commit_index(&Hash::build(h), &root);
    assert_eq!(
        refs_model
            .get_ref_for_head_commit(index("b2"))
            .unwrap()
            .name(),
        "alpha"
    );
    assert_eq!(refs_model.refs_to_commit(index("a1")).len(), 1);
    assert!(refs_model.get_ref_for_head_commit(index("d2")).is_none());

    let comparator = HeadCommitsComparator::new(refs_model.clone(), managers);
    assert_eq!(
        comparator.compare(&index("b2"), &index("c2")),
        Ordering::Less,
        "alpha < main"
    );
    assert_eq!(
        comparator.compare(&index("d2"), &index("c2")),
        Ordering::Greater,
        "no refs last"
    );
    assert_eq!(
        comparator.compare(&index("c2"), &index("c2")),
        Ordering::Equal
    );

    let colors = ig_log::data::GraphColorManagerImpl::new(refs_model);
    assert_eq!(
        colors.get_color(&index("b2"), 1, 1),
        java_string_hash_code("alpha")
    );
    assert_eq!(colors.get_color(&index("b2"), 1, 7), 7);
    assert_eq!(colors.get_color(&index("d2"), 2, 2), DEFAULT_COLOR);

    // branch heads are the branch refs' commits (tags excluded)
    let graph = pack.permanent_graph().unwrap();
    let branches = graph.get_containing_branches(&index("a1"));
    assert_eq!(branches, HashSet::from([index("b2"), index("c2")]));
    pack = build_data(&[], BTreeMap::new(), &providers, &storage, false);
    assert!(pack.permanent_graph().is_none() && !pack.is_full());
}

#[test]
fn commit_order_check() {
    let c = |id: i32, parents: &[i32]| GraphCommitImpl::create_commit(id, parents.to_vec(), 0);
    assert!(check_commits(&[c(3, &[2]), c(2, &[1]), c(1, &[])]).is_ok());
    assert!(
        check_commits(&[c(3, &[9]), c(2, &[])]).is_ok(),
        "parents outside the list are not loaded"
    );
    let parent_first = check_commits(&[c(1, &[]), c(2, &[1])]).unwrap_err();
    assert!(parent_first.contains("topological"), "{parent_first}");
    let duplicate = check_commits(&[c(2, &[1]), c(2, &[1]), c(1, &[])]).unwrap_err();
    assert!(duplicate.contains("Duplicate"), "{duplicate}");
}

/// A provider returning a parent before its child gives an error data pack, not a wrong graph.
#[test]
fn refresher_rejects_non_topological_full_log() {
    let root = vcs_root("/r");
    let provider = Arc::new(TestVcsLogProvider::default());
    provider.append_history(log(&["1|-a|-", "2|-b|-a"]));
    let mut providers = LogProviders::new();
    providers.insert(root.clone(), provider as Arc<dyn VcsLogProvider>);
    let mut refresher = VcsLogRefresher::new(Arc::new(InMemoryStorage::new()), providers, 1);
    let pack = refresher.initialize();
    let error = pack.load_error().expect("error data pack");
    assert!(error.0.contains("topological"), "{error}");
}

#[test]
fn node_ids_are_indexed_including_duplicates_and_not_loaded_commits() {
    let commits = vec![
        GraphCommitImpl::create_commit("c".to_string(), vec!["b".to_string()], 3),
        GraphCommitImpl::create_commit("b".to_string(), vec!["a".to_string()], 2),
        GraphCommitImpl::create_commit("c".to_string(), vec![], 1),
    ];
    let info = PermanentCommitsInfoImpl::new_instance(
        &commits,
        [(-2, "a".to_string())].into_iter().collect(),
    );
    assert_eq!(
        info.get_node_id(&"c".to_string()),
        0,
        "the first row of a duplicate id"
    );
    assert_eq!(info.get_node_id(&"b".to_string()), 1);
    assert_eq!(info.get_node_id(&"a".to_string()), -2);
    assert_eq!(info.get_node_id(&"x".to_string()), -1);
    let ids: HashSet<String> = ["c", "a", "x"].map(String::from).into_iter().collect();
    assert_eq!(info.convert_to_node_ids(&ids), HashSet::from([0, 2, -2]));
    assert_eq!(
        info.convert_to_node_ids_skipping(&ids, true),
        HashSet::from([0, 2])
    );
    assert!(info.contains_all(&["c".to_string(), "b".to_string()]));
    assert!(
        !info.contains_all(&["a".to_string()]),
        "not-loaded commits are not contained"
    );
}

/// The refresher builds the pack with the providers' ref managers: `ByName` decides the head
/// order of the permanent graph, the ref chosen for a head and so the branch colors.
#[test]
fn refresher_wires_ref_managers_into_the_data_pack() {
    let root = vcs_root("/r");
    let provider = Arc::new(TestVcsLogProvider::with_ref_manager(Arc::new(ByName)));
    // heads b2 (row 0, refs "zoo" and "yak") and c2 (row 1, ref "alpha")
    provider.append_history(log(&["5|-b2|-a1", "4|-c2|-a1", "1|-a1|-"]));
    for (hash, name) in [("b2", "zoo"), ("b2", "yak"), ("c2", "alpha")] {
        provider.add_ref(VcsRef::new(Hash::build(hash), name, BRANCH, root.clone()));
    }
    let storage: Arc<dyn VcsLogStorage> = Arc::new(InMemoryStorage::new());
    let mut providers = LogProviders::new();
    providers.insert(root.clone(), provider as Arc<dyn VcsLogProvider>);
    let mut refresher = VcsLogRefresher::new(storage.clone(), providers, 10);
    let pack = refresher.initialize();

    let index = |h: &str| storage.get_commit_index(&Hash::build(h), &root);
    let refs_model = pack.refs_model();
    assert_eq!(
        refs_model
            .get_ref_for_head_commit(index("b2"))
            .unwrap()
            .name(),
        "yak"
    );
    assert_eq!(
        refs_model
            .get_ref_for_head_commit(index("c2"))
            .unwrap()
            .name(),
        "alpha"
    );
    let colors = ig_log::data::GraphColorManagerImpl::new(refs_model.clone());
    assert_eq!(
        colors.get_color(&index("b2"), 1, 1),
        java_string_hash_code("yak")
    );

    // head order in the permanent graph: "alpha" (c2, row 1) before "yak" (b2, row 0)
    let graph = pack.permanent_graph().unwrap();
    let info = graph.permanent_commits_info_impl();
    let heads: Vec<_> = graph
        .permanent_graph_layout_impl()
        .get_head_node_index()
        .iter()
        .map(|&node| info.get_commit_id(node))
        .collect();
    assert_eq!(heads, [index("c2"), index("b2")]);
}

#[test]
fn heads_without_refs_are_ordered_by_index() {
    let root = vcs_root("/r");
    let storage: Arc<dyn VcsLogStorage> = Arc::new(InMemoryStorage::new());
    let index = |h: &str| storage.get_commit_index(&Hash::build(h), &root);
    let (x, y) = (index("1"), index("2"));
    let refs = BTreeMap::from([(
        root.clone(),
        Arc::new(RootRefsModel::create([], storage.as_ref())),
    )]);
    let managers = BTreeMap::from([(root, Arc::new(MockRefManager) as Arc<dyn VcsLogRefManager>)]);
    let model = Arc::new(ig_log::data::RefsModel::create(
        refs,
        storage.clone(),
        managers.clone(),
    ));
    let comparator = HeadCommitsComparator::new(model, managers);
    assert_eq!(comparator.compare(&x, &y), x.cmp(&y));
    assert_eq!(comparator.compare(&y, &x), y.cmp(&x));
    assert_eq!(comparator.compare(&x, &x), Ordering::Equal);
}

/// Refs of different managers are ordered by the managers' `name()`, whatever the indexes.
#[test]
fn refs_from_different_managers_are_ordered_by_manager_name() {
    let r1 = vcs_root("/r1");
    let r2 = vcs_root("/r2");
    let storage: Arc<dyn VcsLogStorage> = Arc::new(InMemoryStorage::new());
    let (h1, h2) = (
        storage.get_commit_index(&Hash::build("a"), &r1),
        storage.get_commit_index(&Hash::build("b"), &r2),
    );
    let model_of = |hash: &str, root: &ig_log::data::VcsRoot| {
        Arc::new(RootRefsModel::create(
            [VcsRef::new(Hash::build(hash), "m", BRANCH, root.clone())],
            storage.as_ref(),
        ))
    };
    let refs = BTreeMap::from([
        (r1.clone(), model_of("a", &r1)),
        (r2.clone(), model_of("b", &r2)),
    ]);
    // "ByName" < "MockRefManager"
    let managers = |first: Arc<dyn VcsLogRefManager>, second: Arc<dyn VcsLogRefManager>| {
        BTreeMap::from([(r1.clone(), first), (r2.clone(), second)])
    };
    for (m, expected) in [
        (
            managers(Arc::new(ByName), Arc::new(MockRefManager)),
            Ordering::Less,
        ),
        (
            managers(Arc::new(MockRefManager), Arc::new(ByName)),
            Ordering::Greater,
        ),
    ] {
        let model = Arc::new(ig_log::data::RefsModel::create(
            refs.clone(),
            storage.clone(),
            m.clone(),
        ));
        let comparator = HeadCommitsComparator::new(model, m);
        assert_eq!(comparator.compare(&h1, &h2), expected);
        assert_eq!(comparator.compare(&h2, &h1), expected.reverse());
    }
}
