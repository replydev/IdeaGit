import os, sys
R = sys.argv[1]   # prototype root
OUT = sys.argv[2]
S = "crates/ig-git/src/"
T = "crates/ig-git/tests/"

def rd(p): return open(os.path.join(R, p)).read().rstrip("\n") + "\n"

def split_tests(p):
    s = rd(p)
    i = s.index("#[cfg(test)]")
    return s[:i].rstrip("\n") + "\n", s[i:]

LIB_HEAD = "//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.\n//! See `reference/upstream-map.toml` for the upstream files each module is ported from.\n\n"
mods = []
def lib_with(m):
    mods.append(m)
    return LIB_HEAD + "".join(f"pub mod {x};\n" for x in sorted(mods))

def code(lang, body): return f"```{lang}\n{body}```\n"

out = []
w = out.append

w("""# IdeaGit Plan 1 — Git Core Foundation (`ig-git`) Implementation Plan

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
git sparse-checkout set plugins/git4idea platform/vcs-log platform/vcs-impl/src/com/intellij/vcs \\
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
- Output lines are split on `\\n`, `\\r\\n` and `\\r`. Stderr progress lines (`looksLikeProgress`) are dropped from the collected error output.
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
  src/config.rs                    remotes from .git/config
  src/branch.rs                    GitLocalBranch, GitRemoteBranch, parse_remote_branch
  src/reader.rs                    GitRepositoryReader -> GitBranchState
  src/roots.rs                     detect_roots
  tests/common/mod.rs              real-git test helpers
  tests/runner_integration.rs
  tests/status_integration.rs
  tests/reader_fixtures.rs         ported GitRepositoryReaderTest
  tests/reader_integration.rs
  tests/fixtures/repo/{example1,symbolic-refs}   upstream test data
```

---
""")

# Task 1
lib1 = lib_with("error")
w("""### Task 1: Workspace scaffold, licensing, upstream pin, CI

**Files:**
- Create: `Cargo.toml`, `crates/ig-git/Cargo.toml`, `crates/ig-git/src/lib.rs`, `crates/ig-git/src/error.rs`
- Create: `.gitignore`, `.gitattributes`, `LICENSE`, `NOTICE`, `reference/UPSTREAM`, `reference/upstream-map.toml`, `.github/workflows/ci.yml`

- [ ] **Step 1: Create the workspace manifest** `Cargo.toml`

""" + code("toml", rd("Cargo.toml")) + """
- [ ] **Step 2: Create the crate manifest** `crates/ig-git/Cargo.toml`

""" + code("toml", rd("crates/ig-git/Cargo.toml")) + """
- [ ] **Step 3: Create `crates/ig-git/src/error.rs`**

""" + code("rust", rd(S+"error.rs")) + """
- [ ] **Step 4: Create `crates/ig-git/src/lib.rs`**

""" + code("rust", lib1) + """
- [ ] **Step 5: Create `.gitignore` and `.gitattributes`**

`.gitignore`:
""" + code("", rd(".gitignore")) + """
`.gitattributes`:
""" + code("", rd(".gitattributes")) + """
- [ ] **Step 6: Add `LICENSE` (Apache-2.0 text) and `NOTICE`**

Upstream's root `LICENSE.txt` is **not** the Apache text: since June 2026 it holds the "JetBrains Open-Source Build Terms", which cover JetBrains' binary builds. The source files are Apache-2.0 (see their headers), so copy the Apache-2.0 text that upstream ships:

```bash
git -C "$IJ" show HEAD:plugins/hg4idea/resources/META-INF/LICENSE-2.0.txt > LICENSE
head -2 LICENSE   # expected: "Apache License" / "Version 2.0, January 2004"
```

`NOTICE`:
""" + code("", rd("NOTICE")) + """
- [ ] **Step 7: Create `reference/UPSTREAM`**

""" + code("toml", rd("reference/UPSTREAM")) + """
- [ ] **Step 8: Create `reference/upstream-map.toml`** (entries for every module this plan adds)

""" + code("toml", rd("reference/upstream-map.toml")) + """
- [ ] **Step 9: Create `.github/workflows/ci.yml`**

""" + code("yaml", rd(".github/workflows/ci.yml")) + """
- [ ] **Step 10: Build**

Run: `cargo build`
Expected: `Finished` with no errors. `Cargo.lock` is created; commit it.

- [ ] **Step 11: Commit**

```bash
git add Cargo.toml Cargo.lock crates .gitignore .gitattributes LICENSE NOTICE reference .github
git commit -m "chore: scaffold Cargo workspace and ig-git crate"
```

---
""")

def module_task(n, title, mod, upstream_note, test_filter=None, extra_files_before="", extra_after=""):
    impl, tests = split_tests(S+mod+".rs")
    lib = lib_with(mod)
    tf = test_filter or (mod + "::")
    w(f"""### Task {n}: {title}

**Files:**
- Create: `crates/ig-git/src/{mod}.rs`
- Modify: `crates/ig-git/src/lib.rs`
{extra_files_before}
{upstream_note}

- [ ] **Step 1: Write the failing tests**: create `crates/ig-git/src/{mod}.rs` containing only the test module

""" + code("rust", tests) + f"""
and register the module in `crates/ig-git/src/lib.rs`:

""" + code("rust", lib) + f"""
- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ig-git --lib {tf}`
Expected: FAIL. Compilation errors for the not-yet-defined items (`cannot find ...` / `unresolved import`).

- [ ] **Step 3: Implement**: insert this **above** the `#[cfg(test)]` module in `crates/ig-git/src/{mod}.rs`

""" + code("rust", impl) + f"""
- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ig-git --lib {tf}`
Expected: PASS (all tests in `{mod}::tests`).
{extra_after}
- [ ] **Step 5: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): {mod} ({title.split(': ',1)[-1] if ': ' in title else title})"
```

---
""")

module_task(2, "Commit hash value", "hash",
  "Upstream: `platform/vcs-impl/shared/src/com/intellij/vcs/log/impl/HashImpl.java` (`build` accepts any non-empty hex string), used through `GitRefUtil.parseHash`.")
module_task(3, "Git version parsing and version specialties", "version",
  "Upstream: `$IJ/plugins/git4idea/backend/src/config/GitVersion.java`, `GitVersionSpecialty.kt`. The tests are ported from `plugins/git4idea/tests/git4idea/tests/GitVersionTest.java`. Note the upstream quirk: ordering ignores the type, but equality does not.")
module_task(4, "Git command catalogue and locking policy", "command",
  "Upstream: `$IJ/plugins/git4idea/backend/src/commands/GitCommand.java`. Keep the comments explaining why `FETCH`, `PUSH` and `CLONE` are read commands.")

# Task 5: console + handler
cons = rd(S+"console.rs")
impl, tests = split_tests(S+"handler.rs")
mods.append("console")
lib = lib_with("handler")
w("""### Task 5: Console events and the command handler

**Files:**
- Create: `crates/ig-git/src/console.rs`, `crates/ig-git/src/handler.rs`
- Modify: `crates/ig-git/src/lib.rs`

Upstream: `GitHandler.java` (constructor, `getConfigParameters`, `prepareEnvironment`, `printableCommandLine`), `GitLineHandler.java`, `platform/vcs-impl/src/com/intellij/vcs/VcsLocaleHelper.kt`. The console event shape mirrors `GitCommandOutputLogger` in `GitImplBase.java` (`showCommandStart`, `showCommandOutput`, `showCommandFinished`).

- [ ] **Step 1: Create `crates/ig-git/src/console.rs`** (plain data types, no logic to test)

""" + code("rust", cons) + """
- [ ] **Step 2: Write the failing handler tests**: create `crates/ig-git/src/handler.rs` containing only

""" + code("rust", tests) + """
and update `crates/ig-git/src/lib.rs`:

""" + code("rust", lib) + """
- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p ig-git --lib handler::`
Expected: FAIL. Compilation errors (`GitHandler`, `relative_path` and others not found).

- [ ] **Step 4: Implement**: insert above the test module in `handler.rs`

""" + code("rust", impl) + """
- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p ig-git --lib handler::`
Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): console events and GitHandler command line/environment"
```

---
""")

# Task 6 runner
impl, tests = split_tests(S+"runner.rs")
lib = lib_with("runner")
w("""### Task 6: Command runner, result, and console logging

**Files:**
- Create: `crates/ig-git/src/runner.rs`
- Create: `crates/ig-git/tests/common/mod.rs`, `crates/ig-git/tests/runner_integration.rs`
- Modify: `crates/ig-git/src/lib.rs`

Upstream: `GitImplBase.java` (`doRun`, `getGitTraceEnvironmentVariables`, `lock`/`shouldTakeWriteLock`, `writeOutputToConsole`, `GitCommandResultListener`, `looksLikeProgress`), and `GitCommandResult.java` (`success`, `getErrorOrStdOutput`, `getOutputOrThrow`, `throwOnError`).

- [ ] **Step 1: Write the failing unit tests**: create `crates/ig-git/src/runner.rs` containing only

""" + code("rust", tests) + """
and update `crates/ig-git/src/lib.rs`:

""" + code("rust", lib) + """
- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ig-git --lib runner::`
Expected: FAIL. Compilation errors (`LineSplitter`, `looks_like_progress` and others not found).

- [ ] **Step 3: Implement**: insert above the test module in `runner.rs`

""" + code("rust", impl) + """
- [ ] **Step 4: Run the unit tests**

Run: `cargo test -p ig-git --lib runner::`
Expected: PASS (4 tests).

- [ ] **Step 5: Add the real-git test helpers** `crates/ig-git/tests/common/mod.rs`

""" + code("rust", rd(T+"common/mod.rs")) + """
- [ ] **Step 6: Add the runner integration tests** `crates/ig-git/tests/runner_integration.rs`

""" + code("rust", rd(T+"runner_integration.rs")) + """
- [ ] **Step 7: Run them**

Run: `cargo test -p ig-git --test runner_integration`
Expected: PASS (4 tests). If `detects_supported_version` fails, the installed git is older than 2.17 (2.19.2 on Windows); upgrade git.

- [ ] **Step 8: Commit**

```bash
git add crates/ig-git
git commit -m "feat(ig-git): git runner with locking, trace env and console events"
```

---
""")

module_task(7, "`git status` porcelain v1 parsing and invocation", "status",
  "Upstream: `$IJ/plugins/git4idea/backend/src/index/GitIndexStatusUtil.kt` (`parseGitStatusOutput`, `appendParameters`, `getFileStatus`, the `is*` helpers, `LightFileStatus.StatusRecord`). Error message texts come from `GitBundle.properties` (`status.exception.message.*`).",
  extra_files_before="- Create: `crates/ig-git/tests/status_integration.rs`",
  extra_after="""
- [ ] **Step 4b: Add the status integration test** `crates/ig-git/tests/status_integration.rs`

""" + code("rust", rd(T+"status_integration.rs")) + """
Run: `cargo test -p ig-git --test status_integration`
Expected: PASS (1 test).
""")

module_task(8, "Repository files layout and `.git` discovery", "repo_files",
  "Upstream: `$IJ/plugins/git4idea/backend/src/repo/GitRepositoryFiles.java` (main vs worktree dir split, `getMainGitDirForWorktree`), `GitUtil.java` (`findGitDir`, `parsePathToRepository`), `platform/dvcs-impl/.../DvcsUtil.java` (`tryLoadFile`).")
module_task(9, "Ref parsing, symbolic resolution, loose refs, ref-name validation", "refs",
  "Upstream: `$IJ/plugins/git4idea/backend/src/repo/GitRefUtil.java`, `validators/GitRefNameValidator.java` (the Java regex uses look-arounds, which Rust `regex` lacks, so `check_ref_name` spells out each rule), and `shared/src/com/intellij/vcs/git/ref/GitRefUtil.kt` (`stripRefsPrefix`).")
module_task(10, "Remotes via `git config --null -l`", "config",
  "Upstream: `$IJ/plugins/git4idea/backend/src/config/GitConfigUtil.java` (`getValues`) and `$IJ/plugins/git4idea/backend/src/repo/GitConfig.kt` (`read`, `parseSectionNameAndVariable`, `createRemote`, the `urls.isNotEmpty()` filter). Git itself parses the config, so includes, escapes and continuation lines behave exactly as in git. `url.insteadOf`, branch tracking and hooks belong to Plan 8 (push/pull).",
  extra_files_before="- Create: `crates/ig-git/tests/config_integration.rs`",
  extra_after="""
- [ ] **Step 4b: Add the config integration test** `crates/ig-git/tests/config_integration.rs`

""" + code("rust", rd(T+"config_integration.rs")) + """
Run: `cargo test -p ig-git --test config_integration`
Expected: PASS (2 tests).
""")
module_task(11, "Local and remote branch types", "branch",
  "Upstream: `GitLocalBranch.kt`, `shared/src/git4idea/GitBranch.kt`, `GitStandardRemoteBranch.kt`, `GitSvnRemoteBranch.kt`, and `GitUtil.parseRemoteBranch` (the remote is the shortest `/`-prefix naming a known remote; unknown remotes fall back to the first segment).")

# Task 12 reader
lib = lib_with("reader")
w("""### Task 12: Repository state reader (with ported upstream fixtures)

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

""" + code("rust", rd(T+"reader_fixtures.rs")) + """
- [ ] **Step 3: Write the failing real-repository tests** `crates/ig-git/tests/reader_integration.rs`

""" + code("rust", rd(T+"reader_integration.rs")) + """
- [ ] **Step 4: Run them to verify they fail**

Run: `cargo test -p ig-git --test reader_fixtures --test reader_integration`
Expected: FAIL. `unresolved import ig_git::reader`.

- [ ] **Step 5: Implement** `crates/ig-git/src/reader.rs`

""" + code("rust", rd(S+"reader.rs")) + """
and update `crates/ig-git/src/lib.rs`:

""" + code("rust", lib) + """
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
""")

module_task(13, "Git root detection", "roots",
  "Upstream (simplified): `platform/vcs-impl/src/com/intellij/openapi/vcs/roots/VcsRootDetectorImpl.java`. It finds the closest enclosing root plus nested roots (submodules, nested clones, worktrees checked out inside the folder) up to a depth, skipping `.git`, `node_modules`, `target`, `.idea` and `.ideagit`.")

w("""### Task 14: Full verification and spec correction

**Files:**
- Modify: `docs/superpowers/specs/2026-11-08-ideagit-rust-design.md`

- [ ] **Step 1: Run the full suite, lints and formatting**

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

Expected: no fmt diff, no clippy warnings, and every test passing: 34 unit tests plus `reader_fixtures` (2), `reader_integration` (2), `runner_integration` (4), `status_integration` (1).

- [ ] **Step 2: Correct the spec where it disagreed with upstream.** In `docs/superpowers/specs/2026-11-08-ideagit-rust-design.md`:
  - Replace `` (`status --porcelain=v2 -z`) `` with `` (`status --porcelain -z`, format v1, as upstream) ``.
  - In the Key decisions table, replace `Same as upstream (currently 2.19+)` with `Same as upstream: 2.17.0 (2.19.2 on Windows)`.
  - In Licensing, add: "Upstream's root `LICENSE.txt` now holds the JetBrains Open-Source Build Terms (for JetBrains' binary builds). The source code is Apache-2.0 per its headers, and IdeaGit ships the Apache-2.0 text."

- [ ] **Step 3: Commit**

```bash
git add docs
git commit -m "docs: align spec with upstream status format, minimum git and license layout"
```

- [ ] **Step 4: Push and check CI** (only if a remote exists and the user approved pushing). All three OS jobs must be green.
""")

open(OUT, "w").write("".join(out))
print("written", OUT, sum(1 for _ in open(OUT)))
