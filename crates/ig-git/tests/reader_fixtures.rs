//! Ported from `plugins/git4idea/tests/git4idea/repo/GitRepositoryReaderTest.kt`, using the upstream
//! test data in `fixtures/repo/*` (copied from `plugins/git4idea/testData/repo`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ig_git::console::NullConsole;
use ig_git::reader::GitRepositoryReader;
use ig_git::repo_files::GitRepositoryFiles;
use ig_git::runner::Git;
use ig_git::version::GitVersion;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Copies the fixture into a temp dir and renames `dot_git` to `.git`, like upstream `prepareTest`.
fn prepare(case: &str) -> (tempfile::TempDir, PathBuf) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/repo")
        .join(case);
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("test");
    copy_dir(&src, &root);
    std::fs::rename(root.join("dot_git"), root.join(".git")).unwrap();
    (tmp, root)
}

/// Lines of `<hash> <name>`, empty lines skipped.
fn read_refs(path: &Path) -> Vec<(String, String)> {
    let mut refs: Vec<(String, String)> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let (hash, name) = l.trim().split_once(' ').unwrap();
            (name.to_string(), hash.to_string())
        })
        .collect();
    refs.sort();
    refs
}

fn check_case(case: &str) {
    let (_tmp, root) = prepare(case);
    let files = GitRepositoryFiles::create(&root, &root.join(".git"));
    let git = Git::new("git", GitVersion::NULL, Arc::new(NullConsole));
    // Upstream's test reads remotes from the test project root, which has none: full ref names don't
    // depend on remote resolution, and unknown remotes fall back to the first path segment.
    let state = GitRepositoryReader::new(&files, &git).read_state(&[]);

    let head = std::fs::read_to_string(root.join("head.txt"))
        .unwrap()
        .trim()
        .to_string();
    assert_eq!(
        state.current_revision.as_deref(),
        Some(head.as_str()),
        "HEAD revision"
    );

    let current = std::fs::read_to_string(root.join("current-branch.txt")).unwrap();
    let (current_hash, current_name) = current.trim().split_once(' ').unwrap();
    let current_branch = state.current_branch.clone().expect("on a branch");
    assert_eq!(current_branch.name(), current_name);
    assert_eq!(state.local_branches[&current_branch].as_str(), current_hash);

    let mut local: Vec<(String, String)> = state
        .local_branches
        .iter()
        .map(|(b, h)| (b.full_name(), h.as_str().to_string()))
        .collect();
    local.sort();
    assert_eq!(
        local,
        read_refs(&root.join("local-branches.txt")),
        "local branches"
    );

    let mut remote: Vec<(String, String)> = state
        .remote_branches
        .iter()
        .map(|(b, h)| (b.full_name(), h.as_str().to_string()))
        .collect();
    remote.sort();
    assert_eq!(
        remote,
        read_refs(&root.join("remote-branches.txt")),
        "remote branches"
    );
}

#[test]
fn example1() {
    check_case("example1");
}

#[test]
fn symbolic_refs() {
    check_case("symbolic-refs");
}
