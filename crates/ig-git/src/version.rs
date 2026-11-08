//! Port of `plugins/git4idea/backend/src/config/GitVersion.java` and the version-based entries of
//! `GitVersionSpecialty.kt` that IdeaGit currently needs. WSL types are intentionally not ported.

use std::cmp::Ordering;
use std::sync::LazyLock;

use regex::Regex;

use crate::error::{GitError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitVersionType {
    Unix,
    Msys,
    Cygwin,
    /// The type doesn't matter or couldn't be detected.
    Undefined,
    /// Version information is unavailable (invalid executable or not yet detected).
    Null,
}

/// Note: like upstream, ordering (`compare`) ignores the type while equality takes it into account.
#[derive(Debug, Clone, Copy)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub revision: u32,
    pub patch_level: u32,
    pub ty: GitVersionType,
}

static FORMAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^git version (\d+)\.(\d+)(?:\.(\d+))?(?:\.(\d+))?(.*)$").expect("valid regex")
});

impl GitVersion {
    pub const NULL: GitVersion = GitVersion::with_type(0, 0, 0, 0, GitVersionType::Null);

    pub const fn new(major: u32, minor: u32, revision: u32, patch_level: u32) -> Self {
        Self::with_type(
            major,
            minor,
            revision,
            patch_level,
            GitVersionType::Undefined,
        )
    }

    pub const fn with_type(
        major: u32,
        minor: u32,
        revision: u32,
        patch_level: u32,
        ty: GitVersionType,
    ) -> Self {
        Self {
            major,
            minor,
            revision,
            patch_level,
            ty,
        }
    }

    /// The minimal supported version (upstream `GitVersion.MIN`).
    pub fn min() -> GitVersion {
        if cfg!(windows) {
            GitVersion::new(2, 19, 2, 0)
        } else {
            GitVersion::new(2, 17, 0, 0)
        }
    }

    /// Parses the output of `git version`.
    pub fn parse(output: &str) -> Result<GitVersion> {
        Self::parse_for_os(output, cfg!(windows))
    }

    /// Same as [`GitVersion::parse`] with an explicit OS, so Windows type detection is testable anywhere.
    pub fn parse_for_os(output: &str, is_windows: bool) -> Result<GitVersion> {
        let trimmed = output.trim();
        if trimmed.is_empty() {
            return Err(GitError::VersionParse(output.to_string()));
        }
        let caps = FORMAT
            .captures(trimmed)
            .ok_or_else(|| GitError::VersionParse(output.to_string()))?;
        let int_group = |i: usize| -> Result<u32> {
            match caps.get(i) {
                None => Ok(0),
                Some(m) => m
                    .as_str()
                    .parse::<u32>()
                    .map_err(|_| GitError::VersionParse(output.to_string())),
            }
        };
        let suffix = caps
            .get(5)
            .map(|m| m.as_str().trim().to_lowercase())
            .unwrap_or_default();
        let ty = if is_windows {
            if suffix.contains("msysgit") || suffix.contains("windows") {
                GitVersionType::Msys
            } else {
                GitVersionType::Cygwin
            }
        } else {
            GitVersionType::Unix
        };
        Ok(GitVersion::with_type(
            int_group(1)?,
            int_group(2)?,
            int_group(3)?,
            int_group(4)?,
            ty,
        ))
    }

    pub fn is_null(&self) -> bool {
        self.ty == GitVersionType::Null
    }

    /// True if the version is supported (not NULL and at least [`GitVersion::min`]).
    pub fn is_supported(&self) -> bool {
        !self.is_null() && self.compare(&GitVersion::min()) != Ordering::Less
    }

    /// Upstream `compareTo`: only numbers are compared. A NULL `other` makes any non-NULL version
    /// Greater; a NULL `self` compares as 0.0.0.0 (so it equals a non-NULL 0.0.0.0, as upstream).
    pub fn compare(&self, other: &GitVersion) -> Ordering {
        if other.is_null() {
            return if self.is_null() {
                Ordering::Equal
            } else {
                Ordering::Greater
            };
        }
        (self.major, self.minor, self.revision, self.patch_level).cmp(&(
            other.major,
            other.minor,
            other.revision,
            other.patch_level,
        ))
    }

    pub fn is_later_or_equal(&self, other: &GitVersion) -> bool {
        self.compare(other) != Ordering::Less
    }

    pub fn is_older_or_equal(&self, other: &GitVersion) -> bool {
        self.compare(other) != Ordering::Greater
    }

    /// e.g. `2.39.1`, or `2.7.3.3` when there is a patch level.
    pub fn presentation(&self) -> String {
        let mut s = format!("{}.{}.{}", self.major, self.minor, self.revision);
        if self.patch_level > 0 {
            s.push_str(&format!(".{}", self.patch_level));
        }
        s
    }
}

/// Upstream `equals`: numbers must match, and types must match unless either is `Undefined`.
/// Like upstream, this is not transitive (`Unix == Undefined == Msys`, but `Unix != Msys`), so
/// don't use `GitVersion` as a map/set key.
impl PartialEq for GitVersion {
    fn eq(&self, other: &Self) -> bool {
        if self.compare(other) != Ordering::Equal {
            return false;
        }
        if self.ty == GitVersionType::Undefined || other.ty == GitVersionType::Undefined {
            return true;
        }
        self.ty == other.ty
    }
}

/// Port of the simple, version-threshold entries of `GitVersionSpecialty.kt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitVersionSpecialty {
    EnvGitTracePackAccessAllowed,
    EnvGitOptionalLocksAllowed,
    StatusSupportsIgnoredModes,
    StatusSupportsNoRenames,
    StartedUsingRawBodyInFormat,
    NoWalkUnsorted,
}

impl GitVersionSpecialty {
    fn minimal_version(self) -> GitVersion {
        match self {
            Self::EnvGitTracePackAccessAllowed => GitVersion::new(2, 1, 0, 0),
            Self::EnvGitOptionalLocksAllowed => GitVersion::new(2, 15, 0, 0),
            Self::StatusSupportsIgnoredModes => GitVersion::new(2, 16, 0, 0),
            Self::StatusSupportsNoRenames => GitVersion::new(2, 18, 0, 0),
            Self::StartedUsingRawBodyInFormat => GitVersion::new(1, 7, 2, 0),
            Self::NoWalkUnsorted => GitVersion::new(1, 7, 12, 1),
        }
    }

    pub fn exists_in(self, version: &GitVersion) -> bool {
        version.is_later_or_equal(&self.minimal_version())
    }
}

#[cfg(test)]
mod tests {
    //! Ported from `plugins/git4idea/tests/git4idea/tests/GitVersionTest.java`.
    use super::*;

    const COMMON: &[(&str, u32, u32, u32, u32)] = &[
        ("git version 1.6.4", 1, 6, 4, 0),
        ("git version 1.7.3.3", 1, 7, 3, 3),
        ("git version 1.7.3.5.gb27be", 1, 7, 3, 5),
        ("git version 1.7.4-rc1", 1, 7, 4, 0),
        ("git version 1.7.7.5 (Apple Git-24)", 1, 7, 7, 5),
        ("git version 1.9.rc0.143.g6fd479e", 1, 9, 0, 0),
    ];

    const MSYS: &[(&str, u32, u32, u32, u32)] = &[
        ("git version 1.6.4.msysgit.0", 1, 6, 4, 0),
        ("git version 1.7.3.3.msysgit.1", 1, 7, 3, 3),
        ("git version 1.7.3.2.msysgit", 1, 7, 3, 2),
        ("git version 1.7.3.5.msysgit.gb27be", 1, 7, 3, 5),
        ("git version 2.19.1.windows.1", 2, 19, 1, 0),
    ];

    fn assert_version(v: GitVersion, expected: (&str, u32, u32, u32, u32), ty: GitVersionType) {
        assert_eq!(
            (v.major, v.minor, v.revision, v.patch_level, v.ty),
            (expected.1, expected.2, expected.3, expected.4, ty),
            "parsing {:?}",
            expected.0
        );
    }

    #[test]
    fn parses_unix_versions() {
        for t in COMMON {
            assert_version(
                GitVersion::parse_for_os(t.0, false).unwrap(),
                *t,
                GitVersionType::Unix,
            );
        }
    }

    #[test]
    fn parses_cygwin_and_msys_on_windows() {
        for t in COMMON {
            assert_version(
                GitVersion::parse_for_os(t.0, true).unwrap(),
                *t,
                GitVersionType::Cygwin,
            );
        }
        for t in MSYS {
            assert_version(
                GitVersion::parse_for_os(t.0, true).unwrap(),
                *t,
                GitVersionType::Msys,
            );
        }
    }

    #[test]
    fn equality_and_ordering() {
        let v1 = GitVersion::parse_for_os("git version 1.6.3", true).unwrap();
        let v2 = GitVersion::parse_for_os("git version 1.7.3", true).unwrap();
        let v3 = GitVersion::parse_for_os("git version 1.7.3", true).unwrap();
        let v4 = GitVersion::parse_for_os("git version 1.7.3.msysgit.0", true).unwrap();
        assert!(v1 != v2);
        assert!(v1 != v3);
        assert!(v2 == v3);
        assert!(v1 != v4);
        assert!(v2 != v4, "cygwin and msys of the same number are not equal");
        assert_eq!(v1.compare(&v2), Ordering::Less);
        assert_eq!(v2.compare(&v3), Ordering::Equal);
        assert_eq!(
            v2.compare(&v4),
            Ordering::Equal,
            "ordering ignores the type"
        );
    }

    #[test]
    fn rejects_garbage_and_handles_null() {
        assert!(GitVersion::parse("").is_err());
        assert!(GitVersion::parse("hello").is_err());
        assert_eq!(
            GitVersion::NULL.compare(&GitVersion::new(0, 0, 0, 1)),
            Ordering::Less
        );
        assert_eq!(
            GitVersion::new(2, 0, 0, 0).compare(&GitVersion::NULL),
            Ordering::Greater
        );
        assert!(!GitVersion::NULL.is_supported());
        assert!(GitVersion::new(2, 40, 0, 0).is_supported());
    }

    #[test]
    fn specialties() {
        let v = GitVersion::new(2, 15, 0, 0);
        assert!(GitVersionSpecialty::EnvGitOptionalLocksAllowed.exists_in(&v));
        assert!(!GitVersionSpecialty::StatusSupportsIgnoredModes.exists_in(&v));
        assert!(!GitVersionSpecialty::StatusSupportsNoRenames.exists_in(&GitVersion::NULL));
    }

    #[test]
    fn raw_body_and_no_walk_unsorted_thresholds() {
        use GitVersionSpecialty::{NoWalkUnsorted, StartedUsingRawBodyInFormat};
        let raw = StartedUsingRawBodyInFormat;
        assert!(!raw.exists_in(&GitVersion::new(1, 7, 1, 9)));
        assert!(!raw.exists_in(&GitVersion::new(1, 6, 9, 9)));
        assert!(raw.exists_in(&GitVersion::new(1, 7, 2, 0)));
        assert!(raw.exists_in(&GitVersion::new(1, 7, 2, 1)));
        assert!(!raw.exists_in(&GitVersion::NULL));

        let nw = NoWalkUnsorted;
        assert!(!nw.exists_in(&GitVersion::new(1, 7, 12, 0)));
        assert!(!nw.exists_in(&GitVersion::new(1, 7, 11, 9)));
        assert!(nw.exists_in(&GitVersion::new(1, 7, 12, 1)));
        assert!(nw.exists_in(&GitVersion::new(1, 7, 13, 0)));
        assert!(!nw.exists_in(&GitVersion::NULL));
    }

    #[test]
    fn presentation() {
        assert_eq!(GitVersion::new(2, 39, 1, 0).presentation(), "2.39.1");
        assert_eq!(GitVersion::new(1, 7, 3, 3).presentation(), "1.7.3.3");
    }
}
