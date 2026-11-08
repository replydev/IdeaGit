//! `git status` integration tests against the real `git` executable.

mod common;

use common::{git, init_repo, runner};
use ig_git::status::{FileStatus, StatusOptions, get_file_status};

#[test]
fn status_reports_changes_renames_and_untracked() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_repo(root);
    std::fs::write(root.join("a.txt"), "two\n").unwrap();
    std::fs::write(root.join("b.txt"), "b\n").unwrap();
    git(root, &["add", "b.txt"]);
    git(root, &["commit", "-q", "-m", "add b"]);
    git(root, &["mv", "b.txt", "c d.txt"]);
    std::fs::create_dir_all(root.join("dir")).unwrap();
    std::fs::write(root.join("dir/new.txt"), "n\n").unwrap();

    let g = runner();
    let mut records = get_file_status(&g, root, &[], StatusOptions::default()).unwrap();
    records.sort_by(|a, b| a.path.cmp(&b.path));
    let summary: Vec<(String, FileStatus, Option<String>)> = records
        .iter()
        .map(|r| (r.path.clone(), r.file_status(), r.orig_path.clone()))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("a.txt".into(), FileStatus::Modified, None),
            ("c d.txt".into(), FileStatus::Modified, Some("b.txt".into())),
            ("dir/new.txt".into(), FileStatus::Unknown, None),
        ]
    );

    let only_a =
        get_file_status(&g, root, &[root.join("a.txt")], StatusOptions::default()).unwrap();
    assert_eq!(only_a.len(), 1);
    assert_eq!(only_a[0].path, "a.txt");
}
