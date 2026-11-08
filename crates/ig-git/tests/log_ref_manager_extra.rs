//! IdeaGit tests (not ported from upstream).
//!
//! Extra tests for the git ref manager and repository manager: root tiebreak, current branch with
//! master present, comparator antisymmetry, snapshot sorting and repository removal.

mod common;

use std::cmp::Ordering;
use std::sync::Arc;

use common::log_helpers::{init_master_repo, last};
use common::{git, runner};
use ig_git::log::provider::GitLogProvider;
use ig_git::log::ref_manager::{GitRefManager, HEAD, LOCAL_BRANCH, REMOTE_BRANCH, TAG};
use ig_git::log::repository_manager::GitRepositoryManager;
use ig_log::data::{Hash, VcsLogProvider, VcsLogRefManager, VcsRef, VcsRefType, VcsRoot};

fn make_ref(root: &VcsRoot, name: &str, ref_type: VcsRefType) -> VcsRef {
    VcsRef::new(Hash::build(&last(root)), name, ref_type, root.clone())
}

fn names(refs: &[VcsRef]) -> Vec<&str> {
    refs.iter().map(|r| r.name()).collect()
}

fn manager() -> (Arc<GitRepositoryManager>, GitRefManager) {
    let repos = Arc::new(GitRepositoryManager::new(Arc::new(runner())));
    (repos.clone(), GitRefManager::new(repos))
}

#[test]
fn same_name_and_type_in_two_roots_is_ordered_by_root() {
    let (tmp1, tmp2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (root1, root2) = (init_master_repo(tmp1.path()), init_master_repo(tmp2.path()));
    let (repos, refs) = manager();
    repos.update(&root1);
    repos.update(&root2);
    let (a, b) = (
        make_ref(&root1, "dev", LOCAL_BRANCH),
        make_ref(&root2, "dev", LOCAL_BRANCH),
    );
    let expected = ig_log::data::compare_roots(&root1, &root2);
    assert_ne!(expected, Ordering::Equal);
    assert_eq!(refs.labels_order_compare(&a, &b), expected);
    assert_eq!(refs.labels_order_compare(&b, &a), expected.reverse());
    assert_eq!(refs.branch_layout_compare(&a, &b), expected);
    assert_eq!(refs.branch_layout_compare(&b, &a), expected.reverse());
}

/// A repository on `feature` with branches `master`, `a`, `z`, plus HEAD, a remote and a tag.
fn feature_checked_out() -> (
    tempfile::TempDir,
    VcsRoot,
    Arc<GitRepositoryManager>,
    GitRefManager,
    Vec<VcsRef>,
) {
    let tmp = tempfile::tempdir().unwrap();
    let root = init_master_repo(tmp.path());
    for branch in ["feature", "a", "z"] {
        git(&root, &["branch", branch]);
    }
    git(&root, &["update-ref", "refs/remotes/origin/master", "HEAD"]);
    git(&root, &["checkout", "-q", "feature"]);
    let (repos, refs) = manager();
    repos.update(&root);
    let all = vec![
        make_ref(&root, "z", LOCAL_BRANCH),
        make_ref(&root, "origin/master", REMOTE_BRANCH),
        make_ref(&root, "v1", TAG),
        make_ref(&root, "master", LOCAL_BRANCH),
        make_ref(&root, "a", LOCAL_BRANCH),
        make_ref(&root, "HEAD", HEAD),
        make_ref(&root, "feature", LOCAL_BRANCH),
    ];
    (tmp, root, repos, refs, all)
}

#[test]
fn current_branch_and_master_both_outrank_other_local_branches() {
    let (_tmp, _root, _repos, refs, mut all) = feature_checked_out();
    all.sort_by(|a, b| refs.labels_order_compare(a, b));
    // Upstream `GitLabelComparator.orderedTypes`: HEAD, CURRENT_BRANCH, MASTER, ORIGIN_MASTER,
    // LOCAL_BRANCH, REMOTE_BRANCH, TAG, OTHER. So `feature` (current) comes before `master`, which
    // comes before `origin/master`, which outranks plain local branches `a` and `z`; the tag is last.
    assert_eq!(
        names(&all),
        ["HEAD", "feature", "master", "origin/master", "a", "z", "v1"]
    );
}

#[test]
fn comparators_are_antisymmetric_over_a_mixed_set() {
    let (_tmp, _root, _repos, refs, all) = feature_checked_out();
    for a in &all {
        assert_eq!(refs.labels_order_compare(a, a), Ordering::Equal);
        assert_eq!(refs.branch_layout_compare(a, a), Ordering::Equal);
        for b in &all {
            assert_eq!(
                refs.labels_order_compare(a, b),
                refs.labels_order_compare(b, a).reverse(),
                "labels {} vs {}",
                a.name(),
                b.name()
            );
            assert_eq!(
                refs.branch_layout_compare(a, b),
                refs.branch_layout_compare(b, a).reverse(),
                "layout {} vs {}",
                a.name(),
                b.name()
            );
        }
    }
}

#[test]
fn sort_labels_matches_labels_order_compare() {
    let (_tmp, root, _repos, refs, all) = feature_checked_out();
    let other = tempfile::tempdir().unwrap();
    let root2 = init_master_repo(other.path());
    let mut mixed = all;
    mixed.push(make_ref(&root2, "feature", LOCAL_BRANCH));
    mixed.push(make_ref(&root2, "master", LOCAL_BRANCH));
    mixed.push(make_ref(&root, "master", LOCAL_BRANCH));

    let mut expected = mixed.clone();
    expected.sort_by(|a, b| refs.labels_order_compare(a, b));
    let mut actual = mixed;
    refs.sort_labels(&mut actual);
    assert_eq!(actual, expected);
}

#[test]
fn remove_and_retain_roots_drop_cached_state() {
    let (tmp1, tmp2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (root1, root2) = (init_master_repo(tmp1.path()), init_master_repo(tmp2.path()));
    let (repos, _) = manager();
    repos.update(&root1);
    repos.update(&root2);
    assert!(repos.get_repository_for_root_quick(&root1).is_some());

    repos.remove(&root1);
    assert!(repos.get_repository_for_root_quick(&root1).is_none());
    assert!(repos.get_repository_for_root_quick(&root2).is_some());

    repos.update(&root1);
    repos.retain_roots(std::slice::from_ref(&root1));
    assert!(repos.get_repository_for_root_quick(&root1).is_some());
    assert!(repos.get_repository_for_root_quick(&root2).is_none());
}

#[test]
fn sort_labels_through_dyn_trait_matches_git_ref_manager() {
    let (_tmp, root, repos, refs, all) = feature_checked_out();
    let provider = GitLogProvider::with_repository_manager(repos);
    let dyn_refs: Arc<dyn VcsLogRefManager> = provider.reference_manager();
    let mut mixed = all;
    mixed.push(make_ref(&root, "feature", REMOTE_BRANCH));

    let mut expected = mixed.clone();
    refs.sort_labels(&mut expected);
    let mut actual = mixed;
    dyn_refs.sort_labels(&mut actual);
    assert_eq!(actual, expected);
}
