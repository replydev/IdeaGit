//! Git root detection for an opened folder. Simplified port of `VcsRootDetector` /
//! `GitRootChecker`: the closest enclosing root (the folder itself or an ancestor) plus every
//! nested root (submodules, nested clones, linked worktrees) found by walking down.

use std::path::{Path, PathBuf};

use crate::repo_files::find_git_dir;

/// Directory names never descended into while scanning for nested roots.
const SKIPPED_DIRS: &[&str] = &[".git", "node_modules", "target", ".idea", ".ideagit"];

/// Returns git roots relevant to `dir`, sorted, without duplicates.
pub fn detect_roots(dir: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(ancestor) = dir.ancestors().find(|a| find_git_dir(a).is_some()) {
        roots.push(ancestor.to_path_buf());
    }
    scan(dir, 0, max_depth, &mut roots);
    roots.sort();
    roots.dedup();
    roots
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
            detect_roots(&outer, 5),
            vec![outer.clone(), outer.join("libs/inner")]
        );
        assert_eq!(
            detect_roots(&outer.join("libs"), 5),
            vec![outer.clone(), outer.join("libs/inner")]
        );
    }

    #[test]
    fn no_roots_outside_repositories() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(detect_roots(tmp.path(), 3).is_empty());
    }
}
