//! Remote reading through `git config --null -l` against real repositories.

mod common;

use common::{git, init_repo, runner};
use ig_git::config::read_remotes;

#[test]
fn reads_remotes_including_included_files_and_case_sensitive_names() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_repo(root);
    git(
        root,
        &["remote", "add", "origin", "https://example.com/o.git"],
    );
    git(
        root,
        &["config", "remote.Upstream.url", "https://example.com/u.git"],
    );
    std::fs::write(
        root.join(".git/extra.config"),
        "[remote \"inc\"]\n\turl = \"https://example.com/i.git\" ; note\n",
    )
    .unwrap();
    git(root, &["config", "include.path", "extra.config"]);

    let remotes = read_remotes(&runner(), root);
    let mut names: Vec<&str> = remotes.iter().map(|r| r.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["Upstream", "inc", "origin"]);
    let inc = remotes.iter().find(|r| r.name == "inc").unwrap();
    assert_eq!(inc.urls, ["https://example.com/i.git"]);
}

#[test]
fn unreadable_config_yields_no_remotes() {
    let g = ig_git::runner::Git::new(
        "/definitely/not/git",
        ig_git::version::GitVersion::NULL,
        std::sync::Arc::new(ig_git::console::NullConsole),
    );
    assert!(read_remotes(&g, std::path::Path::new(".")).is_empty());
}
