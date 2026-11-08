//! Repository reader integration tests against real repositories.

mod common;

use common::{git, git_output, init_repo, runner};
use ig_git::config::read_remotes;
use ig_git::reader::{GitRepositoryReader, RepositoryState};
use ig_git::repo_files::{GitRepositoryFiles, find_git_dir};
use ig_git::status::{FileStatus, StatusOptions, get_file_status};

fn read_state(root: &std::path::Path) -> ig_git::reader::GitBranchState {
    let g = runner();
    let git_dir = find_git_dir(root).expect("git dir");
    let files = GitRepositoryFiles::create(root, &git_dir);
    let remotes = read_remotes(&g, root);
    GitRepositoryReader::new(&files, &g).read_state(&remotes)
}

#[test]
fn reads_branch_detached_and_merge_states() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_repo(root);
    let head = git(root, &["rev-parse", "HEAD"]).trim().to_string();

    let state = read_state(root);
    assert_eq!(state.state, RepositoryState::Normal);
    assert_eq!(
        state.current_branch.as_ref().map(|b| b.name()),
        Some("main")
    );
    assert_eq!(state.current_revision.as_deref(), Some(head.as_str()));

    // conflicting merge
    git(root, &["checkout", "-q", "-b", "other"]);
    std::fs::write(root.join("a.txt"), "other\n").unwrap();
    git(root, &["commit", "-q", "-am", "other"]);
    git(root, &["checkout", "-q", "main"]);
    std::fs::write(root.join("a.txt"), "main\n").unwrap();
    git(root, &["commit", "-q", "-am", "main"]);
    let merge = git_output(root, &["merge", "other"]);
    assert!(!merge.status.success(), "merge should conflict");
    assert!(String::from_utf8_lossy(&merge.stdout).contains("CONFLICT"));
    let state = read_state(root);
    assert_eq!(state.state, RepositoryState::Merging);
    let conflicted = get_file_status(&runner(), root, &[], StatusOptions::default()).unwrap();
    assert_eq!(conflicted[0].file_status(), FileStatus::MergedWithConflicts);
    git(root, &["merge", "--abort"]);

    git(root, &["checkout", "-q", "--detach", &head]);
    let state = read_state(root);
    assert_eq!(state.state, RepositoryState::Detached);
    assert_eq!(state.current_branch, None);
    assert_eq!(state.current_revision.as_deref(), Some(head.as_str()));
}

#[test]
fn reads_linked_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path().join("main");
    init_repo(&main);
    let wt = tmp.path().join("wt");
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "wt-branch",
            wt.to_str().unwrap(),
        ],
    );

    let git_dir = find_git_dir(&wt).expect("worktree .git file resolves");
    let files = GitRepositoryFiles::create(&wt, &git_dir);
    assert_eq!(
        std::fs::canonicalize(files.common_git_dir()).unwrap(),
        std::fs::canonicalize(main.join(".git")).unwrap()
    );
    let state = read_state(&wt);
    assert_eq!(
        state.current_branch.as_ref().map(|b| b.name()),
        Some("wt-branch")
    );
    assert!(state.local_branches.keys().any(|b| b.name() == "main"));
}

fn commit_file(root: &std::path::Path, file: &str, content: &str, msg: &str) {
    std::fs::write(root.join(file), content).unwrap();
    git(root, &["add", file]);
    git(root, &["commit", "-q", "-m", msg]);
}

/// `init_repo`, then `feat` changes `a.txt` and `main` changes it differently. Ends on `main`.
fn conflicting_branches(root: &std::path::Path) {
    init_repo(root);
    git(root, &["checkout", "-q", "-b", "feat"]);
    commit_file(root, "a.txt", "feat\n", "feat");
    git(root, &["checkout", "-q", "main"]);
    commit_file(root, "a.txt", "main\n", "main");
}

fn reftable_supported(dir: &std::path::Path) -> bool {
    git_output(dir, &["init", "-q", "--ref-format=reftable", "-b", "main"])
        .status
        .success()
}

fn assert_failed(out: &std::process::Output, what: &str) {
    assert!(!out.status.success(), "{what} should conflict");
}

#[test]
fn unborn_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    git(root, &["init", "-q", "-b", "main"]);
    let state = read_state(root);
    assert_eq!(state.state, RepositoryState::Normal);
    assert_eq!(
        state.current_branch.as_ref().map(|b| b.name()),
        Some("main")
    );
    assert_eq!(state.current_revision, None);
}

fn assert_rebasing(root: &std::path::Path) {
    let state = read_state(root);
    assert_eq!(state.state, RepositoryState::Rebasing);
    assert_eq!(
        state.current_branch.as_ref().map(|b| b.name()),
        Some("feat")
    );
    assert!(state.current_revision.is_some());
}

#[test]
fn rebase_in_progress_reports_rebased_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    conflicting_branches(root);
    git(root, &["checkout", "-q", "feat"]);
    assert_failed(&git_output(root, &["rebase", "main"]), "rebase");
    assert_rebasing(root);
}

#[test]
fn apply_rebase_in_progress() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    conflicting_branches(root);
    git(root, &["checkout", "-q", "feat"]);
    assert_failed(
        &git_output(root, &["rebase", "--apply", "main"]),
        "rebase --apply",
    );
    assert_rebasing(root);
}

#[test]
fn cherry_pick_conflict_is_grafting() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    conflicting_branches(root);
    assert_failed(&git_output(root, &["cherry-pick", "feat"]), "cherry-pick");
    assert_eq!(read_state(root).state, RepositoryState::Grafting);
}

#[test]
fn revert_conflict_is_reverting() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_repo(root);
    commit_file(root, "a.txt", "two\n", "c1");
    let c1 = git(root, &["rev-parse", "HEAD"]).trim().to_string();
    commit_file(root, "a.txt", "three\n", "c2");
    assert_failed(&git_output(root, &["revert", "--no-edit", &c1]), "revert");
    assert_eq!(read_state(root).state, RepositoryState::Reverting);
}

#[test]
fn multi_commit_cherry_pick_after_resolving_first_is_grafting() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_repo(root);
    git(root, &["checkout", "-q", "-b", "feat"]);
    commit_file(root, "a.txt", "feat\n", "f1");
    commit_file(root, "b.txt", "b\n", "f2");
    git(root, &["checkout", "-q", "main"]);
    commit_file(root, "a.txt", "main\n", "main");
    assert_failed(
        &git_output(root, &["cherry-pick", "main..feat"]),
        "cherry-pick main..feat",
    );
    std::fs::write(root.join("a.txt"), "resolved\n").unwrap();
    git(root, &["add", "a.txt"]);
    git(root, &["commit", "--no-edit", "-q"]);
    assert!(!root.join(".git/CHERRY_PICK_HEAD").exists());
    assert_eq!(read_state(root).state, RepositoryState::Grafting);
}

#[test]
fn reftable_repository_states() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    if !reftable_supported(root) {
        eprintln!("skipping: git lacks reftable support");
        return;
    }
    commit_file(root, "a.txt", "one\n", "initial");
    git(root, &["branch", "other"]);
    let head = git(root, &["rev-parse", "HEAD"]).trim().to_string();

    let state = read_state(root);
    assert_eq!(state.state, RepositoryState::Normal);
    assert_eq!(
        state.current_branch.as_ref().map(|b| b.name()),
        Some("main")
    );
    assert_eq!(state.current_revision.as_deref(), Some(head.as_str()));
    let mut names: Vec<String> = state
        .local_branches
        .keys()
        .map(|b| b.name().to_string())
        .collect();
    names.sort();
    assert_eq!(names, ["main", "other"]);

    git(root, &["checkout", "-q", "--detach"]);
    let state = read_state(root);
    assert_eq!(state.state, RepositoryState::Detached);
    assert_eq!(state.current_revision.as_deref(), Some(head.as_str()));

    git(root, &["checkout", "-q", "main"]);
    git(root, &["checkout", "-q", "--orphan", "newroot"]);
    let state = read_state(root);
    assert_eq!(state.state, RepositoryState::Normal);
    assert_eq!(
        state.current_branch.as_ref().map(|b| b.name()),
        Some("newroot")
    );
    assert_eq!(state.current_revision, None);
}

#[test]
fn reftable_cherry_pick_conflict_is_grafting() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    if !reftable_supported(root) {
        eprintln!("skipping: git lacks reftable support");
        return;
    }
    commit_file(root, "a.txt", "one\n", "initial");
    git(root, &["checkout", "-q", "-b", "feat"]);
    commit_file(root, "a.txt", "feat\n", "feat");
    git(root, &["checkout", "-q", "main"]);
    commit_file(root, "a.txt", "main\n", "main");
    assert_failed(&git_output(root, &["cherry-pick", "feat"]), "cherry-pick");
    assert_eq!(read_state(root).state, RepositoryState::Grafting);
}

#[test]
fn reftable_revert_conflict_is_reverting() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    if !reftable_supported(root) {
        eprintln!("skipping: git lacks reftable support");
        return;
    }
    commit_file(root, "a.txt", "one\n", "initial");
    commit_file(root, "a.txt", "two\n", "c1");
    let c1 = git(root, &["rev-parse", "HEAD"]).trim().to_string();
    commit_file(root, "a.txt", "three\n", "c2");
    assert_failed(&git_output(root, &["revert", "--no-edit", &c1]), "revert");
    assert!(
        !root.join(".git/REVERT_HEAD").is_file(),
        "reftable keeps REVERT_HEAD out of the files backend"
    );
    assert_eq!(read_state(root).state, RepositoryState::Reverting);
}

#[test]
fn non_utf8_sequencer_todo_is_still_grafting() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_repo(root);
    let dir = root.join(".git/sequencer");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("todo"), b"pick 0000000 caf\xe9 subject\n").unwrap();
    assert_eq!(read_state(root).state, RepositoryState::Grafting);
}
