//! Where upstream files come from: a git revision of an intellij-community clone (read with
//! `git cat-file`, so sparse checkouts work) or a plain directory tree (used by tests).

use std::cell::OnceCell;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Error, Result, bail};

/// Read-only access to the upstream tree. Paths are relative and `/`-separated.
pub trait Source {
    /// Raw bytes of the file at `path`.
    fn read(&self, path: &str) -> Result<Vec<u8>>;

    /// Every file path in the tree, sorted.
    fn files(&self) -> Result<&[String]>;

    /// Human-readable description, for error messages.
    fn describe(&self) -> String;

    /// The file at `path` as UTF-8 text (a leading BOM is dropped).
    fn read_text(&self, path: &str) -> Result<String> {
        let bytes = self.read(path)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| Error(format!("{path} in {} is not UTF-8", self.describe())))?;
        Ok(text
            .strip_prefix('\u{feff}')
            .map(str::to_string)
            .unwrap_or(text))
    }

    fn exists(&self, path: &str) -> Result<bool> {
        Ok(self
            .files()?
            .binary_search_by(|p| p.as_str().cmp(path))
            .is_ok())
    }

    /// Sorted file paths starting with `prefix`.
    fn files_under(&self, prefix: &str) -> Result<Vec<&str>> {
        let files = self.files()?;
        let start = files.partition_point(|p| p.as_str() < prefix);
        Ok(files[start..]
            .iter()
            .take_while(|p| p.starts_with(prefix))
            .map(String::as_str)
            .collect())
    }
}

/// A revision of a git repository.
pub struct GitSource {
    repo: PathBuf,
    rev: String,
    files: OnceCell<Vec<String>>,
}

impl GitSource {
    /// Checks that `rev` names a commit in `repo`.
    pub fn new(repo: &Path, rev: &str) -> Result<Self> {
        let source = GitSource {
            repo: repo.to_path_buf(),
            rev: rev.to_string(),
            files: OnceCell::new(),
        };
        source
            .git(&[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{rev}^{{commit}}"),
            ])
            .map_err(|e| {
                Error(format!(
                    "{} is not a git repository containing commit {rev}: {e}",
                    repo.display()
                ))
            })?;
        Ok(source)
    }

    fn git(&self, args: &[&str]) -> Result<Vec<u8>> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.repo)
            .args(args)
            .output()
            .map_err(|e| Error(format!("cannot run git: {e}")))?;
        if !out.status.success() {
            bail!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(out.stdout)
    }
}

impl Source for GitSource {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        self.git(&["cat-file", "blob", &format!("{}:{path}", self.rev)])
            .map_err(|_| Error(format!("{path} does not exist in {}", self.describe())))
    }

    fn files(&self) -> Result<&[String]> {
        if self.files.get().is_none() {
            let out = self.git(&["ls-tree", "-r", "-z", "--name-only", &self.rev])?;
            let mut files: Vec<String> = out
                .split(|&b| b == 0)
                .filter(|p| !p.is_empty())
                .map(|p| String::from_utf8_lossy(p).into_owned())
                .collect();
            files.sort();
            let _ = self.files.set(files);
        }
        Ok(self.files.get().map(Vec::as_slice).unwrap_or_default())
    }

    fn describe(&self) -> String {
        format!("{} at {}", self.repo.display(), self.rev)
    }
}

/// A plain directory tree.
pub struct FsSource {
    root: PathBuf,
    files: OnceCell<Vec<String>>,
}

impl FsSource {
    pub fn new(root: &Path) -> Result<Self> {
        if !root.is_dir() {
            bail!("{} is not a directory", root.display());
        }
        Ok(FsSource {
            root: root.to_path_buf(),
            files: OnceCell::new(),
        })
    }
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if entry.file_type()?.is_dir() {
            walk(&entry.path(), &rel, out)?;
        } else {
            out.push(rel);
        }
    }
    Ok(())
}

impl Source for FsSource {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        std::fs::read(self.root.join(path))
            .map_err(|e| Error(format!("cannot read {path} in {}: {e}", self.describe())))
    }

    fn files(&self) -> Result<&[String]> {
        if self.files.get().is_none() {
            let mut files = Vec::new();
            walk(&self.root, "", &mut files)?;
            files.sort();
            let _ = self.files.set(files);
        }
        Ok(self.files.get().map(Vec::as_slice).unwrap_or_default())
    }

    fn describe(&self) -> String {
        self.root.display().to_string()
    }
}
