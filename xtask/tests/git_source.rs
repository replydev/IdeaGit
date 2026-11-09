//! IdeaGit tests: `import-ui` through `GitSource` over a throwaway git repository built from the
//! fixture upstream tree must match the `FsSource` import.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use xtask::{FsSource, GitSource, Source, import_ui};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Runs git hermetically (no system or user config, no inherited repository variables).
fn git(home: &Path, repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .env_clear()
        .envs(std::env::var_os("PATH").map(|p| ("PATH", p)))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("HOME", home)
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}

fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    for name in ["theme.keys", "icons.list", "strings.list", "keymap.list"] {
        std::fs::copy(
            fixtures().join("inputs").join(name),
            dir.path().join("assets").join(name),
        )
        .unwrap();
    }
    dir
}

fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, &mut out);
    // Relative paths, so snapshots of different workspaces compare.
    out.into_iter()
        .map(|(p, b)| (p.strip_prefix(dir).unwrap().to_path_buf(), b))
        .collect()
}

/// A temp git repo holding the fixture upstream tree in one commit; returns (home, repo, hash).
fn upstream_repo() -> (tempfile::TempDir, PathBuf, String) {
    let home = tempfile::tempdir().unwrap();
    let repo = home.path().join("upstream");
    copy_tree(&fixtures().join("upstream"), &repo);
    git(home.path(), &repo, &["init", "-q"]);
    git(home.path(), &repo, &["add", "-A"]);
    git(home.path(), &repo, &["commit", "-q", "-m", "fixture"]);
    let hash = git(home.path(), &repo, &["rev-parse", "HEAD"]);
    (home, repo, hash)
}

#[test]
fn git_import_matches_fs_import_and_records_the_full_hash() {
    let (_home, repo, hash) = upstream_repo();
    assert_eq!(hash.len(), 40);

    // A symbolic rev resolves to the full hash.
    let source = GitSource::new(&repo, "HEAD").unwrap();
    assert_eq!(source.commit(), hash);

    let git_ws = workspace();
    import_ui(&source, git_ws.path(), source.commit()).unwrap();
    let fs_ws = workspace();
    let fs_source = FsSource::new(&fixtures().join("upstream")).unwrap();
    import_ui(&fs_source, fs_ws.path(), &hash).unwrap();

    assert_eq!(snapshot(git_ws.path()), snapshot(fs_ws.path()));
    let tokens =
        std::fs::read_to_string(git_ws.path().join("crates/ig-ui-kit/src/theme/tokens.rs"))
            .unwrap();
    assert!(
        tokens.contains(&format!("intellij-community {hash}")),
        "header lacks the full hash"
    );
    assert!(!tokens.contains("HEAD"));
}

#[test]
fn git_source_reports_missing_paths_distinctly() {
    let (_home, repo, _) = upstream_repo();
    let source = GitSource::new(&repo, "HEAD").unwrap();
    let err = source.read("no/such/file.txt").unwrap_err().to_string();
    assert!(err.contains("no/such/file.txt does not exist in"), "{err}");
    assert!(
        source
            .exists("platform/icons/src/expui/general/add.svg")
            .unwrap()
    );
}

#[test]
fn bad_rev_is_a_clear_error() {
    let (_home, repo, _) = upstream_repo();
    let err = GitSource::new(&repo, "no-such-rev")
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("containing commit no-such-rev"), "{err}");

    // An option-looking rev is not interpreted by git.
    let err = GitSource::new(&repo, "--help").err().unwrap().to_string();
    assert!(err.contains("containing commit --help"), "{err}");

    let not_a_repo = tempfile::tempdir().unwrap();
    assert!(GitSource::new(not_a_repo.path(), "HEAD").is_err());
}

#[test]
fn failed_git_import_leaves_the_workspace_unchanged() {
    let (_home, repo, _) = upstream_repo();
    let ws = workspace();
    std::fs::write(
        ws.path().join("assets/icons.list"),
        "general/nope platform/icons/src/expui/general/nope.svg\n",
    )
    .unwrap();
    let before = snapshot(ws.path());
    let source = GitSource::new(&repo, "HEAD").unwrap();
    let err = import_ui(&source, ws.path(), source.commit()).unwrap_err();
    assert!(err.to_string().contains("general/nope"), "{err}");
    assert_eq!(snapshot(ws.path()), before);
}
