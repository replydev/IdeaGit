//! IdeaGit tests (not ported from upstream) for the git log provider: regressions found in review.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use common::log_helpers::{provider, tac};
use common::{SCRUBBED_ENV, git, init_repo};
use ig_git::console::NullConsole;
use ig_git::log::provider::GitLogProvider;
use ig_git::runner::Git;
use ig_log::data::{
    InMemoryStorage, LogProviders, VcsLogRefresher, VcsRefsContainer, VcsRoot, vcs_root,
};

fn refresher(provider: GitLogProvider, root: &VcsRoot) -> VcsLogRefresher {
    let mut providers: LogProviders = BTreeMap::new();
    providers.insert(root.clone(), Arc::new(provider));
    VcsLogRefresher::new(Arc::new(InMemoryStorage::new()), providers, 1000)
}

/// A root that had no commits when first seen shows its commits once they exist.
#[test]
fn fresh_repository_shows_first_commit_after_refresh() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "master"]);
    let root = vcs_root(dir);
    let mut refresher = refresher(provider(), &root);
    let data = refresher.initialize();
    assert_eq!(data.commits_count(), 0);

    tac(dir, "a.txt");
    let data = refresher.refresh(std::slice::from_ref(&root));
    assert!(data.load_error().is_none(), "{:?}", data.load_error());
    assert_eq!(data.commits_count(), 1);
}

/// `column.ui=always` makes `git tag -l` print several tags per line; refresh must not break.
#[test]
fn refresh_with_column_ui_always_lists_tags() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("repo");
    init_repo(&dir);
    let root = vcs_root(&dir);
    let config = tmp.path().join("column.gitconfig");
    std::fs::write(&config, "[column]\n\tui = always\n").unwrap();
    let runner = Git::detect("git", Arc::new(NullConsole))
        .unwrap()
        .with_base_environment(
            &[
                ("GIT_CONFIG_GLOBAL", config.to_str().unwrap()),
                ("GIT_CONFIG_NOSYSTEM", "1"),
                ("GIT_AUTHOR_NAME", "Test"),
                ("GIT_AUTHOR_EMAIL", "test@example.com"),
                ("GIT_COMMITTER_NAME", "Test"),
                ("GIT_COMMITTER_EMAIL", "test@example.com"),
            ],
            SCRUBBED_ENV,
        );
    let mut refresher = refresher(GitLogProvider::new(Arc::new(runner)), &root);
    refresher.initialize();

    for tag in ["v1", "v2", "v3", "v4"] {
        git(&dir, &["tag", tag]);
    }
    tac(&dir, "b.txt");
    let data = refresher.refresh(std::slice::from_ref(&root));
    assert!(data.load_error().is_none(), "{:?}", data.load_error());
    let tags: usize = data
        .refs_model()
        .refs_by_root()
        .values()
        .map(|m| m.tags().count())
        .sum();
    assert_eq!(tags, 4);
}
