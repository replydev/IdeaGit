//! Ports of `plugins/git4idea/tests/git4idea/log/GitLogProviderTest.kt` (`GitLogProviderTest` and
//! `GitReadRecentCommitsTest`) and `plugins/git4idea/tests/git4idea/history/GitLogUtilTest.kt`,
//! against real repositories.
//!
//! Not ported (out of scope, see `reference/upstream-map.toml`): the filter tests (`test filter by
//! branch`, `... and user`, `test by range`, `... and branch`, `test filter by text`, `... no
//! regex`, `... with regex and user`, `... simple text and user`), `test full details`, the
//! readFullDetails cases of `GitLogUtilTest` (renames, merge diffs), and the
//! `experimental refs collection = true` parameter of `GitReadRecentCommitsTest`.
//! `GitLogUtilTest`'s two message/hash cases read through metadata instead of full details.

mod common;

use std::path::Path;

use common::log_helpers::{default_user, init_master_repo, last, provider, read_all_refs, tac};
use common::{git, git_output, runner};
use ig_git::log::provider::GitLogProvider;
use ig_git::log::ref_manager::{LOCAL_BRANCH, TAG};
use ig_git::log::util::collect_metadata_with_params;
use ig_log::GraphCommit;
use ig_log::data::{
    DetailedLogData, Hash, Requirements, TimedVcsCommit, VcsCommitMetadata, VcsLogProvider,
    VcsRefsList, VcsRoot, vcs_root,
};

/// `readCommitsFromGit`: `git log --all --date-order --full-history --sparse`.
fn read_commits_from_git(dir: &Path, root: &VcsRoot) -> Vec<VcsCommitMetadata> {
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
    output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|record| {
            let items: Vec<&str> = record.split('|').collect();
            let time = items[2].parse::<i64>().unwrap() * 1000;
            VcsCommitMetadata::new(
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
            )
        })
        .collect()
}

fn ids<C: GraphCommit<Hash>>(commits: &[C]) -> Vec<Hash> {
    commits.iter().map(|c| c.id().clone()).collect()
}

fn sorted_ids<C: GraphCommit<Hash>>(commits: &[C]) -> Vec<Hash> {
    let mut v = ids(commits);
    v.sort();
    v
}

fn prepare_some_history(dir: &Path) {
    tac(dir, "a.txt");
    git(dir, &["tag", "ATAG"]);
    tac(dir, "b.txt");
}

fn create_tagged_branch(dir: &Path) {
    let hash = last(dir);
    tac(dir, "c.txt");
    tac(dir, "d.txt");
    tac(dir, "e.txt");
    git(dir, &["tag", "poor-tag"]);
    git(dir, &["reset", "-q", "--hard", &hash]);
}

/// `readRecentCommits` of the test: `repo.update()`, then `readFirstBlock`.
fn read_recent_commits(
    provider: &GitLogProvider,
    root: &VcsRoot,
    requirements: &Requirements<'_>,
) -> DetailedLogData {
    provider.repository_manager().update(root);
    provider.read_first_block(root, requirements).unwrap()
}

fn short_presentation(c: &VcsCommitMetadata) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        c.id().to_short_string(),
        c.author_time(),
        c.author(),
        c.commit_time(),
        c.committer(),
        c.subject()
    )
}

// ---- GitLogProviderTest

#[test]
fn test_all_log_with_tagged_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    create_tagged_branch(dir);
    let expected_log = read_commits_from_git(dir, &root);
    let mut collector: Vec<TimedVcsCommit> = Vec::new();
    provider()
        .read_all_hashes(&root, &mut |c| collector.push(c))
        .unwrap();
    assert_eq!(ids(&collector), ids(&expected_log));
}

fn check_full_hash(object_format: &str, hex_size: usize, prefix_len: usize) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(format!("{object_format}-repo"));
    std::fs::create_dir_all(&dir).unwrap();
    if object_format == "sha1" {
        git(&dir, &["init", "-q"]);
    } else {
        // `--object-format` needs git 2.29
        let init = git_output(
            &dir,
            &["init", "-q", &format!("--object-format={object_format}")],
        );
        if !init.status.success() {
            eprintln!("skipping: git lacks --object-format=sha256");
            return;
        }
    }
    git(&dir, &["symbolic-ref", "HEAD", "refs/heads/master"]);
    tac(&dir, "initial.txt");
    let root = vcs_root(&dir);
    let provider = provider();
    provider.repository_manager().update(&root);
    let head_hash = last(&dir);
    assert_eq!(head_hash.len(), hex_size);

    assert!(provider.is_full_hash(&root, &head_hash));
    assert!(!provider.is_full_hash(&root, &head_hash[..prefix_len]));
    assert!(!provider.is_full_hash(&root, &"z".repeat(hex_size)));
}

#[test]
fn test_full_hash_check_sha1_repository() {
    check_full_hash("sha1", 40, 7);
}

#[test]
fn test_full_hash_check_sha256_repository() {
    check_full_hash("sha256", 64, 40);
}

#[test]
fn test_get_current_user() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    git(dir, &["config", "user.name", "John Doe"]);
    git(dir, &["config", "user.email", "John.Doe@example.com"]);
    let user = provider().get_current_user(&root).unwrap();
    let user = user.expect("User is not defined");
    assert_eq!(user.name(), "John Doe", "User name is incorrect");
    assert_eq!(
        user.email(),
        "john.doe@example.com",
        "User email is incorrect"
    );
}

/// IdeaGit: without `user.name` there is no current user.
#[test]
fn current_user_without_config_is_none() {
    let tmp = tempfile::tempdir().unwrap();
    let root = init_master_repo(tmp.path());
    assert_eq!(provider().get_current_user(&root).unwrap(), None);
}

#[test]
fn test_short_details() {
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
    let log = read_commits_from_git(dir, &root);

    let provider = provider();
    let mut hashes = Vec::new();
    provider
        .read_all_hashes(&root, &mut |c| hashes.push(c.id().as_string()))
        .unwrap();

    let mut result = Vec::new();
    provider
        .read_metadata(&root, &hashes, &mut |m| result.push(m))
        .unwrap();

    let actual: Vec<String> = result.iter().map(short_presentation).collect();
    let expected: Vec<String> = log.iter().map(short_presentation).collect();
    assert_eq!(actual, expected);
}

// ---- GitReadRecentCommitsTest (experimental refs collection = false)

#[test]
fn test_init_with_tagged_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    let expected_log_without_tagged_branch = read_commits_from_git(dir, &root);
    create_tagged_branch(dir);

    let block = read_recent_commits(&provider(), &root, &Requirements::simple(1000));
    assert_eq!(
        ids(&block.commits),
        ids(&expected_log_without_tagged_branch)
    );
}

#[test]
fn test_refresh_with_new_tagged_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    let prev_refs = VcsRefsList(read_all_refs(dir, &root).into_iter().collect());
    create_tagged_branch(dir);

    let expected_log = read_commits_from_git(dir, &root);
    let block = read_recent_commits(
        &provider(),
        &root,
        &Requirements::refresh(1000, true, &prev_refs),
    );
    assert_eq!(sorted_ids(&block.commits), sorted_ids(&expected_log));
}

#[test]
fn test_refresh_when_new_tag_moved() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    let prev_refs = VcsRefsList(read_all_refs(dir, &root).into_iter().collect());
    git(dir, &["tag", "-f", "ATAG"]);

    let expected_log = read_commits_from_git(dir, &root);
    let refs = read_all_refs(dir, &root);
    let block = read_recent_commits(
        &provider(),
        &root,
        &Requirements::refresh(1000, true, &prev_refs),
    );
    assert_eq!(sorted_ids(&block.commits), sorted_ids(&expected_log));
    assert_eq!(block.refs.len(), refs.len(), "{:?} vs {refs:?}", block.refs);
    assert_eq!(
        block
            .refs
            .into_iter()
            .collect::<std::collections::HashSet<_>>(),
        refs
    );
}

#[test]
fn test_new_tag_on_old_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    let prev_refs = VcsRefsList(read_all_refs(dir, &root).into_iter().collect());
    let commits = read_commits_from_git(dir, &root);
    let first_commit = commits[commits.len() - 1].id().as_string();
    git(dir, &["tag", "NEW_TAG", &first_commit]);

    let refs = read_all_refs(dir, &root);
    let block = read_recent_commits(
        &provider(),
        &root,
        &Requirements::refresh(1000, true, &prev_refs),
    );
    assert_eq!(block.refs.len(), refs.len(), "{:?} vs {refs:?}", block.refs);
    assert_eq!(
        block
            .refs
            .into_iter()
            .collect::<std::collections::HashSet<_>>(),
        refs
    );
}

#[test]
fn test_dont_report_origin_head() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    git(dir, &["update-ref", "refs/remotes/origin/HEAD", "master"]);

    let block = read_recent_commits(&provider(), &root, &Requirements::simple(1000));
    assert!(
        block.refs.iter().all(|r| r.name() != "origin/HEAD"),
        "origin/HEAD should be ignored"
    );
}

#[test]
fn test_support_equally_named_branch_and_tag() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    git(dir, &["branch", "build"]);
    git(dir, &["tag", "build"]);

    let empty = VcsRefsList::default();
    let data = read_recent_commits(
        &provider(),
        &root,
        &Requirements::refresh(1000, true, &empty),
    );
    let expected_log = read_commits_from_git(dir, &root);
    assert_eq!(ids(&data.commits), ids(&expected_log));
    assert!(
        data.refs
            .iter()
            .any(|r| r.name() == "build" && r.ref_type() == LOCAL_BRANCH)
    );
    assert!(
        data.refs
            .iter()
            .any(|r| r.name() == "build" && r.ref_type() == TAG)
    );
}

// ---- GitLogUtilTest

#[test]
fn test_loading_details_with_u0001_character() {
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

    let mut details = Vec::new();
    provider()
        .read_metadata(&root, &[last(dir)], &mut |m| details.push(m))
        .unwrap();
    let last_commit = details.first().expect("commit");
    assert_eq!(last_commit.full_message(), message);
}

#[test]
fn test_loading_details_without_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    let message_file = dir.join("message.txt");
    std::fs::write(&message_file, "").unwrap();

    let commit_count = 20;
    let mut expected = Vec::new();
    for i in 0..commit_count {
        std::fs::write(dir.join("file.txt"), format!("content number {i}")).unwrap();
        git(dir, &["add", "."]);
        git(
            dir,
            &[
                "commit",
                "-q",
                "--allow-empty-message",
                "-F",
                message_file.to_str().unwrap(),
            ],
        );
        expected.push(last(dir));
    }
    expected.reverse();

    let max_count = format!("--max-count={commit_count}");
    let data = collect_metadata_with_params(&runner(), &root, &[&max_count]).unwrap();
    let actual_hashes: Vec<String> = data.commits.iter().map(|c| c.id().as_string()).collect();
    assert_eq!(actual_hashes, expected);
    assert!(
        data.commits
            .iter()
            .all(|c| c.subject().is_empty() && c.full_message().is_empty())
    );
}

/// IdeaGit: a fresh repository (no commits) has no log data.
#[test]
fn fresh_repository_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q"]);
    git(dir, &["symbolic-ref", "HEAD", "refs/heads/master"]);
    let root = vcs_root(dir);
    let provider = provider();
    let block = provider
        .read_first_block(&root, &Requirements::simple(10))
        .unwrap();
    assert!(block.commits.is_empty() && block.refs.is_empty());
    let mut count = 0;
    let data = provider
        .read_all_hashes(&root, &mut |_| count += 1)
        .unwrap();
    assert_eq!(count, 0);
    assert!(data.refs.is_empty());
    // the branch of a fresh repository is known (upstream `getCurrentBranch`)
    assert_eq!(
        provider.get_current_branch(&root).as_deref(),
        Some("master")
    );
}
