# IdeaGit Plan 1 — Git Core Foundation (`ig-git`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the Cargo workspace and the headless `ig-git` crate: git version detection, the git command runner with Git-console events, `git status` parsing, ref reading, and the repository state reader, all faithfully ported from IntelliJ's `git4idea`.

**Architecture:** `ig-git` is a plain Rust library with no UI dependency. Every git call goes through `GitHandler` (builds the command line and environment exactly like upstream `GitHandler`) and `Git::run_command` (upstream `GitImplBase.doRun`: trace/optional-lock env, per-project write lock, console events for non-silent commands). Repository state is read from `.git` files as upstream `GitRepositoryReader` does, falling back to `git for-each-ref` for reftable repositories. Each module names its upstream source, and `reference/upstream-map.toml` records the mapping for future fix-porting.

**Tech Stack:** Rust 2024 edition (stable ≥ 1.85), `regex`, `thiserror`, `tempfile` (tests). The installed `git` CLI is used at runtime and in integration tests.

**Spec:** `docs/superpowers/specs/2026-11-08-ideagit-rust-design.md`

---

## Where this plan fits

Slice 1 of the spec is split into sequential plans, each producing working, tested software:

1. **Git core foundation (this plan)**: `ig-git` runner, console events, status, refs, repository reader, root detection.
2. Log data + graph: `ig-log` (port of `platform/vcs-log/graph` with upstream graph tests; `git log` parsing).
3. Diff comparison: `ig-diff` (port of `com.intellij.diff.comparison` with upstream tests).
4. Change model + commit backend: `ig-vcs` changelists and line-status tracking; commit and partial commit via a temporary index.
5. App shell: GPUI window, `ig-ui-kit` (theme/icons/strings import via `xtask import-ui`), tool-window docking, actions/keymap, notifications, welcome screen, Git console tool window.
6. Git Log UI.
7. Commit tool window, diff viewer, basic editor.
8. Branches popup, checkout/smart checkout, askpass, fetch/pull/push/update.
9. Distribution, `xtask upstream-watch`, minimum-git CI job.

## Upstream reference checkout

Tasks reference upstream files by path. They live in a local, sparse, blob-less clone **outside** this repository:

```bash
cd ~/Code
git clone --filter=blob:none --sparse --depth 1 --branch 263 https://github.com/JetBrains/intellij-community.git intellij-community
cd intellij-community
git sparse-checkout set plugins/git4idea platform/vcs-log platform/vcs-impl/src/com/intellij/vcs \
  platform/diff-impl/src/com/intellij/diff/comparison platform/diff-impl/tests
git rev-parse HEAD   # the plan was written against b3c1c1958bc81cc96ea882fd4beba72849c1ed98
```

If `HEAD` differs, that's fine: `reference/UPSTREAM` pins `b3c1c19…`. To read pinned files, use `git show b3c1c19:<path>`. That needs a full-depth clone, or `git fetch --depth 1 origin b3c1c1958bc81cc96ea882fd4beba72849c1ed98`.

`IJ` below means that clone's path (`~/Code/intellij-community`).

## Upstream behaviors this plan reproduces (read before coding)

- Every git command gets `-c core.quotepath=false -c log.showSignature=false` before the command name (`GitHandler.getConfigParameters`).
- Environment: inherited process env, then `LANGUAGE=` and `LC_ALL=en_US.UTF-8` (`VcsLocaleHelper`), then custom vars, then the marker `INTELLIJ_GIT_EXECUTABLE=true`. IdeaGit also sets `IDEAGIT_GIT_EXECUTABLE=true`.
- `GIT_TRACE=0`, `GIT_TRACE_PACK_ACCESS=` (git ≥ 2.1), `GIT_TRACE_PACKET=`, `GIT_TRACE_PERFORMANCE=0`, `GIT_TRACE_SETUP=0`; `GIT_OPTIONAL_LOCKS=0` when git ≥ 2.15.
- Locking: only `WRITE` commands (and `READ_OPTIONAL_LOCKING` ones when optional locks can't be suppressed) take the per-project lock; reads never block.
- Console: only **non-silent** handlers are logged. Handlers are silent by default unless the command is `WRITE`; stdout is suppressed in the console by default. Blank lines are not logged.
- Output lines are split on `\n`, `\r\n` and `\r`. Stderr progress lines (`looksLikeProgress`) are dropped from the collected error output.
- `git status --porcelain -z` uses format **v1** (not v2), with `--untracked-files=all` and `--ignored=no|matching` (git ≥ 2.16) / `--no-renames` (git ≥ 2.18).
- Branches are read from `packed-refs` then loose refs (loose wins), symbolic refs resolved, and `origin/HEAD` skipped. HEAD content `ref: refs/heads/.invalid` means reftable, so `git for-each-ref` is used.
- Repository state precedence: MERGING > REBASING > DETACHED > GRAFTING (cherry-pick) > REVERTING > sequencer todo > NORMAL.

## File structure

```
Cargo.toml                         workspace
LICENSE, NOTICE                    Apache-2.0 + attribution
.gitignore, .gitattributes
.github/workflows/ci.yml           fmt + clippy + tests on Linux/macOS/Windows
reference/UPSTREAM                 pinned intellij-community revision
reference/upstream-map.toml        Rust module -> upstream files
crates/ig-git/
  Cargo.toml
  src/lib.rs                       module list
  src/error.rs                     GitError / Result
  src/hash.rs                      Hash (HashImpl.build validation)
  src/version.rs                   GitVersion, GitVersionSpecialty
  src/command.rs                   GitCommand, LockingPolicy
  src/console.rs                   ConsoleEvent, ConsoleSink, RecordingConsole
  src/handler.rs                   GitHandler: command line + environment
  src/runner.rs                    Git (runner), GitCommandResult, line splitting, progress filter
  src/status.rs                    git status porcelain v1 parsing + invocation
  src/repo_files.rs                GitRepositoryFiles, find_git_dir, try_load_file
  src/refs.rs                      packed/loose refs, symbolic resolution, ref-name validation
  src/config.rs                    git config values + remotes (git config --null -l)
  src/branch.rs                    GitLocalBranch, GitRemoteBranch, parse_remote_branch
  src/reader.rs                    GitRepositoryReader -> GitBranchState
  src/roots.rs                     detect_roots
  tests/common/mod.rs              real-git test helpers
  tests/runner_integration.rs
  tests/status_integration.rs
  tests/config_integration.rs
  tests/reader_fixtures.rs         ported GitRepositoryReaderTest
  tests/reader_integration.rs
  tests/fixtures/repo/{example1,symbolic-refs}   upstream test data
```

---
### Task 1: Workspace scaffold, licensing, upstream pin, CI

**Files:**
- Create: `Cargo.toml`, `crates/ig-git/Cargo.toml`, `crates/ig-git/src/lib.rs`, `crates/ig-git/src/error.rs`
- Create: `.gitignore`, `.gitattributes`, `LICENSE`, `NOTICE`, `reference/UPSTREAM`, `reference/upstream-map.toml`, `.github/workflows/ci.yml`

- [ ] **Step 1: Create the workspace manifest** `Cargo.toml`

```toml
[workspace]
resolver = "3"
members = ["crates/ig-git"]

[workspace.package]
edition = "2024"
license = "Apache-2.0"
rust-version = "1.85"

[workspace.dependencies]
regex = "1"
thiserror = "2"
tempfile = "3"
```

- [ ] **Step 2: Create the crate manifest** `crates/ig-git/Cargo.toml`

```toml
[package]
name = "ig-git"
version = "0.1.0"
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[dependencies]
regex.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 3: Create `crates/ig-git/src/error.rs`**

```rust
//! Error type for the git core. Mirrors upstream's use of `VcsException` / `RepoStateException`.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    /// A git command failed. The message is the presentable error output (see `GitCommandResult`).
    #[error("{0}")]
    Command(String),
    #[error("unsupported format of git --version output: {0:?}")]
    VersionParse(String),
    #[error("{message} (X={x:?}, Y={y:?}, line={line:?})")]
    StatusParse {
        message: String,
        x: char,
        y: char,
        line: String,
    },
    #[error("repository state error: {0}")]
    RepoState(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, GitError>;
```

- [ ] **Step 4: Create `crates/ig-git/src/lib.rs`**

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod error;
```

- [ ] **Step 5: Create `.gitignore` and `.gitattributes`**

`.gitignore`:
```
/target
```

`.gitattributes`:
```
# Upstream test data must stay byte-exact (no CRLF conversion on Windows).
crates/ig-git/tests/fixtures/** -text
```

- [ ] **Step 6: Add `LICENSE` (Apache-2.0 text) and `NOTICE`**

Upstream's root `LICENSE.txt` is **not** the Apache text: since June 2026 it holds the "JetBrains Open-Source Build Terms", which cover JetBrains' binary builds. The source files are Apache-2.0 (see their headers), so copy the Apache-2.0 text that upstream ships:

```bash
git -C "$IJ" show HEAD:plugins/hg4idea/resources/META-INF/LICENSE-2.0.txt > LICENSE
head -2 LICENSE   # expected: "Apache License" / "Version 2.0, January 2004"
```

`NOTICE`:
```
IdeaGit
Copyright 2026 The IdeaGit contributors

This product includes software ported from IntelliJ IDEA Community Edition
(https://github.com/JetBrains/intellij-community),
Copyright 2000-2026 JetBrains s.r.o. and contributors,
licensed under the Apache License, Version 2.0.

Each ported Rust module names its upstream source files in its module documentation;
reference/upstream-map.toml lists all of them together with the upstream commit they match.

The NOTICE file of intellij-community reads:

    This software includes code from IntelliJ IDEA
    Copyright (C) JetBrains s.r.o.
    https://www.jetbrains.com/idea/

Test data under crates/ig-git/tests/fixtures/repo is copied unchanged from
intellij-community/plugins/git4idea/testData/repo.
```

- [ ] **Step 7: Create `reference/UPSTREAM`**

```toml
# The intellij-community revision IdeaGit is ported from.
repository = "https://github.com/JetBrains/intellij-community"
branch = "263"
commit = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
```

- [ ] **Step 8: Create `reference/upstream-map.toml`** (entries for every module this plan adds)

```toml
# Rust module -> intellij-community files it was ported from.
# pinned:     upstream commit this module last matched (see reference/UPSTREAM)
# tests:      upstream tests ported alongside
# deviations: intentional differences from upstream

[modules."ig-git::command"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/commands/GitCommand.java"]

[modules."ig-git::handler"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/commands/GitHandler.java",
  "plugins/git4idea/backend/src/commands/GitLineHandler.java",
  "platform/vcs-impl/src/com/intellij/vcs/VcsLocaleHelper.kt",
]
deviations = [
  "No WSL/Eel executables, no Windows .cmd escaping, no VcsEnvCustomizer extensions",
  "Also sets IDEAGIT_GIT_EXECUTABLE=true next to INTELLIJ_GIT_EXECUTABLE=true",
]

[modules."ig-git::runner"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/commands/GitImplBase.java", "plugins/git4idea/backend/src/commands/GitCommandResult.java"]
deviations = [
  "Synchronous std::process execution on the calling thread; callers use a background executor",
  "No root-name prefix in error messages yet (multi-root presentation comes with the UI)",
]

[modules."ig-git::console"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/commands/GitImplBase.java#GitCommandOutputLogger"]

[modules."ig-git::version"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/config/GitVersion.java", "plugins/git4idea/backend/src/config/GitVersionSpecialty.kt"]
tests = ["plugins/git4idea/tests/git4idea/tests/GitVersionTest.java"]
deviations = ["WSL1/WSL2 types not ported", "Only the specialties used so far are ported"]

[modules."ig-git::status"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/index/GitIndexStatusUtil.kt"]
deviations = ["Long path lists are not split into chunks (GitUtil.createHandlerWithPaths)"]

[modules."ig-git::hash"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-impl/shared/src/com/intellij/vcs/log/impl/HashImpl.java"]

[modules."ig-git::refs"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/repo/GitRefUtil.java",
  "plugins/git4idea/backend/src/validators/GitRefNameValidator.java",
  "plugins/git4idea/shared/src/com/intellij/vcs/git/ref/GitRefUtil.kt",
]

[modules."ig-git::repo_files"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/repo/GitRepositoryFiles.java",
  "plugins/git4idea/backend/src/GitUtil.java#findGitDir",
  "platform/dvcs-impl/src/com/intellij/dvcs/DvcsUtil.java#tryLoadFile",
]

[modules."ig-git::config"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/config/GitConfigUtil.java#getValues",
  "plugins/git4idea/backend/src/repo/GitConfig.kt#read",
]
deviations = [
  "Remotes only; no url.insteadOf substitution, branch tracking, hooks or core settings yet",
  "Valueless keys (`key\\0`) parse as `true`; upstream's parser merges them into the next record",
]

[modules."ig-git::branch"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/GitLocalBranch.kt",
  "plugins/git4idea/shared/src/git4idea/GitBranch.kt",
  "plugins/git4idea/shared/src/git4idea/GitStandardRemoteBranch.kt",
  "plugins/git4idea/shared/src/git4idea/GitSvnRemoteBranch.kt",
  "plugins/git4idea/backend/src/GitUtil.java#parseRemoteBranch",
]
deviations = ["Branch names compare case-sensitively (upstream: case-insensitive on case-insensitive file systems)"]

[modules."ig-git::reader"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/repo/GitRepositoryReader.kt"]
tests = ["plugins/git4idea/tests/git4idea/repo/GitRepositoryReaderTest.kt", "plugins/git4idea/testData/repo"]
deviations = ["Hooks info and shallow detection not yet ported"]

[modules."ig-git::roots"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-impl/src/com/intellij/openapi/vcs/roots/VcsRootDetectorImpl.java"]
deviations = ["Simplified: enclosing root + bounded downward scan with a fixed skip list"]
```

- [ ] **Step 9: Create `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  test:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - run: cargo fmt --all --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test --all
```

- [ ] **Step 10: Build**

Run: `cargo build`
Expected: `Finished` with no errors. `Cargo.lock` is created; commit it.

- [ ] **Step 11: Commit**

```bash
git add Cargo.toml Cargo.lock crates .gitignore .gitattributes LICENSE NOTICE reference .github
git commit -m "chore: scaffold Cargo workspace and ig-git crate"
```

---
### Task 2: Commit hash value

**Files:**
- Create: `crates/ig-git/src/hash.rs`
- Modify: `crates/ig-git/src/lib.rs`

Upstream: `platform/vcs-impl/shared/src/com/intellij/vcs/log/impl/HashImpl.java` (`build` accepts any non-empty hex string), used through `GitRefUtil.parseHash`.

- [ ] **Step 1: Write the failing tests**: create `crates/ig-git/src/hash.rs` containing only the test module

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_hash_and_lowercases() {
        let h = Hash::parse("0E1D130689BC52F140C5C374AA9CC2B8916C0AD7").unwrap();
        assert_eq!(h.as_str(), "0e1d130689bc52f140c5c374aa9cc2b8916c0ad7");
    }

    #[test]
    fn rejects_symbolic_ref_and_empty() {
        assert!(Hash::parse("ref: refs/heads/master").is_none());
        assert!(Hash::parse("").is_none());
    }
}
```

and register the module in `crates/ig-git/src/lib.rs`:

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod error;
pub mod hash;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ig-git --lib hash::`
Expected: FAIL. Compilation errors for the not-yet-defined items (`cannot find ...` / `unresolved import`).

- [ ] **Step 3: Implement**: insert this **above** the `#[cfg(test)]` module in `crates/ig-git/src/hash.rs`

```rust
//! Commit hash value. Port of the validation in `HashImpl.build` as used by `GitRefUtil.parseHash`:
//! any non-empty hexadecimal string is accepted (SHA-1 and SHA-256 alike).

use std::fmt;

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash(String);

impl Hash {
    /// Returns `None` if `value` is empty or contains a non-hex character (e.g. `ref: refs/heads/main`).
    pub fn parse(value: &str) -> Option<Hash> {
        if !value.is_empty() && value.bytes().all(|b| b.is_ascii_hexdigit()) {
            Some(Hash(value.to_ascii_lowercase()))
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", self.0)
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ig-git --lib hash::`
Expected: PASS (all tests in `hash::tests`).

- [ ] **Step 5: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): hash (Commit hash value)"
```

---
### Task 3: Git version parsing and version specialties

**Files:**
- Create: `crates/ig-git/src/version.rs`
- Modify: `crates/ig-git/src/lib.rs`

Upstream: `$IJ/plugins/git4idea/backend/src/config/GitVersion.java`, `GitVersionSpecialty.kt`. The tests are ported from `plugins/git4idea/tests/git4idea/tests/GitVersionTest.java`. Note the upstream quirk: ordering ignores the type, but equality does not.

- [ ] **Step 1: Write the failing tests**: create `crates/ig-git/src/version.rs` containing only the test module

```rust
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
    fn presentation() {
        assert_eq!(GitVersion::new(2, 39, 1, 0).presentation(), "2.39.1");
        assert_eq!(GitVersion::new(1, 7, 3, 3).presentation(), "1.7.3.3");
    }
}
```

and register the module in `crates/ig-git/src/lib.rs`:

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod error;
pub mod hash;
pub mod version;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ig-git --lib version::`
Expected: FAIL. Compilation errors for the not-yet-defined items (`cannot find ...` / `unresolved import`).

- [ ] **Step 3: Implement**: insert this **above** the `#[cfg(test)]` module in `crates/ig-git/src/version.rs`

```rust
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

    /// Upstream `compareTo`: only numbers are compared; NULL is less than any non-NULL version.
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
}

impl GitVersionSpecialty {
    fn minimal_version(self) -> GitVersion {
        match self {
            Self::EnvGitTracePackAccessAllowed => GitVersion::new(2, 1, 0, 0),
            Self::EnvGitOptionalLocksAllowed => GitVersion::new(2, 15, 0, 0),
            Self::StatusSupportsIgnoredModes => GitVersion::new(2, 16, 0, 0),
            Self::StatusSupportsNoRenames => GitVersion::new(2, 18, 0, 0),
        }
    }

    pub fn exists_in(self, version: &GitVersion) -> bool {
        version.is_later_or_equal(&self.minimal_version())
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ig-git --lib version::`
Expected: PASS (all tests in `version::tests`).

- [ ] **Step 5: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): version (Git version parsing and version specialties)"
```

---
### Task 4: Git command catalogue and locking policy

**Files:**
- Create: `crates/ig-git/src/command.rs`
- Modify: `crates/ig-git/src/lib.rs`

Upstream: `$IJ/plugins/git4idea/backend/src/commands/GitCommand.java`. Keep the comments explaining why `FETCH`, `PUSH` and `CLONE` are read commands.

- [ ] **Step 1: Write the failing tests**: create `crates/ig-git/src/command.rs` containing only the test module

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policies_match_upstream() {
        assert_eq!(GitCommand::COMMIT.locking_policy(), LockingPolicy::Write);
        assert_eq!(GitCommand::FETCH.locking_policy(), LockingPolicy::Read);
        assert_eq!(
            GitCommand::STATUS.locking_policy(),
            LockingPolicy::ReadOptionalLocking
        );
        assert_eq!(GitCommand::STATUS.name(), "status");
        assert_eq!(
            GitCommand::FETCH.write_locking_command().locking_policy(),
            LockingPolicy::Write
        );
    }
}
```

and register the module in `crates/ig-git/src/lib.rs`:

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod command;
pub mod error;
pub mod hash;
pub mod version;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ig-git --lib command::`
Expected: FAIL. Compilation errors for the not-yet-defined items (`cannot find ...` / `unresolved import`).

- [ ] **Step 3: Implement**: insert this **above** the `#[cfg(test)]` module in `crates/ig-git/src/command.rs`

```rust
//! Port of `plugins/git4idea/backend/src/commands/GitCommand.java`.

/// How a command interacts with the per-project command lock (upstream `GitCommand.LockingPolicy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockingPolicy {
    Read,
    /// Commands with non-mandatory write side effects (e.g. `git status`) that can run without
    /// locking the index when `GIT_OPTIONAL_LOCKS=0` is supported.
    ReadOptionalLocking,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitCommand {
    name: &'static str,
    locking: LockingPolicy,
}

impl GitCommand {
    const fn read(name: &'static str) -> Self {
        Self {
            name,
            locking: LockingPolicy::Read,
        }
    }
    const fn read_optional(name: &'static str) -> Self {
        Self {
            name,
            locking: LockingPolicy::ReadOptionalLocking,
        }
    }
    const fn write(name: &'static str) -> Self {
        Self {
            name,
            locking: LockingPolicy::Write,
        }
    }

    pub const ADD: Self = Self::write("add");
    pub const BLAME: Self = Self::read("blame");
    pub const BRANCH: Self = Self::read("branch");
    pub const FOR_EACH_REF: Self = Self::read("for-each-ref");
    pub const CAT_FILE: Self = Self::read("cat-file");
    pub const CHECKOUT: Self = Self::write("checkout");
    pub const CHECK_ATTR: Self = Self::read("check-attr");
    pub const CHECK_IGNORE: Self = Self::read("check-ignore");
    pub const COMMIT: Self = Self::write("commit");
    pub const COMMIT_TREE: Self = Self::write("commit-tree");
    pub const CONFIG: Self = Self::read("config");
    pub const CHERRY: Self = Self::read("cherry");
    pub const CHERRY_PICK: Self = Self::write("cherry-pick");
    /// Write, but can't interfere with any other command, so treated as read (as upstream).
    pub const CLONE: Self = Self::read("clone");
    pub const DIFF: Self = Self::read("diff");
    /// Read: fetch doesn't modify the index.
    pub const FETCH: Self = Self::read("fetch");
    pub const INIT: Self = Self::write("init");
    pub const LOG: Self = Self::read("log");
    pub const SHORTLOG: Self = Self::read("shortlog");
    pub const LS_FILES: Self = Self::read_optional("ls-files");
    pub const LS_TREE: Self = Self::read("ls-tree");
    pub const LS_REMOTE: Self = Self::read("ls-remote");
    pub const MERGE: Self = Self::write("merge");
    pub const MERGE_BASE: Self = Self::read("merge-base");
    pub const MERGE_TREE: Self = Self::read("merge-tree");
    pub const MV: Self = Self::write("mv");
    pub const NOTES: Self = Self::read("notes");
    pub const PULL: Self = Self::write("pull");
    /// Read: push doesn't modify the index.
    pub const PUSH: Self = Self::read("push");
    pub const REBASE: Self = Self::write("rebase");
    pub const REMOTE: Self = Self::read("remote");
    pub const RESET: Self = Self::write("reset");
    pub const RESTORE: Self = Self::write("restore");
    pub const REVERT: Self = Self::write("revert");
    pub const REV_LIST: Self = Self::read("rev-list");
    pub const REV_PARSE: Self = Self::read("rev-parse");
    pub const REF_LOG: Self = Self::read("reflog");
    pub const RM: Self = Self::write("rm");
    pub const SHOW: Self = Self::read("show");
    pub const STASH: Self = Self::write("stash");
    pub const STATUS: Self = Self::read_optional("status");
    pub const SUBMODULE: Self = Self::write("submodule");
    pub const TAG: Self = Self::read("tag");
    pub const UPDATE_INDEX: Self = Self::write("update-index");
    pub const UPDATE_REF: Self = Self::write("update-ref");
    pub const HASH_OBJECT: Self = Self::write("hash-object");
    pub const HOOK: Self = Self::write("hook");
    pub const VERSION: Self = Self::read("version");
    pub const WORKTREE: Self = Self::read("worktree");

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn locking_policy(&self) -> LockingPolicy {
        self.locking
    }

    /// Same command, but forced to read locking (upstream `readLockingCommand`).
    pub fn read_locking_command(self) -> Self {
        Self {
            locking: LockingPolicy::Read,
            ..self
        }
    }

    /// Same command, but forced to write locking (upstream `writeLockingCommand`).
    pub fn write_locking_command(self) -> Self {
        Self {
            locking: LockingPolicy::Write,
            ..self
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ig-git --lib command::`
Expected: PASS (all tests in `command::tests`).

- [ ] **Step 5: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): command (Git command catalogue and locking policy)"
```

---
### Task 5: Console events and the command handler

**Files:**
- Create: `crates/ig-git/src/console.rs`, `crates/ig-git/src/handler.rs`
- Modify: `crates/ig-git/src/lib.rs`

Upstream: `GitHandler.java` (constructor, `getConfigParameters`, `prepareEnvironment`, `printableCommandLine`), `GitLineHandler.java`, `platform/vcs-impl/src/com/intellij/vcs/VcsLocaleHelper.kt`. The console event shape mirrors `GitCommandOutputLogger` in `GitImplBase.java` (`showCommandStart`, `showCommandOutput`, `showCommandFinished`).

- [ ] **Step 1: Create `crates/ig-git/src/console.rs`** (plain data types, no logic to test)

```rust
//! Git console event stream. Mirrors what upstream's `GitCommandOutputLogger` sends to
//! `GitCommandOutputPrinter`: command start, output lines, and termination.

use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleEvent {
    Started {
        id: u64,
        working_dir: PathBuf,
        command_line: String,
    },
    Output {
        id: u64,
        kind: OutputKind,
        line: String,
    },
    Finished {
        id: u64,
        exit_code: i32,
    },
}

/// Receives console events. Implementations must be cheap and non-blocking: they are called from
/// the thread that runs the git process.
pub trait ConsoleSink: Send + Sync {
    fn event(&self, event: ConsoleEvent);
}

/// Discards all events.
pub struct NullConsole;

impl ConsoleSink for NullConsole {
    fn event(&self, _event: ConsoleEvent) {}
}

/// Stores all events in memory; used by tests and as a simple buffer for the UI.
#[derive(Default)]
pub struct RecordingConsole {
    events: Mutex<Vec<ConsoleEvent>>,
}

impl RecordingConsole {
    pub fn events(&self) -> Vec<ConsoleEvent> {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl ConsoleSink for RecordingConsole {
    fn event(&self, event: ConsoleEvent) {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(event);
    }
}
```

- [ ] **Step 2: Write the failing handler tests**: create `crates/ig-git/src/handler.rs` containing only

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_default_parameters_and_silence() {
        let mut h = GitHandler::new("/repo", "git", GitCommand::STATUS);
        h.add_parameters(["--porcelain", "-z"]);
        assert_eq!(
            h.parameters(),
            [
                "-c",
                "core.quotepath=false",
                "-c",
                "log.showSignature=false",
                "status",
                "--porcelain",
                "-z"
            ]
        );
        assert!(h.is_silent(), "read commands are silent by default");
        assert!(!GitHandler::new("/repo", "git", GitCommand::COMMIT).is_silent());
        assert!(h.is_stdout_suppressed());
    }

    #[test]
    fn extra_config_and_printable_line() {
        let mut h = GitHandler::with_config("/repo", "git", GitCommand::COMMIT, &["user.name=A B"]);
        h.add_parameters(["-m", "two words"]);
        assert_eq!(
            h.printable_command_line(),
            "git -c core.quotepath=false -c log.showSignature=false -c \"user.name=A B\" commit -m \"two words\""
        );
    }

    #[test]
    fn environment_order_and_override() {
        let mut h = GitHandler::new("/repo", "git", GitCommand::STATUS);
        h.add_custom_environment_variable("GIT_OPTIONAL_LOCKS", "1");
        h.add_custom_environment_variable("GIT_OPTIONAL_LOCKS", "0");
        let env = h.environment();
        assert_eq!(env[0], ("LANGUAGE".into(), "".into()));
        assert_eq!(env[1], ("LC_ALL".into(), "en_US.UTF-8".into()));
        assert_eq!(
            env.iter()
                .filter(|(k, _)| k == "GIT_OPTIONAL_LOCKS")
                .count(),
            1
        );
        assert!(env.contains(&("GIT_OPTIONAL_LOCKS".into(), "0".into())));
        assert!(env.contains(&(IJ_HANDLER_MARKER_ENV.into(), "true".into())));
    }

    #[test]
    fn relative_paths() {
        let base = Path::new("/repo");
        assert_eq!(relative_path(base, Path::new("/repo/a/b.txt")), "a/b.txt");
        assert_eq!(relative_path(base, Path::new("/repo")), ".");
        assert_eq!(relative_path(base, Path::new("/other/x")), "/other/x");
    }
}
```

and update `crates/ig-git/src/lib.rs`:

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod command;
pub mod console;
pub mod error;
pub mod handler;
pub mod hash;
pub mod version;
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p ig-git --lib handler::`
Expected: FAIL. Compilation errors (`GitHandler`, `relative_path` and others not found).

- [ ] **Step 4: Implement**: insert above the test module in `handler.rs`

```rust
//! Port of the command-line and environment parts of
//! `plugins/git4idea/backend/src/commands/GitHandler.java` and `GitLineHandler.java`.
//!
//! Not ported: WSL/Eel executables, Windows `.cmd` parameter escaping, `VcsEnvCustomizer` extensions.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::command::{GitCommand, LockingPolicy};

/// Marker env var that lets git hooks detect IdeaGit.
pub const IDEAGIT_HANDLER_MARKER_ENV: &str = "IDEAGIT_GIT_EXECUTABLE";
/// Upstream's marker (`GitCommand.IJ_HANDLER_MARKER_ENV`), also set so hooks written for IntelliJ
/// behave the same way under IdeaGit.
pub const IJ_HANDLER_MARKER_ENV: &str = "INTELLIJ_GIT_EXECUTABLE";

/// Port of `VcsLocaleHelper.getDefaultLocaleEnvironmentVars("git")`.
pub fn locale_environment() -> [(&'static str, &'static str); 2] {
    [("LANGUAGE", ""), ("LC_ALL", "en_US.UTF-8")]
}

#[derive(Debug, Clone)]
pub struct GitHandler {
    executable: PathBuf,
    working_dir: PathBuf,
    command: GitCommand,
    /// Everything after the executable: `-c` pairs, the command name, then its arguments.
    parameters: Vec<String>,
    custom_env: Vec<(String, String)>,
    silent: bool,
    stdout_suppressed: bool,
    stderr_suppressed: bool,
}

impl GitHandler {
    pub fn new(
        working_dir: impl Into<PathBuf>,
        executable: impl Into<PathBuf>,
        command: GitCommand,
    ) -> Self {
        Self::with_config(working_dir, executable, command, &[])
    }

    /// `config_parameters` are passed as `-c <param>` before the command name, after the defaults.
    pub fn with_config(
        working_dir: impl Into<PathBuf>,
        executable: impl Into<PathBuf>,
        command: GitCommand,
        config_parameters: &[&str],
    ) -> Self {
        let mut parameters = Vec::new();
        for p in default_config_parameters(config_parameters) {
            parameters.push("-c".to_string());
            parameters.push(p);
        }
        parameters.push(command.name().to_string());
        Self {
            executable: executable.into(),
            working_dir: working_dir.into(),
            command,
            parameters,
            custom_env: Vec::new(),
            // Upstream: non-write commands are not logged to the console by default.
            silent: command.locking_policy() != LockingPolicy::Write,
            stdout_suppressed: true,
            stderr_suppressed: false,
        }
    }

    pub fn command(&self) -> GitCommand {
        self.command
    }

    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    pub fn parameters(&self) -> &[String] {
        &self.parameters
    }

    pub fn add_parameters<I, S>(&mut self, params: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.parameters.extend(params.into_iter().map(Into::into));
    }

    /// Adds `--`, separating options from paths.
    pub fn end_options(&mut self) {
        self.parameters.push("--".to_string());
    }

    /// Adds paths relative to the working directory, with `/` separators.
    pub fn add_relative_paths<P: AsRef<Path>>(&mut self, paths: &[P]) {
        for p in paths {
            let rel = relative_path(&self.working_dir, p.as_ref());
            self.parameters.push(rel);
        }
    }

    pub fn set_silent(&mut self, silent: bool) {
        self.silent = silent;
    }
    pub fn is_silent(&self) -> bool {
        self.silent
    }
    pub fn set_stdout_suppressed(&mut self, suppressed: bool) {
        self.stdout_suppressed = suppressed;
    }
    pub fn is_stdout_suppressed(&self) -> bool {
        self.stdout_suppressed
    }
    pub fn set_stderr_suppressed(&mut self, suppressed: bool) {
        self.stderr_suppressed = suppressed;
    }
    pub fn is_stderr_suppressed(&self) -> bool {
        self.stderr_suppressed
    }

    /// Adds or replaces an environment variable for this command.
    pub fn add_custom_environment_variable(&mut self, name: &str, value: &str) {
        if let Some(entry) = self.custom_env.iter_mut().find(|(k, _)| k == name) {
            entry.1 = value.to_string();
        } else {
            self.custom_env.push((name.to_string(), value.to_string()));
        }
    }

    /// The environment applied on top of the inherited process environment, in application order
    /// (upstream `prepareEnvironment`: locale, then custom variables, then the marker).
    pub fn environment(&self) -> Vec<(String, String)> {
        let mut env: Vec<(String, String)> = locale_environment()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        env.extend(self.custom_env.iter().cloned());
        env.push((IJ_HANDLER_MARKER_ENV.to_string(), "true".to_string()));
        env.push((IDEAGIT_HANDLER_MARKER_ENV.to_string(), "true".to_string()));
        env
    }

    /// The command line as shown in the Git console, e.g. `git -c core.quotepath=false ... status`.
    pub fn printable_command_line(&self) -> String {
        std::iter::once("git".to_string())
            .chain(self.parameters.iter().map(|p| quote_if_needed(p)))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn build_command(&self) -> Command {
        let mut cmd = Command::new(&self.executable);
        cmd.args(&self.parameters).current_dir(&self.working_dir);
        for (k, v) in self.environment() {
            cmd.env(k, v);
        }
        cmd
    }
}

fn default_config_parameters(requested: &[&str]) -> Vec<String> {
    let mut params = vec![
        "core.quotepath=false".to_string(),
        "log.showSignature=false".to_string(),
    ];
    params.extend(requested.iter().map(|s| s.to_string()));
    params
}

fn quote_if_needed(p: &str) -> String {
    if p.is_empty() || p.chars().any(char::is_whitespace) {
        format!("\"{}\"", p.replace('"', "\\\""))
    } else {
        p.to_string()
    }
}

/// Port of `VcsFileUtil.relativePath`: `/`-separated path relative to `base`, `.` for `base` itself.
/// Paths outside `base` are passed through unchanged.
pub(crate) fn relative_path(base: &Path, path: &Path) -> String {
    match path.strip_prefix(base) {
        Ok(rel) if rel.as_os_str().is_empty() => ".".to_string(),
        Ok(rel) => rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p ig-git --lib handler::`
Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): console events and GitHandler command line/environment"
```

---
### Task 6: Command runner, result, and console logging

**Files:**
- Create: `crates/ig-git/src/runner.rs`
- Create: `crates/ig-git/tests/common/mod.rs`, `crates/ig-git/tests/runner_integration.rs`
- Modify: `crates/ig-git/src/lib.rs`

Upstream: `GitImplBase.java` (`doRun`, `getGitTraceEnvironmentVariables`, `lock`/`shouldTakeWriteLock`, `writeOutputToConsole`, `GitCommandResultListener`, `looksLikeProgress`), and `GitCommandResult.java` (`success`, `getErrorOrStdOutput`, `getOutputOrThrow`, `throwOnError`).

- [ ] **Step 1: Write the failing unit tests**: create `crates/ig-git/src/runner.rs` containing only

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn split(chunks: &[&[u8]]) -> Vec<String> {
        let mut s = LineSplitter::default();
        let mut lines = Vec::new();
        for c in chunks {
            s.feed(c, &mut |l| lines.push(l));
        }
        s.finish(&mut |l| lines.push(l));
        lines
    }

    #[test]
    fn splits_lf_crlf_cr_across_chunks() {
        assert_eq!(split(&[b"a\nb\r\nc\rd"]), ["a", "b", "c", "d"]);
        assert_eq!(split(&[b"a\r", b"\nb\n"]), ["a", "b"]);
        assert_eq!(split(&[b"\n\n"]), ["", ""]);
    }

    #[test]
    fn progress_detection() {
        assert!(looks_like_progress(
            "remote: Finding sources:   1% (575/57489)   "
        ));
        assert!(looks_like_progress("Receiving objects:  45% (450/1000)"));
        assert!(looks_like_progress(
            "remote: Counting objects: 198285, done"
        ));
        assert!(looks_like_progress(
            "Expanding reachable commits in commit graph: 95907"
        ));
        assert!(!looks_like_progress("fatal: not a git repository"));
    }

    #[test]
    fn write_lock_rules() {
        assert!(should_take_write_lock(GitCommand::COMMIT, true));
        assert!(!should_take_write_lock(GitCommand::LOG, false));
        assert!(!should_take_write_lock(GitCommand::STATUS, true));
        assert!(should_take_write_lock(GitCommand::STATUS, false));
    }

    #[test]
    fn result_error_presentation() {
        let failed_silently = GitCommandResult {
            start_failed: false,
            exit_code: 128,
            output: vec![],
            error_output: vec![],
        };
        assert_eq!(
            failed_silently.error_output_as_joined_string(),
            "The Git process exited with the code 128"
        );
        assert!(failed_silently.output_or_error().is_err());
        let ok = GitCommandResult {
            exit_code: 0,
            output: vec!["x".into()],
            ..failed_silently
        };
        assert_eq!(ok.output_or_error().unwrap(), "x");
        assert!(
            GitCommandResult {
                exit_code: 1,
                ..ok.clone()
            }
            .success_ignoring(&[1])
        );
    }
}
```

and update `crates/ig-git/src/lib.rs`:

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod command;
pub mod console;
pub mod error;
pub mod handler;
pub mod hash;
pub mod runner;
pub mod version;
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ig-git --lib runner::`
Expected: FAIL. Compilation errors (`LineSplitter`, `looks_like_progress` and others not found).

- [ ] **Step 3: Implement**: insert above the test module in `runner.rs`

```rust
//! Port of the execution parts of `plugins/git4idea/backend/src/commands/GitImplBase.java`
//! (environment, write locking, console logging, output collection) and `GitCommandResult.java`.
//!
//! Commands run synchronously on the calling thread; callers (UI) run them on a background executor.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use crate::command::{GitCommand, LockingPolicy};
use crate::console::{ConsoleEvent, ConsoleSink, OutputKind};
use crate::error::{GitError, Result};
use crate::handler::GitHandler;
use crate::version::{GitVersion, GitVersionSpecialty};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommandResult {
    pub start_failed: bool,
    pub exit_code: i32,
    /// Stdout lines, separators trimmed.
    pub output: Vec<String>,
    /// Stderr lines, separators trimmed, progress lines removed.
    pub error_output: Vec<String>,
}

impl GitCommandResult {
    pub fn success(&self) -> bool {
        self.success_ignoring(&[])
    }

    /// `ignored_error_codes` are non-zero exit codes that count as success.
    pub fn success_ignoring(&self, ignored_error_codes: &[i32]) -> bool {
        !self.start_failed && (self.exit_code == 0 || ignored_error_codes.contains(&self.exit_code))
    }

    pub fn output_as_joined_string(&self) -> String {
        self.output.join("\n")
    }

    /// Upstream `getErrorOrStdOutput`: stderr, else stdout on failure, else a generic exit message.
    pub fn presentable_error_output(&self) -> Vec<String> {
        if !self.error_output.is_empty() {
            return self.error_output.clone();
        }
        if self.success() {
            return Vec::new();
        }
        if !self.output.is_empty() {
            return self.output.clone();
        }
        vec![format!(
            "The Git process exited with the code {}",
            self.exit_code
        )]
    }

    pub fn error_output_as_joined_string(&self) -> String {
        self.presentable_error_output().join("\n")
    }

    /// Upstream `getOutputOrThrow`.
    pub fn output_or_error(&self) -> Result<String> {
        self.throw_on_error()?;
        Ok(self.output_as_joined_string())
    }

    /// Upstream `throwOnError`.
    pub fn throw_on_error(&self) -> Result<()> {
        if self.success() {
            Ok(())
        } else {
            Err(GitError::Command(self.error_output_as_joined_string()))
        }
    }
}

/// Runs git commands for one project. Holds the per-project command lock (upstream
/// `GitVcs.getCommandLock`) that serializes write commands.
pub struct Git {
    executable: PathBuf,
    version: GitVersion,
    console: Arc<dyn ConsoleSink>,
    command_lock: Mutex<()>,
    next_id: AtomicU64,
}

impl Git {
    pub fn new(
        executable: impl Into<PathBuf>,
        version: GitVersion,
        console: Arc<dyn ConsoleSink>,
    ) -> Self {
        Self {
            executable: executable.into(),
            version,
            console,
            command_lock: Mutex::new(()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Runs `git version` with `executable` and returns a runner configured with the detected version.
    pub fn detect(executable: impl Into<PathBuf>, console: Arc<dyn ConsoleSink>) -> Result<Git> {
        let executable = executable.into();
        let probe = Git::new(executable.clone(), GitVersion::NULL, console.clone());
        let cwd = std::env::current_dir()?;
        let result = probe.run_command(probe.handler(&cwd, GitCommand::VERSION));
        let output = result.output_or_error()?;
        let version = GitVersion::parse(&output)?;
        Ok(Git::new(executable, version, console))
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn version(&self) -> GitVersion {
        self.version
    }

    /// Creates a handler for this executable.
    pub fn handler(&self, working_dir: &Path, command: GitCommand) -> GitHandler {
        GitHandler::new(working_dir, &self.executable, command)
    }

    /// Upstream `GitImplBase.doRun`.
    pub fn run_command(&self, mut handler: GitHandler) -> GitCommandResult {
        for (k, v) in trace_environment(&self.version) {
            handler.add_custom_environment_variable(k, v);
        }
        let can_suppress_optional_locks =
            GitVersionSpecialty::EnvGitOptionalLocksAllowed.exists_in(&self.version);
        if can_suppress_optional_locks {
            handler.add_custom_environment_variable("GIT_OPTIONAL_LOCKS", "0");
        }
        let _guard = if should_take_write_lock(handler.command(), can_suppress_optional_locks) {
            Some(self.command_lock.lock().unwrap_or_else(|e| e.into_inner()))
        } else {
            None
        };
        self.execute(&handler)
    }

    fn execute(&self, handler: &GitHandler) -> GitCommandResult {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let log_to_console = !handler.is_silent();

        let mut cmd = handler.build_command();
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                return GitCommandResult {
                    start_failed: true,
                    exit_code: -1,
                    output: Vec::new(),
                    error_output: vec![format!("Failed to start Git process: {e}")],
                };
            }
        };
        if log_to_console {
            self.console.event(ConsoleEvent::Started {
                id,
                working_dir: handler.working_dir().to_path_buf(),
                command_line: handler.printable_command_line(),
            });
        }

        let (tx, rx) = mpsc::channel::<(OutputKind, String)>();
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let out_thread = spawn_reader(stdout, OutputKind::Stdout, tx.clone());
        let err_thread = spawn_reader(stderr, OutputKind::Stderr, tx);

        let mut output = Vec::new();
        let mut error_output = Vec::new();
        for (kind, line) in rx {
            if log_to_console && should_log_line(handler, kind, &line) {
                self.console.event(ConsoleEvent::Output {
                    id,
                    kind,
                    line: line.clone(),
                });
            }
            match kind {
                OutputKind::Stdout => output.push(line),
                OutputKind::Stderr => {
                    if !looks_like_progress(&line) {
                        error_output.push(line);
                    }
                }
            }
        }
        let _ = out_thread.join();
        let _ = err_thread.join();
        let exit_code = match child.wait() {
            Ok(status) => status.code().unwrap_or(-1),
            Err(_) => -1,
        };
        if log_to_console {
            self.console.event(ConsoleEvent::Finished { id, exit_code });
        }
        GitCommandResult {
            start_failed: false,
            exit_code,
            output,
            error_output,
        }
    }
}

fn should_take_write_lock(command: GitCommand, can_suppress_optional_locks: bool) -> bool {
    let policy = command.locking_policy();
    policy != LockingPolicy::Read
        && (policy != LockingPolicy::ReadOptionalLocking || !can_suppress_optional_locks)
}

/// Upstream `getGitTraceEnvironmentVariables` with tracing disabled (registry `git.execution.trace` = 0).
fn trace_environment(version: &GitVersion) -> Vec<(&'static str, &'static str)> {
    let mut env = vec![("GIT_TRACE", "0")];
    if GitVersionSpecialty::EnvGitTracePackAccessAllowed.exists_in(version) {
        env.push(("GIT_TRACE_PACK_ACCESS", ""));
    }
    env.push(("GIT_TRACE_PACKET", ""));
    env.push(("GIT_TRACE_PERFORMANCE", "0"));
    env.push(("GIT_TRACE_SETUP", "0"));
    env
}

/// Upstream `GitCommandOutputLogger.onLineAvailable` filters.
fn should_log_line(handler: &GitHandler, kind: OutputKind, line: &str) -> bool {
    if line.trim().is_empty() {
        return false;
    }
    match kind {
        OutputKind::Stdout => !handler.is_stdout_suppressed(),
        OutputKind::Stderr => !handler.is_stderr_suppressed(),
    }
}

const REMOTE_PROGRESS_PREFIX: &str = "remote: ";
const SUPPRESSED_PROGRESS_INDICATORS: &[&str] = &[
    "Counting objects: ",
    "Enumerating objects: ",
    "Compressing objects: ",
    "Writing objects: ",
    "Receiving objects: ",
    "Resolving deltas: ",
    "Finding sources: ",
    "Updating files: ",
    "Checking out files: ",
    "Expanding reachable commits in commit graph: ",
    "Delta compression using up to ",
];

/// Port of `GitImplBase.looksLikeProgress` (`PROGRESS_PATTERN` = `.*:\s*\d{1,3}% \(\d+/\d+\).*`).
pub fn looks_like_progress(line: &str) -> bool {
    use std::sync::LazyLock;
    static PROGRESS: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^.*:\s*\d{1,3}% \(\d+/\d+\).*$").expect("valid regex")
    });
    if PROGRESS.is_match(line) {
        return true;
    }
    SUPPRESSED_PROGRESS_INDICATORS.iter().any(|prefix| {
        line.starts_with(prefix)
            || line
                .strip_prefix(REMOTE_PROGRESS_PREFIX)
                .is_some_and(|rest| rest.starts_with(prefix))
    })
}

/// Splits a byte stream into lines on `\n`, `\r\n` and `\r` (upstream `BufferingTextSplitter`).
#[derive(Default)]
pub(crate) struct LineSplitter {
    current: Vec<u8>,
    pending_cr: bool,
}

impl LineSplitter {
    pub(crate) fn feed(&mut self, bytes: &[u8], emit: &mut dyn FnMut(String)) {
        for &b in bytes {
            if self.pending_cr {
                self.pending_cr = false;
                if b == b'\n' {
                    continue; // CRLF: the line was already emitted at CR
                }
            }
            match b {
                b'\n' => emit(self.take_line()),
                b'\r' => {
                    emit(self.take_line());
                    self.pending_cr = true;
                }
                _ => self.current.push(b),
            }
        }
    }

    pub(crate) fn finish(&mut self, emit: &mut dyn FnMut(String)) {
        if !self.current.is_empty() {
            emit(self.take_line());
        }
    }

    fn take_line(&mut self) -> String {
        let line = String::from_utf8_lossy(&self.current).into_owned();
        self.current.clear();
        line
    }
}

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    kind: OutputKind,
    tx: mpsc::Sender<(OutputKind, String)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut splitter = LineSplitter::default();
        let mut buf = [0u8; 8192];
        let mut emit = |line: String| {
            let _ = tx.send((kind, line));
        };
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => splitter.feed(&buf[..n], &mut emit),
            }
        }
        splitter.finish(&mut emit);
    })
}
```

- [ ] **Step 4: Run the unit tests**

Run: `cargo test -p ig-git --lib runner::`
Expected: PASS (4 tests).

- [ ] **Step 5: Add the real-git test helpers** `crates/ig-git/tests/common/mod.rs`

```rust
//! Helpers for integration tests that run the real `git` executable.
#![allow(dead_code)] // each test crate uses a different subset

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use ig_git::console::{ConsoleSink, NullConsole};
use ig_git::runner::Git;

/// An empty file used as the global git config, so the developer's config can't affect tests.
fn empty_global_config() -> PathBuf {
    let path = std::env::temp_dir().join("ideagit-tests-empty-gitconfig");
    if !path.exists() {
        std::fs::write(&path, "").unwrap();
    }
    path
}

/// Runs git in `dir` with a fixed identity and no global/system config; returns the raw output.
pub fn git_output(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", empty_global_config())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .expect("git is installed")
}

/// Like [`git_output`], panicking on failure; returns stdout.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = git_output(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `git init -b main` with one commit of `a.txt`.
pub fn init_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("a.txt"), "one\n").unwrap();
    git(dir, &["add", "a.txt"]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

pub fn runner() -> Git {
    runner_with(Arc::new(NullConsole))
}

pub fn runner_with(console: Arc<dyn ConsoleSink>) -> Git {
    Git::detect("git", console).expect("git is installed and detectable")
}
```

- [ ] **Step 6: Add the runner integration tests** `crates/ig-git/tests/runner_integration.rs`

```rust
//! Runner integration tests against the real `git` executable.

mod common;

use std::sync::Arc;

use common::{init_repo, runner, runner_with};
use ig_git::command::GitCommand;
use ig_git::console::{ConsoleEvent, OutputKind, RecordingConsole};

#[test]
fn detects_supported_version() {
    let git = runner();
    assert!(
        git.version().is_supported(),
        "installed git {:?} should be supported",
        git.version()
    );
}

#[test]
fn write_commands_are_logged_to_console_read_commands_are_not() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    let console = Arc::new(RecordingConsole::default());
    let g = runner_with(console.clone());
    let before = console.events().len();

    let mut status = g.handler(tmp.path(), GitCommand::STATUS);
    status.add_parameters(["--porcelain"]);
    assert!(g.run_command(status).success());
    assert_eq!(console.events().len(), before, "status is silent");

    let mut checkout = g.handler(tmp.path(), GitCommand::CHECKOUT);
    checkout.add_parameters(["-b", "feature"]);
    let result = g.run_command(checkout);
    assert!(result.success(), "{:?}", result);

    let events = console.events()[before..].to_vec();
    match &events[0] {
        ConsoleEvent::Started {
            command_line,
            working_dir,
            ..
        } => {
            assert_eq!(
                command_line,
                "git -c core.quotepath=false -c log.showSignature=false checkout -b feature"
            );
            assert_eq!(working_dir, tmp.path());
        }
        other => panic!("expected Started, got {other:?}"),
    }
    assert!(events.iter().any(|e| matches!(e,
        ConsoleEvent::Output { kind: OutputKind::Stderr, line, .. } if line.contains("Switched to a new branch 'feature'"))));
    assert!(matches!(
        events.last(),
        Some(ConsoleEvent::Finished { exit_code: 0, .. })
    ));
}

#[test]
fn failed_command_reports_stderr() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    let g = runner();
    let mut h = g.handler(tmp.path(), GitCommand::CHECKOUT);
    h.add_parameters(["no-such-branch"]);
    let result = g.run_command(h);
    assert!(!result.success());
    let err = result.output_or_error().unwrap_err().to_string();
    assert!(err.contains("no-such-branch"), "{err}");
}

#[test]
fn missing_executable_is_start_failure() {
    let g = ig_git::runner::Git::new(
        "/definitely/not/git",
        ig_git::version::GitVersion::NULL,
        Arc::new(ig_git::console::NullConsole),
    );
    let result = g.run_command(g.handler(std::path::Path::new("."), GitCommand::VERSION));
    assert!(result.start_failed);
    assert!(result.error_output[0].starts_with("Failed to start Git process"));
}
```

- [ ] **Step 7: Run them**

Run: `cargo test -p ig-git --test runner_integration`
Expected: PASS (4 tests). If `detects_supported_version` fails, the installed git is older than 2.17 (2.19.2 on Windows); upgrade git.

- [ ] **Step 8: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): git runner with locking, trace env and console events"
```

---
### Task 7: `git status` porcelain v1 parsing and invocation

**Files:**
- Create: `crates/ig-git/src/status.rs`
- Modify: `crates/ig-git/src/lib.rs`
- Create: `crates/ig-git/tests/status_integration.rs`
Upstream: `$IJ/plugins/git4idea/backend/src/index/GitIndexStatusUtil.kt` (`parseGitStatusOutput`, `appendParameters`, `getFileStatus`, the `is*` helpers, `LightFileStatus.StatusRecord`). Error message texts come from `GitBundle.properties` (`status.exception.message.*`).

- [ ] **Step 1: Write the failing tests**: create `crates/ig-git/src/status.rs` containing only the test module

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn rec(index: char, work_tree: char, path: &str, orig: Option<&str>) -> StatusRecord {
        StatusRecord {
            index,
            work_tree,
            path: path.into(),
            orig_path: orig.map(Into::into),
        }
    }

    #[test]
    fn parses_records_renames_and_skips_noise() {
        let out = " M a.txt\0R  new.txt\0old.txt\0?? dir/u.txt\0\0starting fsmonitor-daemon in x\0UU c.txt\0";
        let records = parse_git_status_output(out).unwrap();
        assert_eq!(
            records,
            vec![
                rec(' ', 'M', "a.txt", None),
                rec('R', ' ', "new.txt", Some("old.txt")),
                rec('?', '?', "dir/u.txt", None),
                rec('U', 'U', "c.txt", None),
            ]
        );
    }

    #[test]
    fn empty_output_is_clean() {
        assert!(parse_git_status_output("").unwrap().is_empty());
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(parse_git_status_output("M").is_err());
        assert!(parse_git_status_output("MMXa.txt").is_err());
        assert!(parse_git_status_output("ZZ a.txt").is_err());
        assert!(
            parse_git_status_output("R  new.txt").is_err(),
            "rename without orig path"
        );
    }

    #[test]
    fn file_status_mapping() {
        assert_eq!(rec(' ', 'M', "a", None).file_status(), FileStatus::Modified);
        assert_eq!(rec('A', ' ', "a", None).file_status(), FileStatus::Added);
        assert_eq!(
            rec('A', 'A', "a", None).file_status(),
            FileStatus::MergedWithConflicts
        );
        assert_eq!(
            rec('D', 'D', "a", None).file_status(),
            FileStatus::MergedWithConflicts
        );
        assert_eq!(rec('?', '?', "a", None).file_status(), FileStatus::Unknown);
        assert_eq!(rec('!', '!', "a", None).file_status(), FileStatus::Ignored);
        assert_eq!(rec('R', 'M', "n", Some("o")).repository_path(), Some("o"));
        assert_eq!(rec('A', ' ', "n", None).repository_path(), None);
        assert_eq!(rec('?', '?', "n", None).repository_path(), None);
    }

    #[test]
    fn parameters_by_version() {
        let modern = GitVersion::new(2, 40, 0, 0);
        assert_eq!(
            status_parameters(&modern, StatusOptions::default()),
            ["--porcelain", "-z", "--untracked-files=all", "--ignored=no"]
        );
        let opts = StatusOptions {
            with_renames: false,
            with_ignored: true,
            ..Default::default()
        };
        assert_eq!(
            status_parameters(&modern, opts),
            [
                "--porcelain",
                "-z",
                "--no-renames",
                "--untracked-files=all",
                "--ignored=matching"
            ]
        );
        let old = GitVersion::new(2, 15, 0, 0);
        assert_eq!(
            status_parameters(&old, opts),
            ["--porcelain", "-z", "--untracked-files=all", "--ignored"]
        );
    }
}
```

and register the module in `crates/ig-git/src/lib.rs`:

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod command;
pub mod console;
pub mod error;
pub mod handler;
pub mod hash;
pub mod runner;
pub mod status;
pub mod version;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ig-git --lib status::`
Expected: FAIL. Compilation errors for the not-yet-defined items (`cannot find ...` / `unresolved import`).

- [ ] **Step 3: Implement**: insert this **above** the `#[cfg(test)]` module in `crates/ig-git/src/status.rs`

```rust
//! Port of `plugins/git4idea/backend/src/index/GitIndexStatusUtil.kt`:
//! runs `git status --porcelain -z` (format v1) and parses its output.
//!
//! `git status` porcelain v1 `XY PATH` / `XY PATH\0ORIG_PATH` codes:
//! ` `=unmodified, M=modified, A=added, D=deleted, R=renamed, C=copied, T=type changed,
//! U=unmerged, ?=untracked, !=ignored. See the upstream file for the full table.
//!
//! Not ported: splitting very long path lists into chunks (`GitUtil.createHandlerWithPaths`).

use std::path::{Path, PathBuf};

use crate::command::GitCommand;
use crate::error::{GitError, Result};
use crate::runner::Git;
use crate::version::{GitVersion, GitVersionSpecialty};

pub type StatusCode = char;

/// Subset of IntelliJ's `FileStatus` produced by git status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    NotChanged,
    Modified,
    Added,
    Deleted,
    MergedWithConflicts,
    Ignored,
    /// Untracked.
    Unknown,
}

/// Upstream `LightFileStatus.StatusRecord`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRecord {
    pub index: StatusCode,
    pub work_tree: StatusCode,
    pub path: String,
    pub orig_path: Option<String>,
}

impl StatusRecord {
    pub fn file_status(&self) -> FileStatus {
        if self.is_conflicted() {
            return FileStatus::MergedWithConflicts;
        }
        status_code_to_file_status(self.index)
            .or_else(|| status_code_to_file_status(self.work_tree))
            .unwrap_or(FileStatus::NotChanged)
    }

    pub fn is_conflicted(&self) -> bool {
        is_conflicted(self.index, self.work_tree)
    }

    pub fn is_tracked(&self) -> bool {
        !is_ignored(self.index) && !is_untracked(self.index)
    }

    /// Path of the file in HEAD, or `None` if the file is not in the repository yet.
    pub fn repository_path(&self) -> Option<&str> {
        if !self.is_tracked() || self.index == 'A' || self.work_tree == 'A' {
            None
        } else {
            Some(self.orig_path.as_deref().unwrap_or(&self.path))
        }
    }
}

/// Upstream `getFileStatus(StatusCode)`; `None` for unmodified (` `) and unrecognized codes.
pub fn status_code_to_file_status(status: StatusCode) -> Option<FileStatus> {
    match status {
        'M' | 'R' | 'C' | 'T' => Some(FileStatus::Modified),
        'A' => Some(FileStatus::Added),
        'D' => Some(FileStatus::Deleted),
        'U' => Some(FileStatus::MergedWithConflicts),
        '!' => Some(FileStatus::Ignored),
        '?' => Some(FileStatus::Unknown),
        _ => None,
    }
}

pub fn is_ignored(status: StatusCode) -> bool {
    status == '!'
}
pub fn is_untracked(status: StatusCode) -> bool {
    status == '?'
}
pub fn is_renamed(status: StatusCode) -> bool {
    status == 'R' || status == 'C'
}
pub fn is_added(status: StatusCode) -> bool {
    status == 'A'
}
pub fn is_intended_to_be_added(index: StatusCode, work_tree: StatusCode) -> bool {
    index == ' ' && work_tree == 'A'
}
pub fn is_deleted(status: StatusCode) -> bool {
    status == 'D'
}
pub fn is_conflicted(index: StatusCode, work_tree: StatusCode) -> bool {
    (index == 'D' && work_tree == 'D')
        || (index == 'A' && work_tree == 'A')
        || (index == 'T' && work_tree == 'T')
        || (index == 'U' || work_tree == 'U')
}

fn is_known_status(status: char) -> bool {
    matches!(
        status,
        ' ' | 'M' | 'A' | 'D' | 'C' | 'R' | 'U' | 'T' | '!' | '?'
    )
}

fn parse_error(message: &str, output: &str, line: &str, x: char, y: char) -> GitError {
    GitError::StatusParse {
        message: format!("{message}\noutput:\n{output}"),
        x,
        y,
        line: line.to_string(),
    }
}

/// Upstream `parseGitStatusOutput`.
pub fn parse_git_status_output(output: &str) -> Result<Vec<StatusRecord>> {
    let mut result = Vec::new();
    let mut it = output.split('\0');
    while let Some(line) = it.next() {
        if line.trim().is_empty() {
            continue; // e.g. the whole output is empty on a clean working tree
        }
        if line.starts_with("starting fsmonitor-daemon in ") {
            continue; // debug output from git-for-windows 2.33
        }
        if line.chars().count() < 4 || line.as_bytes().get(2) != Some(&b' ') {
            return Err(parse_error("The line is too short", output, line, '0', '0'));
        }
        let mut chars = line.chars();
        let x = chars.next().expect("length checked");
        let y = chars.next().expect("length checked");
        if !is_known_status(x) || !is_known_status(y) {
            return Err(parse_error(
                "Unexpected symbol as status",
                output,
                line,
                x,
                y,
            ));
        }
        let path = line[3..].to_string();
        if is_renamed(x) || is_renamed(y) {
            let orig = it
                .next()
                .ok_or_else(|| parse_error("Missing the original path", output, line, x, y))?;
            result.push(StatusRecord {
                index: x,
                work_tree: y,
                path,
                orig_path: Some(orig.to_string()),
            });
        } else {
            result.push(StatusRecord {
                index: x,
                work_tree: y,
                path,
                orig_path: None,
            });
        }
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy)]
pub struct StatusOptions {
    pub with_renames: bool,
    pub with_untracked: bool,
    pub with_ignored: bool,
    pub expand_ignored_directories: bool,
}

impl Default for StatusOptions {
    /// Upstream `appendParameters` defaults.
    fn default() -> Self {
        Self {
            with_renames: true,
            with_untracked: true,
            with_ignored: false,
            expand_ignored_directories: false,
        }
    }
}

/// Upstream `GitLineHandler.appendParameters` for `git status`.
pub fn status_parameters(version: &GitVersion, opts: StatusOptions) -> Vec<String> {
    let mut p = vec!["--porcelain".to_string(), "-z".to_string()];
    if !opts.with_renames && GitVersionSpecialty::StatusSupportsNoRenames.exists_in(version) {
        p.push("--no-renames".into());
    }
    p.push(format!(
        "--untracked-files={}",
        if opts.with_untracked { "all" } else { "no" }
    ));
    if opts.with_ignored && opts.expand_ignored_directories {
        p.push("--ignored".into());
    } else if GitVersionSpecialty::StatusSupportsIgnoredModes.exists_in(version) {
        p.push(
            if opts.with_ignored {
                "--ignored=matching"
            } else {
                "--ignored=no"
            }
            .into(),
        );
    } else if opts.with_ignored {
        p.push("--ignored".into());
    }
    p
}

/// Upstream `getFileStatus(project, root, files, ...)`. An empty `files` list means the whole root.
pub fn get_file_status(
    git: &Git,
    root: &Path,
    files: &[PathBuf],
    opts: StatusOptions,
) -> Result<Vec<StatusRecord>> {
    let mut h = git.handler(root, GitCommand::STATUS);
    h.set_silent(true);
    h.add_parameters(status_parameters(&git.version(), opts));
    if !files.is_empty() {
        h.end_options();
        h.add_relative_paths(files);
    }
    let output = git.run_command(h).output_or_error()?;
    parse_git_status_output(&output)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ig-git --lib status::`
Expected: PASS (all tests in `status::tests`).

- [ ] **Step 4b: Add the status integration test** `crates/ig-git/tests/status_integration.rs`

```rust
//! `git status` integration tests against the real `git` executable.

mod common;

use common::{git, init_repo, runner};
use ig_git::status::{FileStatus, StatusOptions, get_file_status};

#[test]
fn status_reports_changes_renames_and_untracked() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_repo(root);
    std::fs::write(root.join("a.txt"), "two\n").unwrap();
    std::fs::write(root.join("b.txt"), "b\n").unwrap();
    git(root, &["add", "b.txt"]);
    git(root, &["commit", "-q", "-m", "add b"]);
    git(root, &["mv", "b.txt", "c d.txt"]);
    std::fs::create_dir_all(root.join("dir")).unwrap();
    std::fs::write(root.join("dir/new.txt"), "n\n").unwrap();

    let g = runner();
    let mut records = get_file_status(&g, root, &[], StatusOptions::default()).unwrap();
    records.sort_by(|a, b| a.path.cmp(&b.path));
    let summary: Vec<(String, FileStatus, Option<String>)> = records
        .iter()
        .map(|r| (r.path.clone(), r.file_status(), r.orig_path.clone()))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("a.txt".into(), FileStatus::Modified, None),
            ("c d.txt".into(), FileStatus::Modified, Some("b.txt".into())),
            ("dir/new.txt".into(), FileStatus::Unknown, None),
        ]
    );

    let only_a =
        get_file_status(&g, root, &[root.join("a.txt")], StatusOptions::default()).unwrap();
    assert_eq!(only_a.len(), 1);
    assert_eq!(only_a[0].path, "a.txt");
}
```

Run: `cargo test -p ig-git --test status_integration`
Expected: PASS (1 test).

- [ ] **Step 5: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): status (`git status` porcelain v1 parsing and invocation)"
```

---
### Task 8: Repository files layout and `.git` discovery

**Files:**
- Create: `crates/ig-git/src/repo_files.rs`
- Modify: `crates/ig-git/src/lib.rs`

Upstream: `$IJ/plugins/git4idea/backend/src/repo/GitRepositoryFiles.java` (main vs worktree dir split, `getMainGitDirForWorktree`), `GitUtil.java` (`findGitDir`, `parsePathToRepository`), `platform/dvcs-impl/.../DvcsUtil.java` (`tryLoadFile`).

- [ ] **Step 1: Write the failing tests**: create `crates/ig-git/src/repo_files.rs` containing only the test module

```rust
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
```

and register the module in `crates/ig-git/src/lib.rs`:

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod command;
pub mod console;
pub mod error;
pub mod handler;
pub mod hash;
pub mod repo_files;
pub mod runner;
pub mod status;
pub mod version;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ig-git --lib repo_files::`
Expected: FAIL. Compilation errors for the not-yet-defined items (`cannot find ...` / `unresolved import`).

- [ ] **Step 3: Implement**: insert this **above** the `#[cfg(test)]` module in `crates/ig-git/src/repo_files.rs`

```rust
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
    let content = std::fs::read_to_string(&dot_git).ok()?;
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
    let content = std::fs::read_to_string(path)
        .map_err(|e| GitError::RepoState(format!("couldn't load {}: {e}", path.display())))?;
    Ok(content
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string())
}

/// Lexically resolves `.` and `..` (upstream `FileUtil.toCanonicalPath`), without touching symlinks.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ig-git --lib repo_files::`
Expected: PASS (all tests in `repo_files::tests`).

- [ ] **Step 5: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): repo_files (Repository files layout and `.git` discovery)"
```

---
### Task 9: Ref parsing, symbolic resolution, loose refs, ref-name validation

**Files:**
- Create: `crates/ig-git/src/refs.rs`
- Modify: `crates/ig-git/src/lib.rs`

Upstream: `$IJ/plugins/git4idea/backend/src/repo/GitRefUtil.java`, `validators/GitRefNameValidator.java` (the Java regex uses look-arounds, which Rust `regex` lacks, so `check_ref_name` spells out each rule), and `shared/src/com/intellij/vcs/git/ref/GitRefUtil.kt` (`stripRefsPrefix`).

- [ ] **Step 1: Write the failing tests**: create `crates/ig-git/src/refs.rs` containing only the test module

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_packed_refs_lines() {
        assert_eq!(
            parse_refs_line("0e1d130689bc52f140c5c374aa9cc2b8916c0ad7 refs/heads/master"),
            Some((
                "refs/heads/master".into(),
                "0e1d130689bc52f140c5c374aa9cc2b8916c0ad7".into()
            ))
        );
        assert_eq!(parse_refs_line("# pack-refs with: peeled"), None);
        assert_eq!(
            parse_refs_line("^eb355ef03aaa139b76c3a938c27458f6ee5ca002"),
            None
        );
        assert_eq!(parse_refs_line(""), None);
        assert_eq!(
            parse_refs_line("0e1d130689bc52f140c5c374aa9cc2b8916c0ad7"),
            None
        );
        assert_eq!(
            parse_branches_line("c5cf9cb49c25899442bd98b77c2e60b41cd8b85d refs/tags/tag1"),
            None
        );
    }

    #[test]
    fn symbolic_targets() {
        assert_eq!(
            get_target("ref: refs/heads/master").as_deref(),
            Some("refs/heads/master")
        );
        assert_eq!(
            get_target("ref:master").as_deref(),
            Some("refs/heads/master")
        );
        assert_eq!(
            get_target("ref: refs/remotes/origin/feature").as_deref(),
            Some("refs/remotes/origin/feature")
        );
    }

    #[test]
    fn resolves_links_and_drops_cycles() {
        let h = "0e1d130689bc52f140c5c374aa9cc2b8916c0ad7";
        let data: HashMap<String, String> = [
            ("refs/heads/master", h),
            ("refs/heads/link", "ref: refs/heads/master"),
            ("refs/heads/transitive", "ref: refs/heads/link"),
            ("refs/heads/c1", "ref: refs/heads/c2"),
            ("refs/heads/c2", "ref: refs/heads/c1"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let resolved = resolve_refs(&data);
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved["refs/heads/transitive"].as_str(), h);
        assert!(!resolved.contains_key("refs/heads/c1"));
    }

    #[test]
    fn ref_name_validation() {
        assert!(check_ref_name("refs/heads/feature/x"));
        for bad in [
            "", " ", ".a", "-a", "/a", "a..b", "a//b", "a/.b", "a./b", "a.lock", "a/", "a.", "a b",
            "a~b", "a@{b", "a\u{1}",
        ] {
            assert!(!check_ref_name(bad), "{bad:?} should be invalid");
        }
    }
}
```

and register the module in `crates/ig-git/src/lib.rs`:

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod command;
pub mod console;
pub mod error;
pub mod handler;
pub mod hash;
pub mod refs;
pub mod repo_files;
pub mod runner;
pub mod status;
pub mod version;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ig-git --lib refs::`
Expected: FAIL. Compilation errors for the not-yet-defined items (`cannot find ...` / `unresolved import`).

- [ ] **Step 3: Implement**: insert this **above** the `#[cfg(test)]` module in `crates/ig-git/src/refs.rs`

```rust
//! Port of `plugins/git4idea/backend/src/repo/GitRefUtil.java`,
//! `plugins/git4idea/backend/src/validators/GitRefNameValidator.java` (`checkInput`), and
//! `plugins/git4idea/shared/src/com/intellij/vcs/git/ref/GitRefUtil.kt` (`stripRefsPrefix`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::hash::Hash;
use crate::repo_files::try_load_file;

pub const REFS_HEADS_PREFIX: &str = "refs/heads/";
pub const REFS_REMOTES_PREFIX: &str = "refs/remotes/";
pub const REFS_TAGS_PREFIX: &str = "refs/tags/";

static BRANCH_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^ *(?:ref:)? */?((?:refs/heads/|refs/remotes/)?\S+)\s*$").expect("valid regex")
});

pub fn strip_refs_prefix(ref_name: &str) -> &str {
    for prefix in [REFS_HEADS_PREFIX, REFS_REMOTES_PREFIX, REFS_TAGS_PREFIX] {
        if let Some(rest) = ref_name.strip_prefix(prefix) {
            return rest;
        }
    }
    ref_name
}

/// Parses a `<hash> <refname>` line, e.g. from `.git/packed-refs`. Returns `(refname, hash)`.
/// Comments, peeled-tag lines (`^...`) and malformed lines yield `None`.
pub fn parse_refs_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    let first = line.chars().next()?;
    if first == '#' || first == '^' {
        return None;
    }
    let (hash_end, sep) = line.char_indices().find(|(_, c)| !c.is_alphanumeric())?;
    let hash = &line[..hash_end];
    if !sep.is_whitespace() {
        return None;
    }
    let start = hash_end + sep.len_utf8();
    let rest = &line[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some((rest[..end].to_string(), hash.trim().to_string()))
}

/// Like [`parse_refs_line`], but only for local and remote branches.
pub fn parse_branches_line(line: &str) -> Option<(String, String)> {
    let (name, hash) = parse_refs_line(line)?;
    if name.starts_with(REFS_HEADS_PREFIX) || name.starts_with(REFS_REMOTES_PREFIX) {
        Some((name, hash))
    } else {
        None
    }
}

/// Target of a symbolic ref value such as `ref: refs/heads/main` (or `ref:main` → `refs/heads/main`).
pub fn get_target(ref_value: &str) -> Option<String> {
    let caps = BRANCH_PATTERN.captures(ref_value)?;
    let target = caps.get(1)?.as_str();
    if target.starts_with(REFS_HEADS_PREFIX) || target.starts_with(REFS_REMOTES_PREFIX) {
        Some(target.to_string())
    } else {
        Some(format!("{REFS_HEADS_PREFIX}{target}"))
    }
}

/// Resolves `refname -> value` where value is a hash or a symbolic ref to another entry.
/// Unresolvable and cyclic symbolic refs are dropped.
pub fn resolve_refs(data: &HashMap<String, String>) -> HashMap<String, Hash> {
    let mut resolved: HashMap<String, Hash> = HashMap::new();
    for (name, value) in data {
        if let Some(hash) = Hash::parse(value) {
            resolved.insert(name.clone(), hash);
        }
    }
    let mut unresolved: Vec<(String, String)> = data
        .iter()
        .filter(|(name, _)| !resolved.contains_key(*name))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let mut progressed = true;
    while progressed && !unresolved.is_empty() {
        progressed = false;
        unresolved.retain(|(name, value)| match get_target(value) {
            None => false, // unexpected record: dropped (upstream logs a warning)
            Some(link) => match resolved.get(&link).cloned() {
                Some(hash) => {
                    resolved.insert(name.clone(), hash);
                    progressed = true;
                    false
                }
                None => true, // transitive link, try again next round
            },
        });
    }
    resolved
}

/// Reads loose refs under `refs_root_dir` (e.g. `.git/refs/heads`) as `prefix + relative path -> value`.
/// Hidden files and directories (starting with `.`) and invalid ref names are skipped.
pub fn read_from_refs_files(refs_root_dir: &Path, prefix: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    if refs_root_dir.is_dir() {
        collect_refs(refs_root_dir, refs_root_dir, prefix, &mut result);
    }
    result
}

fn collect_refs(root: &Path, dir: &Path, prefix: &str, result: &mut HashMap<String, String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_refs(root, &path, prefix, result);
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let branch_name = format!("{prefix}{rel}");
        if check_ref_name(&branch_name) {
            if let Ok(value) = try_load_file(&path) {
                result.insert(branch_name, value);
            }
        }
    }
}

/// Port of `GitRefNameValidator.checkInput`: true if `name` is a valid git reference name.
pub fn check_ref_name(name: &str) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    let starts_bad = name.starts_with('.') || name.starts_with('-') || name.starts_with('/');
    let has_bad_char = name.chars().any(|c| {
        matches!(c, '~' | ':' | '^' | '?' | '*' | '"' | '[' | '\\' | ' ')
            || (c as u32) < 32
            || c == '\u{7f}'
    });
    let bad_sequence = ["@{", "//", "..", "./", "/."]
        .iter()
        .any(|s| name.contains(s));
    let ends_bad = name.ends_with('.') || name.ends_with('/') || name.ends_with(".lock");
    !(starts_bad || has_bad_char || bad_sequence || ends_bad)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ig-git --lib refs::`
Expected: PASS (all tests in `refs::tests`).

- [ ] **Step 5: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): refs (Ref parsing, symbolic resolution, loose refs, ref-name validation)"
```

---
### Task 10: Remotes via `git config --null -l`

**Files:**
- Create: `crates/ig-git/src/config.rs`
- Modify: `crates/ig-git/src/lib.rs`
- Create: `crates/ig-git/tests/config_integration.rs`
Upstream: `$IJ/plugins/git4idea/backend/src/config/GitConfigUtil.java` (`getValues`) and `$IJ/plugins/git4idea/backend/src/repo/GitConfig.kt` (`read`, `parseSectionNameAndVariable`, `createRemote`, the `urls.isNotEmpty()` filter). Git itself parses the config, so includes, escapes and continuation lines behave exactly as in git. `url.insteadOf`, branch tracking and hooks belong to Plan 8 (push/pull).

- [ ] **Step 1: Write the failing tests**: create `crates/ig-git/src/config.rs` containing only the test module

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_null_output_with_multiline_and_valueless_entries() {
        let out = "core.bare\nfalse\0core.flag\0remote.origin.url\nu1\0remote.origin.url\nu2\0alias.x\nline1\nline2\0";
        let v = parse_null_config_output(out);
        assert_eq!(v.get("core.bare"), Some(&["false".to_string()][..]));
        assert_eq!(v.get("core.flag"), Some(&["true".to_string()][..]));
        assert_eq!(v.get("remote.origin.url").map(<[String]>::len), Some(2));
        assert_eq!(v.get("alias.x"), Some(&["line1\nline2".to_string()][..]));
        assert_eq!(
            v.keys().collect::<Vec<_>>(),
            ["core.bare", "core.flag", "remote.origin.url", "alias.x"]
        );
    }

    #[test]
    fn builds_remotes_like_upstream() {
        let out = "remote.origin.url\ngit@h:o.git\0remote.origin.fetch\n+refs/heads/*:refs/remotes/origin/*\0\
                   remote.NoUrl.fetch\n+refs/heads/*:refs/remotes/NoUrl/*\0\
                   remote.team.a.url\nhttps://h/a.git\0remote.team.a.pushurl\nssh://h/a.git\0\
                   svn-remote.svn.url\nsvn://h/r\0branch.main.remote\norigin\0";
        let remotes = remotes_from_values(&parse_null_config_output(out));
        let names: Vec<&str> = remotes.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            ["origin", "team.a", "svn"],
            "url-less remotes are skipped"
        );
        assert_eq!(
            remotes[0].fetch_refspecs,
            ["+refs/heads/*:refs/remotes/origin/*"]
        );
        assert_eq!(remotes[1].push_urls, ["ssh://h/a.git"]);
    }
}
```

and register the module in `crates/ig-git/src/lib.rs`:

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod command;
pub mod config;
pub mod console;
pub mod error;
pub mod handler;
pub mod hash;
pub mod refs;
pub mod repo_files;
pub mod runner;
pub mod status;
pub mod version;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ig-git --lib config::`
Expected: FAIL. Compilation errors for the not-yet-defined items (`cannot find ...` / `unresolved import`).

- [ ] **Step 3: Implement**: insert this **above** the `#[cfg(test)]` module in `crates/ig-git/src/config.rs`

```rust
//! Port of `plugins/git4idea/backend/src/config/GitConfigUtil.java` (`getValues`) and the remote part
//! of `plugins/git4idea/backend/src/repo/GitConfig.kt` (`read`, `parseSectionNameAndVariable`,
//! `createRemote`, and the `urls.isNotEmpty()` filter of `mapConfiguredRemotes`).
//!
//! As upstream, git itself parses the configuration (`git config --null -l`), so includes, escapes,
//! continuation lines and all config scopes behave exactly as in git.
//!
//! Not yet ported: `url.<base>.insteadOf`/`pushInsteadOf` substitution, branch tracking, hooks,
//! core settings and object format (they arrive with the push/pull plan).

use std::path::Path;

use crate::command::GitCommand;
use crate::error::Result;
use crate::runner::Git;

const GIT_REMOTE_SECTION: &str = "remote";
const SVN_REMOTE_SECTION: &str = "svn-remote";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRemote {
    pub name: String,
    pub urls: Vec<String>,
    pub push_urls: Vec<String>,
    pub fetch_refspecs: Vec<String>,
    pub push_refspecs: Vec<String>,
}

impl GitRemote {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            urls: Vec::new(),
            push_urls: Vec::new(),
            fetch_refspecs: Vec::new(),
            push_refspecs: Vec::new(),
        }
    }
}

/// Config values by key, in order of first appearance (upstream uses a `LinkedHashMap`).
/// Keys are as git prints them: section and variable names lowercased, subsections verbatim.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigValues {
    entries: Vec<(String, Vec<String>)>,
}

impl ConfigValues {
    pub fn get(&self, key: &str) -> Option<&[String]> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_slice())
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }

    fn push(&mut self, key: &str, value: &str) {
        match self.entries.iter_mut().find(|(k, _)| k == key) {
            Some((_, values)) => values.push(value.to_string()),
            None => self
                .entries
                .push((key.to_string(), vec![value.to_string()])),
        }
    }
}

/// Parses `git config --null` output: records of `key\nvalue\0`.
///
/// Deviation: a key printed without value (`key\0`, a valueless boolean) is read as `true`; upstream's
/// parser would merge it into the next record.
pub fn parse_null_config_output(output: &str) -> ConfigValues {
    let mut result = ConfigValues::default();
    let mut rest = output;
    while !rest.is_empty() {
        let Some(nul) = rest.find('\0') else { break };
        let record = &rest[..nul];
        match record.split_once('\n') {
            Some((key, value)) => result.push(key, value),
            None => result.push(record, "true"),
        }
        rest = &rest[nul + 1..];
    }
    result
}

/// Upstream `GitConfigUtil.getValues(project, root, null)`: all values visible from `root`.
pub fn get_values(git: &Git, root: &Path) -> Result<ConfigValues> {
    let mut h = git.handler(root, GitCommand::CONFIG);
    h.set_silent(true);
    h.add_parameters(["--null", "-l"]);
    let output = git.run_command(h).output_or_error()?;
    Ok(parse_null_config_output(&output))
}

/// Upstream `GitConfig.read(project, root).parseRemotes()` without url substitutions: remotes (and
/// svn-remotes) that have at least one url. Like upstream, a failure to read the config yields none.
pub fn read_remotes(git: &Git, root: &Path) -> Vec<GitRemote> {
    get_values(git, root)
        .map(|v| remotes_from_values(&v))
        .unwrap_or_default()
}

pub fn remotes_from_values(values: &ConfigValues) -> Vec<GitRemote> {
    let mut sections: Vec<(&str, &str)> = Vec::new();
    for key in values.keys() {
        if let Some(sv) = parse_section_name_and_variable(key) {
            if !sections.contains(&sv) {
                sections.push(sv);
            }
        }
    }
    sections
        .into_iter()
        .filter(|(section, _)| *section == GIT_REMOTE_SECTION || *section == SVN_REMOTE_SECTION)
        .map(|(section, name)| create_remote(values, section, name))
        .filter(|remote| !remote.urls.is_empty())
        .collect()
}

/// `remote.origin.url` → `("remote", "origin")`; `remote.team.a.url` → `("remote", "team.a")`.
fn parse_section_name_and_variable(key: &str) -> Option<(&str, &str)> {
    let first = key.find('.')?;
    let last = key.rfind('.')?;
    (first != last).then(|| (&key[..first], &key[first + 1..last]))
}

fn create_remote(values: &ConfigValues, section: &str, name: &str) -> GitRemote {
    let get = |variable: &str| {
        values
            .get(&format!("{section}.{name}.{variable}"))
            .map(<[String]>::to_vec)
            .unwrap_or_default()
    };
    GitRemote {
        name: name.to_string(),
        urls: get("url"),
        push_urls: get("pushurl"),
        fetch_refspecs: get("fetch"),
        push_refspecs: get("push"),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ig-git --lib config::`
Expected: PASS (all tests in `config::tests`).

- [ ] **Step 4b: Add the config integration test** `crates/ig-git/tests/config_integration.rs`

```rust
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
    names.retain(|n| ["origin", "Upstream", "inc"].contains(n));
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
```

Run: `cargo test -p ig-git --test config_integration`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): config (Remotes via `git config --null -l`)"
```

---
### Task 11: Local and remote branch types

**Files:**
- Create: `crates/ig-git/src/branch.rs`
- Modify: `crates/ig-git/src/lib.rs`

Upstream: `GitLocalBranch.kt`, `shared/src/git4idea/GitBranch.kt`, `GitStandardRemoteBranch.kt`, `GitSvnRemoteBranch.kt`, and `GitUtil.parseRemoteBranch` (the remote is the shortest `/`-prefix naming a known remote; unknown remotes fall back to the first segment).

- [ ] **Step 1: Write the failing tests**: create `crates/ig-git/src/branch.rs` containing only the test module

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_branch_names() {
        let b = GitLocalBranch::new("refs/heads/feature/x");
        assert_eq!(b.name(), "feature/x");
        assert_eq!(b.full_name(), "refs/heads/feature/x");
    }

    #[test]
    fn remote_branch_parsing() {
        let remotes = vec![GitRemote::new("origin"), GitRemote::new("team/a")];
        assert_eq!(
            parse_remote_branch("refs/remotes/origin/main", &remotes),
            GitRemoteBranch::Standard {
                remote: "origin".into(),
                name_at_remote: "main".into()
            }
        );
        assert_eq!(
            parse_remote_branch("refs/remotes/team/a/feat", &remotes).name(),
            "team/a/feat"
        );
        let b = parse_remote_branch("refs/remotes/team/a/feat", &remotes);
        assert!(matches!(b, GitRemoteBranch::Standard { ref remote, .. } if remote == "team/a"));
        let unknown = parse_remote_branch("refs/remotes/gone/x/y", &remotes);
        assert_eq!(
            unknown,
            GitRemoteBranch::Standard {
                remote: "gone".into(),
                name_at_remote: "x/y".into()
            }
        );
        assert_eq!(
            parse_remote_branch("refs/remotes/trunk", &remotes).full_name(),
            "refs/remotes/trunk"
        );
    }
}
```

and register the module in `crates/ig-git/src/lib.rs`:

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod branch;
pub mod command;
pub mod config;
pub mod console;
pub mod error;
pub mod handler;
pub mod hash;
pub mod refs;
pub mod repo_files;
pub mod runner;
pub mod status;
pub mod version;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ig-git --lib branch::`
Expected: FAIL. Compilation errors for the not-yet-defined items (`cannot find ...` / `unresolved import`).

- [ ] **Step 3: Implement**: insert this **above** the `#[cfg(test)]` module in `crates/ig-git/src/branch.rs`

```rust
//! Branch references. Port of `GitLocalBranch.kt`, `GitStandardRemoteBranch.kt`,
//! `GitSvnRemoteBranch.kt` (shared module) and `GitUtil.parseRemoteBranch`.
//!
//! Deviation: names compare case-sensitively; upstream compares case-insensitively on
//! case-insensitive file systems. `GitRepositoryReader` handles the current-branch case fix.

use crate::config::GitRemote;
use crate::refs::{REFS_HEADS_PREFIX, REFS_REMOTES_PREFIX, strip_refs_prefix};

/// Upstream `GitUtil.ORIGIN_HEAD`.
pub const ORIGIN_HEAD: &str = "origin/HEAD";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GitLocalBranch {
    name: String,
}

impl GitLocalBranch {
    /// Accepts either `main` or `refs/heads/main`.
    pub fn new(name: &str) -> Self {
        Self {
            name: strip_refs_prefix(name).to_string(),
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn full_name(&self) -> String {
        format!("{REFS_HEADS_PREFIX}{}", self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GitRemoteBranch {
    /// e.g. remote `origin`, name at remote `main` → `origin/main`.
    Standard {
        remote: String,
        name_at_remote: String,
    },
    /// git-svn style `refs/remotes/<branch>` without a remote prefix.
    Svn { full_ref: String },
}

impl GitRemoteBranch {
    pub fn name(&self) -> String {
        match self {
            Self::Standard {
                remote,
                name_at_remote,
            } => format!("{remote}/{name_at_remote}"),
            Self::Svn { full_ref } => strip_refs_prefix(full_ref).to_string(),
        }
    }
    pub fn full_name(&self) -> String {
        match self {
            Self::Standard { .. } => format!("{REFS_REMOTES_PREFIX}{}", self.name()),
            Self::Svn { full_ref } => full_ref.clone(),
        }
    }
}

/// Upstream `GitUtil.parseRemoteBranch`: the remote is the shortest `/`-delimited prefix that names a
/// known remote; if none matches, the part before the first `/`.
pub fn parse_remote_branch(full_branch_name: &str, remotes: &[GitRemote]) -> GitRemoteBranch {
    let std_name = strip_refs_prefix(full_branch_name);
    let Some(first_slash) = std_name.find('/') else {
        return GitRemoteBranch::Svn {
            full_ref: full_branch_name.to_string(),
        };
    };
    let mut slash = Some(first_slash);
    while let Some(s) = slash {
        let remote_name = &std_name[..s];
        if remotes.iter().any(|r| r.name == remote_name) {
            return GitRemoteBranch::Standard {
                remote: remote_name.to_string(),
                name_at_remote: std_name[s + 1..].to_string(),
            };
        }
        slash = std_name[s + 1..].find('/').map(|i| s + 1 + i);
    }
    GitRemoteBranch::Standard {
        remote: std_name[..first_slash].to_string(),
        name_at_remote: std_name[first_slash + 1..].to_string(),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ig-git --lib branch::`
Expected: PASS (all tests in `branch::tests`).

- [ ] **Step 5: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): branch (Local and remote branch types)"
```

---
### Task 12: Repository state reader (with ported upstream fixtures)

**Files:**
- Create: `crates/ig-git/src/reader.rs`
- Create: `crates/ig-git/tests/fixtures/repo/example1/**`, `crates/ig-git/tests/fixtures/repo/symbolic-refs/**` (copied from upstream)
- Create: `crates/ig-git/tests/reader_fixtures.rs`, `crates/ig-git/tests/reader_integration.rs`
- Modify: `crates/ig-git/src/lib.rs`

Upstream: `$IJ/plugins/git4idea/backend/src/repo/GitRepositoryReader.kt` (including `GitSequencerTodoReader`). Tests are ported from `plugins/git4idea/tests/git4idea/repo/GitRepositoryReaderTest.kt` and use its test data unchanged.

- [ ] **Step 1: Copy the upstream test data**

```bash
mkdir -p crates/ig-git/tests/fixtures/repo
cp -R "$IJ/plugins/git4idea/testData/repo/example1" "$IJ/plugins/git4idea/testData/repo/symbolic-refs" crates/ig-git/tests/fixtures/repo/
find crates/ig-git/tests/fixtures/repo -name '.hidden*'   # expected: the two hidden entries under example1/dot_git/refs/heads
```

The hidden entries must be present: the reader has to skip them.

- [ ] **Step 2: Write the failing fixture test** `crates/ig-git/tests/reader_fixtures.rs`

```rust
//! Ported from `plugins/git4idea/tests/git4idea/repo/GitRepositoryReaderTest.kt`, using the upstream
//! test data in `fixtures/repo/*` (copied from `plugins/git4idea/testData/repo`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ig_git::console::NullConsole;
use ig_git::reader::GitRepositoryReader;
use ig_git::repo_files::GitRepositoryFiles;
use ig_git::runner::Git;
use ig_git::version::GitVersion;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Copies the fixture into a temp dir and renames `dot_git` to `.git`, like upstream `prepareTest`.
fn prepare(case: &str) -> (tempfile::TempDir, PathBuf) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/repo")
        .join(case);
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("test");
    copy_dir(&src, &root);
    std::fs::rename(root.join("dot_git"), root.join(".git")).unwrap();
    (tmp, root)
}

/// Lines of `<hash> <name>`, empty lines skipped.
fn read_refs(path: &Path) -> Vec<(String, String)> {
    let mut refs: Vec<(String, String)> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let (hash, name) = l.trim().split_once(' ').unwrap();
            (name.to_string(), hash.to_string())
        })
        .collect();
    refs.sort();
    refs
}

fn check_case(case: &str) {
    let (_tmp, root) = prepare(case);
    let files = GitRepositoryFiles::create(&root, &root.join(".git"));
    let git = Git::new("git", GitVersion::NULL, Arc::new(NullConsole));
    // Upstream's test reads remotes from the test project root, which has none: full ref names don't
    // depend on remote resolution, and unknown remotes fall back to the first path segment.
    let state = GitRepositoryReader::new(&files, &git).read_state(&[]);

    let head = std::fs::read_to_string(root.join("head.txt"))
        .unwrap()
        .trim()
        .to_string();
    assert_eq!(
        state.current_revision.as_deref(),
        Some(head.as_str()),
        "HEAD revision"
    );

    let current = std::fs::read_to_string(root.join("current-branch.txt")).unwrap();
    let (current_hash, current_name) = current.trim().split_once(' ').unwrap();
    let current_branch = state.current_branch.clone().expect("on a branch");
    assert_eq!(current_branch.name(), current_name);
    assert_eq!(state.local_branches[&current_branch].as_str(), current_hash);

    let mut local: Vec<(String, String)> = state
        .local_branches
        .iter()
        .map(|(b, h)| (b.full_name(), h.as_str().to_string()))
        .collect();
    local.sort();
    assert_eq!(
        local,
        read_refs(&root.join("local-branches.txt")),
        "local branches"
    );

    let mut remote: Vec<(String, String)> = state
        .remote_branches
        .iter()
        .map(|(b, h)| (b.full_name(), h.as_str().to_string()))
        .collect();
    remote.sort();
    assert_eq!(
        remote,
        read_refs(&root.join("remote-branches.txt")),
        "remote branches"
    );
}

#[test]
fn example1() {
    check_case("example1");
}

#[test]
fn symbolic_refs() {
    check_case("symbolic-refs");
}
```

- [ ] **Step 3: Write the failing real-repository tests** `crates/ig-git/tests/reader_integration.rs`

```rust
//! Repository reader integration tests against real repositories.

mod common;

use common::{git, git_output, init_repo, runner};
use ig_git::config::read_remotes;
use ig_git::reader::{GitRepositoryReader, RepositoryState};
use ig_git::repo_files::{GitRepositoryFiles, find_git_dir};
use ig_git::status::{FileStatus, StatusOptions, get_file_status};

fn read_state(root: &std::path::Path) -> ig_git::reader::GitBranchState {
    let g = runner();
    let git_dir = find_git_dir(root).expect("git dir");
    let files = GitRepositoryFiles::create(root, &git_dir);
    let remotes = read_remotes(&g, root);
    GitRepositoryReader::new(&files, &g).read_state(&remotes)
}

#[test]
fn reads_branch_detached_and_merge_states() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_repo(root);
    let head = git(root, &["rev-parse", "HEAD"]).trim().to_string();

    let state = read_state(root);
    assert_eq!(state.state, RepositoryState::Normal);
    assert_eq!(
        state.current_branch.as_ref().map(|b| b.name()),
        Some("main")
    );
    assert_eq!(state.current_revision.as_deref(), Some(head.as_str()));

    // conflicting merge
    git(root, &["checkout", "-q", "-b", "other"]);
    std::fs::write(root.join("a.txt"), "other\n").unwrap();
    git(root, &["commit", "-q", "-am", "other"]);
    git(root, &["checkout", "-q", "main"]);
    std::fs::write(root.join("a.txt"), "main\n").unwrap();
    git(root, &["commit", "-q", "-am", "main"]);
    let merge = git_output(root, &["merge", "other"]);
    assert!(!merge.status.success(), "merge should conflict");
    assert!(String::from_utf8_lossy(&merge.stdout).contains("CONFLICT"));
    let state = read_state(root);
    assert_eq!(state.state, RepositoryState::Merging);
    let conflicted = get_file_status(&runner(), root, &[], StatusOptions::default()).unwrap();
    assert_eq!(conflicted[0].file_status(), FileStatus::MergedWithConflicts);
    git(root, &["merge", "--abort"]);

    git(root, &["checkout", "-q", "--detach", &head]);
    let state = read_state(root);
    assert_eq!(state.state, RepositoryState::Detached);
    assert_eq!(state.current_branch, None);
    assert_eq!(state.current_revision.as_deref(), Some(head.as_str()));
}

#[test]
fn reads_linked_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path().join("main");
    init_repo(&main);
    let wt = tmp.path().join("wt");
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "wt-branch",
            wt.to_str().unwrap(),
        ],
    );

    let git_dir = find_git_dir(&wt).expect("worktree .git file resolves");
    let files = GitRepositoryFiles::create(&wt, &git_dir);
    assert_eq!(
        std::fs::canonicalize(files.common_git_dir()).unwrap(),
        std::fs::canonicalize(main.join(".git")).unwrap()
    );
    let state = read_state(&wt);
    assert_eq!(
        state.current_branch.as_ref().map(|b| b.name()),
        Some("wt-branch")
    );
    assert!(state.local_branches.keys().any(|b| b.name() == "main"));
}
```

- [ ] **Step 4: Run them to verify they fail**

Run: `cargo test -p ig-git --test reader_fixtures --test reader_integration`
Expected: FAIL. `unresolved import ig_git::reader`.

- [ ] **Step 5: Implement** `crates/ig-git/src/reader.rs`

```rust
//! Port of `plugins/git4idea/backend/src/repo/GitRepositoryReader.kt` (branch state, HEAD and
//! repository state from `.git` files, with a `git for-each-ref` fallback for reftable repositories).
//!
//! Not yet ported: hooks info and shallow detection (needed by the commit/push plans).

use std::collections::HashMap;

use crate::branch::{GitLocalBranch, GitRemoteBranch, ORIGIN_HEAD, parse_remote_branch};
use crate::command::GitCommand;
use crate::config::GitRemote;
use crate::hash::Hash;
use crate::refs::{
    REFS_HEADS_PREFIX, REFS_REMOTES_PREFIX, get_target, parse_branches_line, read_from_refs_files,
    resolve_refs,
};
use crate::repo_files::{GitRepositoryFiles, try_load_file};
use crate::runner::Git;

/// Upstream `Repository.State`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryState {
    Normal,
    Merging,
    Rebasing,
    Detached,
    /// Cherry-pick in progress.
    Grafting,
    Reverting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitBranchState {
    pub current_revision: Option<String>,
    pub current_branch: Option<GitLocalBranch>,
    pub state: RepositoryState,
    pub local_branches: HashMap<GitLocalBranch, Hash>,
    pub remote_branches: HashMap<GitRemoteBranch, Hash>,
}

enum HeadInfo {
    Unknown,
    Branch(GitLocalBranch),
    DetachedHead(Hash),
}

struct GitBranches {
    local: HashMap<GitLocalBranch, Hash>,
    remote: HashMap<GitRemoteBranch, Hash>,
}

const DETACHED_HEAD: &str = "detached HEAD";

pub struct GitRepositoryReader<'a> {
    files: &'a GitRepositoryFiles,
    git: &'a Git,
}

impl<'a> GitRepositoryReader<'a> {
    pub fn new(files: &'a GitRepositoryFiles, git: &'a Git) -> Self {
        Self { files, git }
    }

    pub fn read_state(&self, remotes: &[GitRemote]) -> GitBranchState {
        let head = self.read_head();
        let (branches, head_info) = match head {
            // Registry `git.read.branches.from.disk` is on by default upstream.
            Some(head_info) => (self.read_branches(remotes), head_info),
            None => {
                // reftable format: ask git
                let (branches, from_git) = self.read_branches_from_git(remotes);
                let head_info = match from_git {
                    HeadInfo::Unknown => match self.resolve_head_revision() {
                        Some(hash) => HeadInfo::DetachedHead(hash),
                        None => HeadInfo::Unknown,
                    },
                    other => other,
                };
                (branches, head_info)
            }
        };

        let state = self.read_repository_state(matches!(head_info, HeadInfo::Branch(_)));
        let (current_branch, current_revision) = self.parse_head_state(head_info, state, &branches);
        GitBranchState {
            current_revision,
            current_branch,
            state,
            local_branches: branches.local,
            remote_branches: branches.remote,
        }
    }

    fn parse_head_state(
        &self,
        head_info: HeadInfo,
        state: RepositoryState,
        branches: &GitBranches,
    ) -> (Option<GitLocalBranch>, Option<String>) {
        if let HeadInfo::Branch(branch) = head_info {
            let current = fix_current_branch_case(branch, branches);
            let revision = branches.local.get(&current).map(|h| h.as_str().to_string());
            return (Some(current), revision);
        }
        let current_branch = if state == RepositoryState::Rebasing {
            self.find_rebase_branch()
                .map(|b| fix_current_branch_case(b, branches))
        } else {
            None
        };
        match head_info {
            HeadInfo::DetachedHead(hash) => (current_branch, Some(hash.as_str().to_string())),
            _ => (current_branch, None),
        }
    }

    fn read_repository_state(&self, is_on_branch: bool) -> RepositoryState {
        if self.files.merge_head_file().exists() {
            return RepositoryState::Merging;
        }
        if self.files.rebase_apply_dir().exists() || self.files.rebase_merge_dir().exists() {
            return RepositoryState::Rebasing;
        }
        if !is_on_branch {
            return RepositoryState::Detached;
        }
        if self.files.cherry_pick_head().exists() {
            return RepositoryState::Grafting;
        }
        if self.files.revert_head().exists() {
            return RepositoryState::Reverting;
        }
        if let Some(state) = self.sequencer_todo_based_state() {
            return state;
        }
        RepositoryState::Normal
    }

    /// Upstream `GitSequencerTodoReader`: cherry-pick/revert of several commits can be in progress
    /// with `CHERRY_PICK_HEAD`/`REVERT_HEAD` missing.
    fn sequencer_todo_based_state(&self) -> Option<RepositoryState> {
        let content = std::fs::read_to_string(self.files.sequencer_todo_file()).ok()?;
        if content.starts_with("pick") {
            Some(RepositoryState::Grafting)
        } else if content.starts_with("revert") {
            Some(RepositoryState::Reverting)
        } else {
            None
        }
    }

    fn find_rebase_branch(&self) -> Option<GitLocalBranch> {
        let name = [self.files.rebase_apply_dir(), self.files.rebase_merge_dir()]
            .iter()
            .filter(|dir| dir.exists())
            .find_map(|dir| try_load_file(&dir.join("head-name")).ok())?;
        (name != DETACHED_HEAD).then(|| GitLocalBranch::new(&name))
    }

    /// `None` means the repository uses the reftable format.
    fn read_head(&self) -> Option<HeadInfo> {
        let Ok(content) = try_load_file(&self.files.head_file()) else {
            return Some(HeadInfo::Unknown);
        };
        if content == "ref: refs/heads/.invalid" {
            return None;
        }
        if let Some(hash) = Hash::parse(&content) {
            return Some(HeadInfo::DetachedHead(hash));
        }
        match get_target(&content) {
            Some(target) => Some(HeadInfo::Branch(GitLocalBranch::new(&target))),
            None => Some(HeadInfo::Unknown),
        }
    }

    fn read_branches(&self, remotes: &[GitRemote]) -> GitBranches {
        // packed-refs first, so loose refs override them
        let mut data: HashMap<String, String> = HashMap::new();
        if let Ok(content) = try_load_file(&self.files.packed_refs()) {
            for line in content.lines() {
                if let Some((name, hash)) = parse_branches_line(line) {
                    data.insert(name, hash);
                }
            }
        }
        data.extend(read_from_refs_files(
            &self.files.refs_heads_dir(),
            REFS_HEADS_PREFIX,
        ));
        data.extend(read_from_refs_files(
            &self.files.refs_remotes_dir(),
            REFS_REMOTES_PREFIX,
        ));
        create_branches_from_data(remotes, resolve_refs(&data))
    }

    fn read_branches_from_git(&self, remotes: &[GitRemote]) -> (GitBranches, HeadInfo) {
        let root = self.files.root_dir();
        let mut h = self.git.handler(root, GitCommand::FOR_EACH_REF);
        h.add_parameters([
            "refs/heads/**",
            "refs/remotes/**",
            "--no-color",
            "--format=%(refname)\t%(objectname)\t%(HEAD)",
        ]);
        let result = self.git.run_command(h);
        if !result.success() {
            return (
                GitBranches {
                    local: HashMap::new(),
                    remote: HashMap::new(),
                },
                HeadInfo::Unknown,
            );
        }
        let mut resolved: HashMap<String, Hash> = HashMap::new();
        let mut head: Option<HeadInfo> = None;
        for line in &result.output {
            let mut parts = line.split('\t');
            let (Some(name), Some(hash)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Some(hash) = Hash::parse(hash) else {
                continue;
            };
            if parts.next() == Some("*") {
                head = Some(HeadInfo::Branch(GitLocalBranch::new(name)));
            }
            resolved.insert(name.to_string(), hash);
        }
        let branches = create_branches_from_data(remotes, resolved);
        if let Some(head) = head {
            return (branches, head);
        }
        if branches.local.is_empty() {
            // fresh repository: no commits yet, but HEAD names a branch
            if let Some(current) = self.resolve_current_branch() {
                return (
                    GitBranches {
                        local: HashMap::new(),
                        remote: branches.remote,
                    },
                    HeadInfo::Branch(current),
                );
            }
        }
        (branches, HeadInfo::Unknown)
    }

    fn resolve_current_branch(&self) -> Option<GitLocalBranch> {
        let mut h = self.git.handler(self.files.root_dir(), GitCommand::BRANCH);
        h.add_parameters(["--show-current", "--no-color"]);
        let output = self.git.run_command(h).output_or_error().ok()?;
        let output = output.trim();
        (!output.is_empty()).then(|| GitLocalBranch::new(output))
    }

    fn resolve_head_revision(&self) -> Option<Hash> {
        let mut h = self
            .git
            .handler(self.files.root_dir(), GitCommand::REV_PARSE);
        h.add_parameters(["--verify", "HEAD^{commit}"]);
        let output = self.git.run_command(h).output_or_error().ok()?;
        let output = output.trim();
        if output.len() == 40 || output.len() == 64 {
            Hash::parse(output)
        } else {
            None
        }
    }
}

fn fix_current_branch_case(branch: GitLocalBranch, branches: &GitBranches) -> GitLocalBranch {
    if branches.local.contains_key(&branch) || cfg!(target_os = "linux") {
        return branch;
    }
    branches
        .local
        .keys()
        .find(|b| b.name().eq_ignore_ascii_case(branch.name()))
        .cloned()
        .unwrap_or(branch)
}

fn create_branches_from_data(remotes: &[GitRemote], data: HashMap<String, Hash>) -> GitBranches {
    let mut local = HashMap::new();
    let mut remote = HashMap::new();
    for (ref_name, hash) in data {
        if ref_name.starts_with(REFS_HEADS_PREFIX) {
            local.insert(GitLocalBranch::new(&ref_name), hash);
        } else if ref_name.starts_with(REFS_REMOTES_PREFIX) {
            let branch = parse_remote_branch(&ref_name, remotes);
            if branch.name() == ORIGIN_HEAD {
                continue; // fake "origin/HEAD" reference
            }
            remote.insert(branch, hash);
        }
    }
    GitBranches { local, remote }
}
```

and update `crates/ig-git/src/lib.rs`:

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod branch;
pub mod command;
pub mod config;
pub mod console;
pub mod error;
pub mod handler;
pub mod hash;
pub mod reader;
pub mod refs;
pub mod repo_files;
pub mod runner;
pub mod status;
pub mod version;
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p ig-git --test reader_fixtures --test reader_integration`
Expected: PASS (`example1`, `symbolic_refs`, `reads_branch_detached_and_merge_states`, `reads_linked_worktree`).

In `symbolic-refs`, the `cycle1→cycle2→cycle3→cycle1` refs must be absent, while `master-link`, `simple-master-link` (`ref:master`), `transitive-link` and `origin-feature-link` must resolve. In `example1`, `origin/HEAD`, `refs/stash`, tags and the hidden files must be absent.

- [ ] **Step 7: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): repository state reader with upstream fixtures"
```

---
### Task 13: Git root detection

**Files:**
- Create: `crates/ig-git/src/roots.rs`
- Modify: `crates/ig-git/src/lib.rs`

Upstream (simplified): `platform/vcs-impl/src/com/intellij/openapi/vcs/roots/VcsRootDetectorImpl.java`. It finds the closest enclosing root plus nested roots (submodules, nested clones, worktrees checked out inside the folder) up to a depth, skipping `.git`, `node_modules`, `target`, `.idea` and `.ideagit`.

- [ ] **Step 1: Write the failing tests**: create `crates/ig-git/src/roots.rs` containing only the test module

```rust
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
```

and register the module in `crates/ig-git/src/lib.rs`:

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod branch;
pub mod command;
pub mod config;
pub mod console;
pub mod error;
pub mod handler;
pub mod hash;
pub mod reader;
pub mod refs;
pub mod repo_files;
pub mod roots;
pub mod runner;
pub mod status;
pub mod version;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ig-git --lib roots::`
Expected: FAIL. Compilation errors for the not-yet-defined items (`cannot find ...` / `unresolved import`).

- [ ] **Step 3: Implement**: insert this **above** the `#[cfg(test)]` module in `crates/ig-git/src/roots.rs`

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ig-git --lib roots::`
Expected: PASS (all tests in `roots::tests`).

- [ ] **Step 5: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): roots (Git root detection)"
```

---
### Task 14: Full verification

- [ ] **Step 1: Run the full suite, lints and formatting**

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

Expected: no fmt diff, no clippy warnings, and every test passing: 34 unit tests plus `reader_fixtures` (2), `reader_integration` (2), `runner_integration` (4), `status_integration` (1), `config_integration` (2).

- [ ] **Step 2: Push and check CI** (only if a remote exists and the user approved pushing). All three OS jobs must be green, and on Windows the `reader_fixtures` tests must pass with the byte-exact fixtures guaranteed by `.gitattributes`.

## Not in this plan (by design)

- Hooks info and shallow detection in the reader (Plan 4/8 need them).
- Rest of the `GitConfig` port: `insteadOf` substitution, branch tracking, hooks, core settings (Plan 8).
- Splitting long path lists into chunks for `status` (Plan 4).
- Askpass and credential handling (Plan 8).
- Minimum-git CI job (Plan 9).
- Runner cancellation, plus not hanging when a grandchild keeps a pipe open after git exits (ssh ControlPersist, backgrounded hooks). Do both in Plan 8 by polling `try_wait` with a reader grace period and making lock acquisition interruptible.
- Reporting read errors on the output pipes as a failed result, and `*_ignoring(codes)` variants of `output_or_error`/`throw_on_error` (add with the first caller).
- Test isolation for commands run inside library functions such as `get_file_status`. Integration tests can isolate handlers they build themselves (`common::isolate`). Library-created handlers still inherit the developer's global git config, so Plan 4 (commit) needs a test-only base environment on `Git`.
- CI: MSRV (1.85) check job and cargo caching (Plan 9).
