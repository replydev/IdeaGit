//! References (branches, tags, HEAD) shown in the log.
//!
//! Ported from `platform/vcs-log/api/src/com/intellij/vcs/log/VcsRef.java`,
//! `platform/vcs-log/api/src/com/intellij/vcs/log/VcsRefType.java`,
//! `platform/vcs-log/api/src/com/intellij/vcs/log/VcsRefsContainer.kt` and
//! `platform/vcs-log/impl/src/com/intellij/vcs/log/impl/VcsRefImpl.java`.
//!
//! Deviations: a ref type carries no background color (a UI concern, to be mapped by the Log UI
//! from [`VcsRefType::name`]); ref names are not interned (`VcsRefNamesInterner`).

use std::fmt;

use crate::data::hash::Hash;
use crate::data::root::VcsRoot;

/// The type of a reference (`VcsRefType`). Types are compared by name and branch flag; upstream
/// compares the singleton instances by identity.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VcsRefType {
    name: &'static str,
    is_branch: bool,
}

impl VcsRefType {
    pub const fn new(name: &'static str, is_branch: bool) -> VcsRefType {
        VcsRefType { name, is_branch }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Whether this is a branch-like reference that defines a branch layout and color.
    pub fn is_branch(&self) -> bool {
        self.is_branch
    }
}

impl fmt::Display for VcsRefType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

/// A reference to a commit (`VcsRefImpl`): equal when hash, name, type and root are equal.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct VcsRef {
    commit_hash: Hash,
    name: String,
    ref_type: VcsRefType,
    root: VcsRoot,
}

impl VcsRef {
    pub fn new(commit_hash: Hash, name: &str, ref_type: VcsRefType, root: VcsRoot) -> VcsRef {
        VcsRef {
            commit_hash,
            name: name.to_string(),
            ref_type,
            root,
        }
    }

    pub fn commit_hash(&self) -> &Hash {
        &self.commit_hash
    }

    /// The short name, e.g. `main`, `origin/main`, `v1.0` or `HEAD`.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ref_type(&self) -> VcsRefType {
        self.ref_type
    }

    pub fn root(&self) -> &VcsRoot {
        &self.root
    }
}

impl fmt::Debug for VcsRef {
    /// `VcsRefImpl.toString`: `root-name:name(hash|type)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let root_name = self
            .root
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        write!(
            f,
            "{root_name}:{}({}|{})",
            self.name, self.commit_hash, self.ref_type
        )
    }
}

/// A collection of references split into branches and tags (`VcsRefsContainer`).
pub trait VcsRefsContainer {
    fn branches(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_>;

    fn tags(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_>;

    fn all_refs(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.branches().chain(self.tags()))
    }
}

/// A [`VcsRefsContainer`] over a list of refs: branch-typed refs are branches, all others tags
/// (`TestVcsRefsSequences` in upstream tests, `EmptyRefs` when empty).
#[derive(Clone, Debug, Default)]
pub struct VcsRefsList(pub Vec<VcsRef>);

impl VcsRefsContainer for VcsRefsList {
    fn branches(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.0.iter().filter(|r| r.ref_type().is_branch()))
    }

    fn tags(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.0.iter().filter(|r| !r.ref_type().is_branch()))
    }
}
