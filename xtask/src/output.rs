//! Deterministic output: files are written only when their content changes, and files left over
//! in generated directories from an earlier import are removed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::{Error, Result, Summary};

pub struct Output {
    root: PathBuf,
    pending: Vec<(String, Vec<u8>)>,
}

impl Output {
    pub fn new(root: &Path) -> Self {
        Output {
            root: root.to_path_buf(),
            pending: Vec::new(),
        }
    }

    pub fn text(&mut self, rel: &str, text: &str) {
        self.bytes(rel, text.as_bytes());
    }

    pub fn bytes(&mut self, rel: &str, bytes: &[u8]) {
        self.pending.push((rel.to_string(), bytes.to_vec()));
    }

    /// Writes pending files, then removes files under `generated_dirs` that weren't written.
    pub fn finish(self, generated_dirs: &[&str]) -> Result<Summary> {
        let mut summary = Summary::default();
        let mut keep = BTreeSet::new();
        for (rel, _) in &self.pending {
            if !keep.insert(rel.clone()) {
                return Err(Error(format!("two outputs map to {rel}")));
            }
        }
        for (rel, bytes) in self.pending {
            let path = self.root.join(&rel);
            if std::fs::read(&path).is_ok_and(|old| old == bytes) {
                summary.unchanged += 1;
                continue;
            }
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, &bytes)
                .map_err(|e| Error(format!("cannot write {}: {e}", path.display())))?;
            summary.written.push(rel);
        }
        for dir in generated_dirs {
            let mut stale = Vec::new();
            list_files(&self.root.join(dir), dir, &mut stale)?;
            for rel in stale.into_iter().filter(|r| !keep.contains(r)) {
                std::fs::remove_file(self.root.join(&rel))?;
                summary.removed.push(rel);
            }
        }
        Ok(summary)
    }
}

fn list_files(dir: &Path, rel: &str, out: &mut Vec<String>) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        let child = format!("{rel}/{}", entry.file_name().to_string_lossy());
        if entry.file_type()?.is_dir() {
            list_files(&entry.path(), &child, out)?;
        } else {
            out.push(child);
        }
    }
    Ok(())
}
