//! IdeaGit tests (not ported from upstream) for the git log provider: regressions found in review.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use common::log_helpers::{default_user, init_master_repo, last, provider, tac};
use common::{SCRUBBED_ENV, git, init_repo, runner};
use ig_git::console::NullConsole;
use ig_git::log::provider::GitLogProvider;
use ig_git::log::ref_manager::{HEAD, REMOTE_BRANCH, TAG};
use ig_git::log::util::collect_metadata_with_params;
use ig_git::runner::Git;
use ig_log::GraphCommit;
use ig_log::data::{
    Hash, InMemoryStorage, LogProviders, Requirements, VcsCommitMetadata, VcsLogProvider,
    VcsLogRefresher, VcsRefsContainer, VcsRefsList, VcsRoot, vcs_root,
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
    git(dir, &["init", "-q"]);
    git(dir, &["symbolic-ref", "HEAD", "refs/heads/master"]);
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

fn refresh_block(provider: &GitLogProvider, root: &VcsRoot) -> ig_log::data::DetailedLogData {
    let empty = VcsRefsList::default();
    provider
        .read_first_block(root, &Requirements::refresh(1000, true, &empty))
        .unwrap()
}

/// Remote branches are read from the repository state on the refresh path; the symbolic
/// `origin/HEAD` is not a branch.
#[test]
fn refresh_reports_remote_branches_but_not_origin_head() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    let head = last(dir);
    git(
        dir,
        &["remote", "add", "origin", "https://example.com/repo.git"],
    );
    git(dir, &["update-ref", "refs/remotes/origin/master", "HEAD"]);
    git(
        dir,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/master",
        ],
    );

    let data = refresh_block(&provider(), &root);
    let origin_master: Vec<_> = data
        .refs
        .iter()
        .filter(|r| r.name() == "origin/master")
        .collect();
    assert_eq!(origin_master.len(), 1, "{:?}", data.refs);
    assert_eq!(origin_master[0].ref_type(), REMOTE_BRANCH);
    assert_eq!(origin_master[0].commit_hash().as_string(), head);
    assert!(
        data.refs.iter().all(|r| r.name() != "origin/HEAD"),
        "{:?}",
        data.refs
    );
}

/// A detached HEAD is the HEAD ref at the detached commit, and the current branch is `HEAD`.
#[test]
fn detached_head_ref_and_current_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    tac(dir, "b.txt");
    git(dir, &["checkout", "-q", "--detach", "HEAD~1"]);
    let detached = last(dir);

    let provider = provider();
    let data = refresh_block(&provider, &root);
    let head_ref = data
        .refs
        .iter()
        .find(|r| r.ref_type() == HEAD)
        .expect("HEAD ref");
    assert_eq!(head_ref.commit_hash().as_string(), detached);
    assert_eq!(provider.get_current_branch(&root).as_deref(), Some("HEAD"));
}

/// Tags on commits that no branch reaches are found on refresh (annotated ones peeled to the
/// commit), and the commits their histories share are loaded once.
#[test]
fn refresh_loads_tags_of_unloaded_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    git(dir, &["checkout", "-q", "--detach"]);
    let first = tac(dir, "x.txt");
    git(dir, &["tag", "light", "HEAD"]);
    let second = tac(dir, "y.txt");
    git(dir, &["tag", "-a", "-m", "annotated", "annotated", "HEAD"]);
    git(dir, &["checkout", "-q", "master"]);

    let data = refresh_block(&provider(), &root);
    let tag_hash = |name: &str| {
        let tags: Vec<_> = data
            .refs
            .iter()
            .filter(|r| r.name() == name && r.ref_type() == TAG)
            .collect();
        assert_eq!(tags.len(), 1, "{name}: {:?}", data.refs);
        tags[0].commit_hash().as_string()
    };
    assert_eq!(tag_hash("light"), first);
    assert_eq!(tag_hash("annotated"), second);

    let ids: Vec<Hash> = data.commits.iter().map(|c| c.id().clone()).collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len(), "duplicate commits: {ids:?}");
    assert_eq!(ids.len(), 3, "initial, first and second: {ids:?}");
    for h in [&first, &second] {
        assert!(ids.contains(&Hash::build(h)));
    }
}

/// `read_metadata` is all-or-nothing, like upstream: an unknown hash fails the whole batch.
#[test]
fn read_metadata_with_unknown_hash_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    let known = last(dir);
    let unknown = "1".repeat(40);
    let provider = provider();

    let mut got = Vec::new();
    assert!(
        provider
            .read_metadata(&root, std::slice::from_ref(&unknown), &mut |m| got.push(m))
            .is_err()
    );
    // all-or-nothing: the known hash in the batch does not rescue it
    assert!(
        provider
            .read_metadata(&root, &[known, unknown], &mut |m| got.push(m))
            .is_err()
    );
}

/// The metadata half of upstream `test full details`: short presentation plus full message of the
/// metadata read by hash equals what `git log` reports (same history as `test_short_details`).
#[test]
fn metadata_presentation_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    // prepareLongHistory
    for i in 0..15 {
        let file = format!("a{}.txt", i % 10);
        if i < 10 {
            tac(dir, &file);
        } else {
            std::fs::write(dir.join(&file), format!("modified {i}")).unwrap();
            git(dir, &["commit", "-q", "-am", &format!("Modified {file}")]);
        }
    }
    let output = git(
        dir,
        &[
            "log",
            "--all",
            "--date-order",
            "--full-history",
            "--sparse",
            "--pretty=%H|%P|%ct|%s|%B",
        ],
    );
    let expected: Vec<String> = output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|record| {
            let items: Vec<&str> = record.split('|').collect();
            let time = items[2].parse::<i64>().unwrap() * 1000;
            let commit = VcsCommitMetadata::new(
                Hash::build(items[0]),
                items[1]
                    .split(' ')
                    .filter(|p| !p.is_empty())
                    .map(Hash::build)
                    .collect(),
                time,
                root.clone(),
                items[3].to_string(),
                default_user(),
                items[4].to_string(),
                default_user(),
                time,
            );
            metadata_presentation(&commit)
        })
        .collect();

    let provider = provider();
    let mut hashes = Vec::new();
    provider
        .read_all_hashes(&root, &mut |c| hashes.push(c.id().as_string()))
        .unwrap();
    let mut actual = Vec::new();
    provider
        .read_metadata(&root, &hashes, &mut |m| {
            actual.push(metadata_presentation(&m))
        })
        .unwrap();
    assert_eq!(actual, expected);
}

/// Upstream `metadataPresentation`: `shortPresentation` + `fullMessage`.
fn metadata_presentation(c: &VcsCommitMetadata) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}{}",
        c.id().to_short_string(),
        c.author_time(),
        c.author(),
        c.commit_time(),
        c.committer(),
        c.subject(),
        c.full_message()
    )
}

/// The `\u{1}` message case of `test_loading_details_with_u0001_character`, through the
/// first-block path (`collect_metadata_with_params` with `--max-count`).
#[test]
fn first_block_metadata_keeps_u0001_message() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    let message =
        "subject containing \u{1} symbol in it\n\ncommit body containing \u{1} symbol in it";
    std::fs::write(dir.join("file.txt"), "content").unwrap();
    let msg_file = dir.join(".git").join("TEST_MSG");
    std::fs::write(&msg_file, message).unwrap();
    git(dir, &["add", "file.txt"]);
    git(dir, &["commit", "-q", "-F", msg_file.to_str().unwrap()]);

    let data = collect_metadata_with_params(&runner(), &root, &["--max-count=1"]).unwrap();
    assert_eq!(data.commits.len(), 1);
    assert_eq!(data.commits[0].full_message(), message);
}
