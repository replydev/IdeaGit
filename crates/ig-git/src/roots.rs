//! Git root detection for an opened folder. Simplified port of `VcsRootDetectorImpl` /
//! `GitRootChecker`: the closest enclosing root (the folder itself or an ancestor below the user
//! home, as upstream ignores `~` and above) plus every nested root (submodules, nested clones,
//! linked worktrees) found by walking down.
//!
//! Like upstream (`VcsRootScanner.isIgnoredDirectory` with the registry default
//! `vcs.root.detector.ignore.pattern` = `[.].*|vendor`), no roots are detected inside directories
//! whose name starts with a dot or is `vendor`, and nothing is scanned when the opened folder itself
//! is under such a directory (`isUnderIgnoredDirectory`); the enclosing root is still found.
//!
//! Not ported: skipping directories excluded in the project model or ignored by the VCS
//! (`isIgnoredFileRoot`); a few tool/cache directory names are skipped instead. Paths are not
//! canonicalized, so callers should pass one canonical absolute path. Keep `max_depth` small (3-5).

use std::path::{Path, PathBuf};

use crate::repo_files::find_git_dir;

/// IdeaGit addition: directory names never descended into while scanning for nested roots, on top
/// of [`is_ignored_dir_name`] (upstream skips project-excluded directories instead).
const SKIPPED_DIRS: &[&str] = &["node_modules", "target", "venv"];

/// Upstream's default `vcs.root.detector.ignore.pattern`, `[.].*|vendor` (a full match of the
/// directory name).
fn is_ignored_dir_name(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with('.') || name == "vendor"
}

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
    // upstream `isUnderIgnoredDirectory`: no nested scan under an ignored directory
    let under_ignored = dir
        .ancestors()
        .any(|a| a.file_name().is_some_and(is_ignored_dir_name));
    if !under_ignored {
        scan(dir, 0, max_depth, &mut roots);
    }
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
        if is_ignored_dir_name(&name) || SKIPPED_DIRS.iter().any(|s| name == *s) {
            continue;
        }
        scan(&entry.path(), depth + 1, max_depth, roots);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temp dir whose own name doesn't start with a dot (`tempdir()` uses `.tmpXXXX`, which the
    /// ignore pattern would skip, as upstream would).
    fn tmp() -> tempfile::TempDir {
        tempfile::Builder::new().prefix("roots").tempdir().unwrap()
    }

    fn make_repo(path: &Path) {
        std::fs::create_dir_all(path.join(".git")).unwrap();
        std::fs::write(path.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    }

    #[test]
    fn finds_enclosing_and_nested_roots() {
        let tmp = tmp();
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
        let tmp = tmp();
        assert!(detect_roots_with_home(tmp.path(), 3, None).is_empty());
    }

    #[test]
    fn ignores_roots_at_or_above_home() {
        let tmp = tmp();
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
        let tmp = tmp();
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

    /// Upstream `vcs.root.detector.ignore.pattern` (`[.].*|vendor`): no roots are detected inside
    /// dot-directories or `vendor`, e.g. linked worktrees under `.claude/worktrees` (each of which
    /// would load the whole shared history again).
    #[test]
    fn skips_dot_directories_and_vendor() {
        let tmp = tmp();
        let project = tmp.path().join("project");
        make_repo(&project);
        make_repo(&project.join(".claude/worktrees/feature"));
        make_repo(&project.join(".hidden"));
        make_repo(&project.join("vendor/lib"));
        make_repo(&project.join("vendored/lib"));
        make_repo(&project.join("app/.cache/x"));
        assert_eq!(
            detect_roots_with_home(&project, 3, None),
            vec![project.clone(), project.join("vendored/lib")]
        );
    }

    /// Upstream `isUnderIgnoredDirectory`: opening a folder under an ignored directory scans for no
    /// nested roots, but the enclosing root is still found.
    #[test]
    fn no_nested_scan_under_an_ignored_directory() {
        let tmp = tmp();
        let worktree = tmp.path().join("repo/.claude/worktrees/feature");
        make_repo(&tmp.path().join("repo"));
        make_repo(&worktree);
        make_repo(&worktree.join("nested"));
        assert_eq!(
            detect_roots_with_home(&worktree, 3, None),
            vec![worktree.clone()]
        );
    }
}
