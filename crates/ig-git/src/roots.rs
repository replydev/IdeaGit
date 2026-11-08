//! Git root detection for an opened folder. Simplified port of `VcsRootDetectorImpl` /
//! `GitRootChecker`: the closest enclosing root (the folder itself or an ancestor below the user
//! home, as upstream ignores `~` and above) plus every nested root (submodules, nested clones,
//! linked worktrees) found by walking down.
//!
//! Not ported: skipping directories ignored by `.gitignore` (upstream `isIgnored`); a fixed list of
//! tool/cache directory names is skipped instead. Paths are not canonicalized, so callers should
//! pass one canonical absolute path. Keep `max_depth` small (3-5).

use std::path::{Path, PathBuf};

use crate::repo_files::find_git_dir;

/// Directory names never descended into while scanning for nested roots.
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".idea",
    ".ideagit",
    ".venv",
    "venv",
    ".gradle",
    ".terraform",
    ".cache",
];

/// Returns git roots relevant to the absolute path `dir`, sorted, without duplicates.
pub fn detect_roots(dir: &Path, max_depth: usize) -> Vec<PathBuf> {
    detect_roots_with_home(dir, max_depth, user_home().as_deref())
}

/// [`detect_roots`] with an explicit user home (ancestors at or above it are not considered).
pub fn detect_roots_with_home(dir: &Path, max_depth: usize, home: Option<&Path>) -> Vec<PathBuf> {
    debug_assert!(
        dir.is_absolute(),
        "detect_roots needs an absolute path: {}",
        dir.display()
    );
    let mut roots = Vec::new();
    for ancestor in dir.ancestors() {
        if ancestor != dir && home.is_some_and(|h| ancestor == h) {
            break; // upstream: "ignore mappings in ~/ and above"
        }
        if find_git_dir(ancestor).is_some() {
            roots.push(ancestor.to_path_buf());
            break;
        }
    }
    scan(dir, 0, max_depth, &mut roots);
    roots.sort();
    roots.dedup();
    roots
}

fn user_home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn scan(dir: &Path, depth: usize, max_depth: usize, roots: &mut Vec<PathBuf>) {
    if depth > max_depth {
        return;
    }
    if depth > 0 && find_git_dir(dir).is_some() {
        roots.push(dir.to_path_buf());
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() || ft.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        if SKIPPED_DIRS.iter().any(|s| name == *s) {
            continue;
        }
        scan(&entry.path(), depth + 1, max_depth, roots);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_repo(path: &Path) {
        std::fs::create_dir_all(path.join(".git")).unwrap();
        std::fs::write(path.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    }

    #[test]
    fn finds_enclosing_and_nested_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().join("outer");
        make_repo(&outer);
        make_repo(&outer.join("libs/inner"));
        std::fs::create_dir_all(outer.join("node_modules/pkg")).unwrap();
        make_repo(&outer.join("node_modules/pkg"));

        assert_eq!(
            detect_roots_with_home(&outer, 5, None),
            vec![outer.clone(), outer.join("libs/inner")]
        );
        assert_eq!(
            detect_roots_with_home(&outer.join("libs"), 5, None),
            vec![outer.clone(), outer.join("libs/inner")]
        );
    }

    #[test]
    fn no_roots_outside_repositories() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(detect_roots_with_home(tmp.path(), 3, None).is_empty());
    }

    #[test]
    fn ignores_roots_at_or_above_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        make_repo(&home); // e.g. a dotfiles repository in ~
        let project = home.join("projects/plain");
        std::fs::create_dir_all(&project).unwrap();
        assert!(detect_roots_with_home(&project, 3, Some(&home)).is_empty());
        // opening home itself still reports it
        assert_eq!(
            detect_roots_with_home(&home, 0, Some(&home)),
            vec![home.clone()]
        );
    }

    #[test]
    fn depth_bound_is_inclusive_and_git_files_count() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("ws");
        make_repo(&root.join("a"));
        make_repo(&root.join("x/b"));
        // a worktree/submodule style `.git` file pointing at a real admin dir
        let admin = tmp.path().join("admin");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::create_dir_all(root.join("c")).unwrap();
        std::fs::write(
            root.join("c/.git"),
            format!("gitdir: {}\n", admin.display()),
        )
        .unwrap();

        assert_eq!(
            detect_roots_with_home(&root, 1, None),
            vec![root.join("a"), root.join("c")]
        );
        assert_eq!(
            detect_roots_with_home(&root, 2, None),
            vec![root.join("a"), root.join("c"), root.join("x/b")]
        );
    }
}
