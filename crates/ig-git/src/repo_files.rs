//! Port of `plugins/git4idea/backend/src/repo/GitRepositoryFiles.java` (paths of git service files,
//! including linked worktrees via `commondir`) and of `GitUtil.findGitDir` / `parsePathToRepository`.

use std::path::{Component, Path, PathBuf};

use crate::error::{GitError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRepositoryFiles {
    root_dir: PathBuf,
    /// Shared `.git` dir (`git rev-parse --git-common-dir`).
    main_dir: PathBuf,
    /// This worktree's admin dir (`<main>/.git/worktrees/<name>` for linked worktrees).
    worktree_dir: PathBuf,
}

impl GitRepositoryFiles {
    /// `root_dir` is the working tree root; `git_dir` its admin dir (see [`find_git_dir`]).
    pub fn create(root_dir: &Path, git_dir: &Path) -> Self {
        let main_dir = main_git_dir_for_worktree(git_dir).unwrap_or_else(|| git_dir.to_path_buf());
        Self {
            root_dir: root_dir.to_path_buf(),
            main_dir,
            worktree_dir: git_dir.to_path_buf(),
        }
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }
    pub fn worktree_git_dir(&self) -> &Path {
        &self.worktree_dir
    }
    /// Shared `.git` dir of all worktrees. Like upstream, it is resolved lexically, so the same
    /// repository can be reached through different paths (e.g. `/tmp` vs `/private/tmp` on macOS):
    /// canonicalize before comparing repositories by this path.
    pub fn common_git_dir(&self) -> &Path {
        &self.main_dir
    }

    // Shared between worktrees: under the main dir.
    pub fn config_file(&self) -> PathBuf {
        self.main_dir.join("config")
    }
    pub fn packed_refs(&self) -> PathBuf {
        self.main_dir.join("packed-refs")
    }
    pub fn reftable_dir(&self) -> PathBuf {
        self.main_dir.join("reftable")
    }
    pub fn refs_heads_dir(&self) -> PathBuf {
        self.main_dir.join("refs").join("heads")
    }
    pub fn refs_remotes_dir(&self) -> PathBuf {
        self.main_dir.join("refs").join("remotes")
    }
    pub fn refs_tags_dir(&self) -> PathBuf {
        self.main_dir.join("refs").join("tags")
    }
    pub fn shallow_file(&self) -> PathBuf {
        self.main_dir.join("shallow")
    }
    pub fn worktrees_dir(&self) -> PathBuf {
        self.main_dir.join("worktrees")
    }

    // Per worktree: under the worktree admin dir.
    pub fn head_file(&self) -> PathBuf {
        self.worktree_dir.join("HEAD")
    }
    pub fn index_file(&self) -> PathBuf {
        self.worktree_dir.join("index")
    }
    pub fn merge_head_file(&self) -> PathBuf {
        self.worktree_dir.join("MERGE_HEAD")
    }
    pub fn cherry_pick_head(&self) -> PathBuf {
        self.worktree_dir.join("CHERRY_PICK_HEAD")
    }
    pub fn revert_head(&self) -> PathBuf {
        self.worktree_dir.join("REVERT_HEAD")
    }
    pub fn sequencer_todo_file(&self) -> PathBuf {
        self.worktree_dir.join("sequencer").join("todo")
    }
    pub fn orig_head_file(&self) -> PathBuf {
        self.worktree_dir.join("ORIG_HEAD")
    }
    pub fn rebase_apply_dir(&self) -> PathBuf {
        self.worktree_dir.join("rebase-apply")
    }
    pub fn rebase_merge_dir(&self) -> PathBuf {
        self.worktree_dir.join("rebase-merge")
    }
}

/// If `git_dir` is a linked worktree admin dir (has `commondir`), returns the main `.git` dir.
fn main_git_dir_for_worktree(git_dir: &Path) -> Option<PathBuf> {
    let common_dir_file = git_dir.join("commondir");
    if !common_dir_file.exists() {
        return None;
    }
    let path_to_main = try_load_file(&common_dir_file).ok()?;
    let relative = normalize(&git_dir.join(&path_to_main));
    if relative.is_dir() {
        return Some(relative);
    }
    let absolute = PathBuf::from(&path_to_main);
    absolute.is_dir().then_some(absolute)
}

/// Upstream `GitUtil.findGitDir`: `<root>/.git` if it is a directory containing `HEAD`, or the
/// directory referenced by a `.git` file (`gitdir: <path>`, used by worktrees and submodules).
pub fn find_git_dir(root_dir: &Path) -> Option<PathBuf> {
    let dot_git = root_dir.join(".git");
    if dot_git.is_dir() {
        return dot_git.join("HEAD").exists().then_some(dot_git);
    }
    if !dot_git.is_file() {
        return None;
    }
    let content = try_load_file(&dot_git).ok()?;
    let path = parse_path_to_repository(&content)?;
    let resolved = normalize(&root_dir.join(path)); // join keeps absolute paths as-is
    resolved.is_dir().then_some(resolved)
}

/// Upstream `GitUtil.parsePathToRepository`.
pub fn parse_path_to_repository(content: &str) -> Option<String> {
    let mut content = content.trim();
    if let Some(rest) = content.strip_prefix("gitdir:") {
        content = rest.trim();
    }
    if content.is_empty() || content.contains('\n') {
        None
    } else {
        Some(content.to_string())
    }
}

/// Upstream `DvcsUtil.tryLoadFile`: file content with normalized line separators, trimmed.
pub fn try_load_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)
        .map_err(|e| GitError::RepoState(format!("couldn't load {}: {e}", path.display())))?;
    let content = String::from_utf8_lossy(&bytes);
    Ok(content
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string())
}

/// Lexically resolves `.` and `..` (upstream `FileUtil.toCanonicalPath`), without touching symlinks.
/// Leading `..` components of a relative path are kept.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gitdir_file_content() {
        assert_eq!(
            parse_path_to_repository("gitdir: ../.git/worktrees/wt\n").as_deref(),
            Some("../.git/worktrees/wt")
        );
        assert_eq!(parse_path_to_repository("  "), None);
    }

    #[test]
    fn normalizes_dot_dot() {
        assert_eq!(
            normalize(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
    }

    #[test]
    fn normalize_keeps_leading_parent_dirs() {
        assert_eq!(normalize(Path::new("../x/./y/..")), PathBuf::from("../x"));
    }

    #[test]
    fn try_load_file_normalizes_and_is_lenient() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("f");
        std::fs::write(&f, b"a\r\nb\rc\xff \n").unwrap();
        assert_eq!(try_load_file(&f).unwrap(), "a\nb\nc\u{fffd}");
        assert!(try_load_file(&tmp.path().join("missing")).is_err());
    }

    #[test]
    fn finds_git_dir_for_repo_submodule_and_rejects_others() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        assert_eq!(
            find_git_dir(&repo),
            None,
            ".git without HEAD is not a repository"
        );
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(find_git_dir(&repo), Some(repo.join(".git")));

        let modules = repo.join(".git/modules/sub");
        std::fs::create_dir_all(&modules).unwrap();
        let sub = repo.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join(".git"), "gitdir: ../.git/modules/sub\n").unwrap();
        assert_eq!(find_git_dir(&sub), Some(modules.clone()));
        let files = GitRepositoryFiles::create(&sub, &modules);
        assert_eq!(
            files.common_git_dir(),
            modules.as_path(),
            "submodule without commondir"
        );

        let dangling = tmp.path().join("dangling");
        std::fs::create_dir_all(&dangling).unwrap();
        std::fs::write(dangling.join(".git"), "gitdir: /no/such/dir\n").unwrap();
        assert_eq!(find_git_dir(&dangling), None);
        assert_eq!(find_git_dir(tmp.path()), None);
    }

    #[test]
    fn worktree_paths_split_between_main_and_worktree_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main/.git");
        let wt_admin = main.join("worktrees/wt");
        std::fs::create_dir_all(&wt_admin).unwrap();
        std::fs::write(wt_admin.join("commondir"), "../..\n").unwrap();
        let files = GitRepositoryFiles::create(&tmp.path().join("wt"), &wt_admin);
        assert_eq!(files.common_git_dir(), main.as_path());
        assert_eq!(files.head_file(), wt_admin.join("HEAD"));
        assert_eq!(files.packed_refs(), main.join("packed-refs"));
    }
}
