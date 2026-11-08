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
