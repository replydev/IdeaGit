//! Ports of `plugins/git4idea/tests/git4idea/log/GitBranchComparatorTest.kt` and
//! `GitLabelComparatorTest.kt`, with the `given`/`expect` helpers of `GitRefManagerTestUtil.kt`.

mod common;

use std::path::Path;

use std::sync::Arc;

use common::log_helpers::{init_master_repo, last, read_all_refs};
use common::{git, runner};
use ig_git::log::ref_manager::{GitRefManager, HEAD, LOCAL_BRANCH, REMOTE_BRANCH, TAG};
use ig_git::log::repository_manager::GitRepositoryManager;
use ig_git::refs::strip_refs_prefix;
use ig_log::data::{Hash, VcsLogRefManager, VcsRef, VcsRoot};

struct Context {
    _tmp: tempfile::TempDir,
    root: VcsRoot,
    repository_manager: Arc<GitRepositoryManager>,
    ref_manager: GitRefManager,
}

impl Context {
    fn new() -> Context {
        let tmp = tempfile::tempdir().unwrap();
        let root = init_master_repo(tmp.path());
        let repository_manager = Arc::new(GitRepositoryManager::new(Arc::new(runner())));
        Context {
            _tmp: tmp,
            root,
            ref_manager: GitRefManager::new(repository_manager.clone()),
            repository_manager,
        }
    }

    fn dir(&self) -> &Path {
        &self.root
    }

    /// `given`: creates the refs at HEAD (`HEAD`, `origin/...` remote branches, `refs/tags/...`
    /// tags, local branches otherwise); local branches with a matching remote become tracking.
    fn given(&self, refs: &[&str]) -> Vec<VcsRef> {
        let dir = self.dir();
        let hash = Hash::build(&last(dir));
        let mut result = Vec::new();
        for ref_name in refs {
            if *ref_name == "HEAD" {
                result.push(self.vcs_ref(&hash, "HEAD", HEAD));
            } else if ref_name.starts_with("origin/") {
                git(
                    dir,
                    &[
                        "update-ref",
                        &format!("refs/remotes/{ref_name}"),
                        &hash.as_string(),
                    ],
                );
                result.push(self.vcs_ref(&hash, ref_name, REMOTE_BRANCH));
            } else if ref_name.starts_with("refs/tags/") {
                git(dir, &["update-ref", ref_name, &hash.as_string()]);
                result.push(self.vcs_ref(&hash, strip_refs_prefix(ref_name), TAG));
            } else {
                git(
                    dir,
                    &[
                        "update-ref",
                        &format!("refs/heads/{ref_name}"),
                        &hash.as_string(),
                    ],
                );
                result.push(self.vcs_ref(&hash, ref_name, LOCAL_BRANCH));
            }
        }
        self.set_up_tracking(&result);
        self.repository_manager.update(&self.root);
        result
    }

    fn vcs_ref(&self, hash: &Hash, name: &str, ref_type: ig_log::data::VcsRefType) -> VcsRef {
        VcsRef::new(hash.clone(), name, ref_type, self.root.clone())
    }

    /// The `git config` calls are inert for these comparators (neither consults tracking info);
    /// they are kept for fidelity with the upstream test.
    fn set_up_tracking(&self, refs: &[VcsRef]) {
        for r in refs.iter().filter(|r| r.ref_type() == LOCAL_BRANCH) {
            let local_branch = r.name();
            let has_matching_remote = refs.iter().any(|it| {
                it.ref_type() == REMOTE_BRANCH && it.name().replace("origin/", "") == local_branch
            });
            if has_matching_remote {
                git(
                    self.dir(),
                    &["config", &format!("branch.{local_branch}.remote"), "origin"],
                );
                git(
                    self.dir(),
                    &[
                        "config",
                        &format!("branch.{local_branch}.merge"),
                        &format!("refs/heads/{local_branch}"),
                    ],
                );
            }
        }
    }

    /// `expect`: the refs actually in the repository, in the order of `ref_names`.
    fn expect(&self, ref_names: &[&str]) -> Vec<VcsRef> {
        let refs = read_all_refs(self.dir(), &self.root);
        ref_names
            .iter()
            .map(|ref_name| {
                refs.iter()
                    .find(|it| it.name() == strip_refs_prefix(ref_name))
                    .unwrap_or_else(|| panic!("Ref {ref_name} not found among {refs:?}"))
                    .clone()
            })
            .collect()
    }

    // GitBranchComparatorTest

    fn get_the_most_powerful_ref(&self, given_branches: &[VcsRef]) -> VcsRef {
        let mut sorted = given_branches.to_vec();
        sorted.sort_by(|a, b| self.ref_manager.branch_layout_compare(a, b));
        sorted[0].clone()
    }

    fn check_best(&self, expected_best: &str, given_branches: &[VcsRef]) {
        assert_eq!(
            self.get_the_most_powerful_ref(given_branches),
            self.expect(&[expected_best])[0]
        );
    }

    // GitLabelComparatorTest

    fn sort(&self, refs: &[VcsRef]) -> Vec<VcsRef> {
        let mut sorted = refs.to_vec();
        sorted.sort_by(|a, b| self.ref_manager.labels_order_compare(a, b));
        sorted
    }

    /// Only the names are compared, as upstream.
    fn check_labels(&self, unsorted: &[VcsRef], expected: &[VcsRef]) {
        let sorted = self.sort(unsorted);
        let actual: Vec<&str> = sorted.iter().map(|r| r.name()).collect();
        let expected: Vec<&str> = expected.iter().map(|r| r.name()).collect();
        assert_eq!(actual, expected, "Sorted refs don't match");
    }
}

mod branch_comparator {
    use super::Context;

    #[test]
    fn tracked_remote_branch_is_more_important_than_its_local_branch() {
        let c = Context::new();
        c.check_best("origin/139", &c.given(&["origin/139", "139"]));
    }

    #[test]
    fn tracked_remote_branch_is_more_important_than_any_local_branch() {
        let c = Context::new();
        c.check_best("origin/139", &c.given(&["feature", "origin/139", "139"]));
    }

    #[test]
    fn untracked_remote_branch_is_more_important_than_any_local_branch() {
        let c = Context::new();
        c.check_best("origin/139", &c.given(&["feature", "origin/139"]));
    }

    #[test]
    fn master_is_more_important_than_other_local_branches() {
        let c = Context::new();
        c.check_best("master", &c.given(&["feature", "master"]));
    }

    #[test]
    fn origin_master_is_more_important_than_other_remote_branches() {
        let c = Context::new();
        c.check_best(
            "origin/master",
            &c.given(&["origin/139", "master", "139", "origin/master", "origin/135"]),
        );
    }

    #[test]
    fn local_branch_is_more_important_than_tag() {
        let c = Context::new();
        c.check_best(
            "feature",
            &c.given(&["refs/tags/v1", "feature", "refs/tags/v2"]),
        );
    }

    #[test]
    fn two_local_non_tracking_branches_are_compared_lexicographically() {
        let c = Context::new();
        c.check_best("feature", &c.given(&["zoo", "feature"]));
    }

    #[test]
    fn tracked_local_branch_is_not_special() {
        let c = Context::new();
        let refs: Vec<_> = c
            .given(&["zoo", "origin/zoo", "feature"])
            .into_iter()
            .filter(|it| it.name() != "origin/zoo")
            .collect();
        assert_eq!(c.get_the_most_powerful_ref(&refs).name(), "feature");
    }

    #[test]
    fn remote_branches_are_compared_lexicographically() {
        let c = Context::new();
        c.check_best(
            "origin/feature",
            &c.given(&["origin/feature", "origin/zoo", "zoo"]),
        );
    }

    #[test]
    fn local_branch_is_more_important_than_head() {
        let c = Context::new();
        c.check_best("feature", &c.given(&["feature", "HEAD"]));
    }

    #[test]
    fn tag_is_more_important_than_head() {
        let c = Context::new();
        c.check_best("refs/tags/v1", &c.given(&["refs/tags/v1", "HEAD"]));
    }
}

mod label_comparator {
    use super::Context;
    use crate::common::git;

    #[test]
    fn empty() {
        let c = Context::new();
        c.check_labels(&[], &[]);
    }

    #[test]
    fn single() {
        let c = Context::new();
        c.check_labels(&c.given(&["HEAD"]), &c.expect(&["HEAD"]));
    }

    #[test]
    fn head_is_at_left_from_branch() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["master", "HEAD"]),
            &c.expect(&["HEAD", "master"]),
        );
    }

    #[test]
    fn local_branches_are_compared_as_strings() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["release", "feature"]),
            &c.expect(&["feature", "release"]),
        );
    }

    #[test]
    fn tag_is_to_the_right_of_remote_branch() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["refs/tags/v1", "origin/master"]),
            &c.expect(&["origin/master", "refs/tags/v1"]),
        );
    }

    #[test]
    fn master_is_to_the_left_of_other_local_branches() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["feature", "master"]),
            &c.expect(&["master", "feature"]),
        );
    }

    #[test]
    fn origin_master_is_to_the_left_of_other_remote_branches() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["origin/master", "origin/aaa"]),
            &c.expect(&["origin/master", "origin/aaa"]),
        );
    }

    #[test]
    fn remote_tracking_branch_is_not_special() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["feature", "origin/aaa", "origin/feature"]),
            &c.expect(&["feature", "origin/aaa", "origin/feature"]),
        );
    }

    #[test]
    fn complex_1() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["refs/tags/v1", "feature", "HEAD", "master"]),
            &c.expect(&["HEAD", "master", "feature", "refs/tags/v1"]),
        );
    }

    #[test]
    fn complex_2() {
        let c = Context::new();
        c.check_labels(
            &c.given(&[
                "origin/master",
                "origin/great_feature",
                "refs/tags/v1",
                "release",
                "HEAD",
                "master",
            ]),
            &c.expect(&[
                "HEAD",
                "master",
                "origin/master",
                "release",
                "origin/great_feature",
                "refs/tags/v1",
            ]),
        );
    }

    /// May happen e.g. in the multi-repo case.
    #[test]
    fn two_masters() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["master", "master"]),
            &c.expect(&["master", "master"]),
        );
    }

    #[test]
    fn current_branch_first() {
        let c = Context::new();
        git(c.dir(), &["checkout", "-q", "-b", "zzz"]);
        c.check_labels(
            &c.given(&["xxx", "zzz", "yyy"]),
            &c.expect(&["zzz", "xxx", "yyy"]),
        );
    }
}
