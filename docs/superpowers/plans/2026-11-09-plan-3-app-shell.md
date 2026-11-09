# IdeaGit Plan 3 — App Shell and Read-Only Log Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The first runnable IdeaGit. It opens a folder, shows IntelliJ's New UI look (theme, icons, fonts, tool-window stripes), browses the Git history in a read-only **Log** tab with IntelliJ's commit graph, and shows the **Git Console**. It runs on macOS, Windows and Linux.

**Architecture:**
- `xtask import-ui` imports the UI resources from the pinned intellij-community commit into `assets/`: theme colors, `expui` icons, bundle strings, keymaps and fonts. It also generates the typed theme tokens in `ig-ui-kit`.
- `ig-ui-kit` is the IntelliJ look on GPUI, with no Git knowledge. It holds the theme, fonts, icons, New UI metrics and the graph palette (`DefaultColorGenerator`). Its widgets are the tool-window stripes and header, drawn through `IjDockRenderer` on gpui-component's dock, plus the status bar, banners and balloons, and table colors.
- `ig-app` is the `ideagit` binary:
  - the welcome and project windows, menus, the action registry and the keymap
  - the Git tool window, with a Log tab and a Console tab
  - the Log itself: the graph and label painters (ports of `SimpleGraphCellPainter` and `LabelPainter`) and one background log worker per project window, which owns `GitLogProvider`, `InMemoryStorage` and `VcsLogRefresher`
- Dependency direction: `ig-app` → `ig-ui-kit`, `ig-git`, `ig-log`.

**Tech Stack:** Rust 2024, MSRV 1.95. Zed's `gpui` and `gpui_platform` come from git, with the Zed commit pinned in `Cargo.lock` only. `gpui-component` is pinned at gpui-kit `c0946e6` (0.5.2).

**Spec:** `docs/superpowers/specs/2026-11-08-app-shell-design.md`. **Previous plans:** 1 (`ig-git` core), 2 (`ig-log` graph core), 2c (git log data layer).

> **Note:** The code blocks below are the verified prototype. Review fixes applied during execution live in the crates and in `reference/upstream-map.toml`, and those are the source of truth.

---

## How this plan is executed

- **Source of every file:** the prototype branch `origin/wip/app-shell-prototype`.
  - Each task copies its files from that branch with `git checkout origin/wip/app-shell-prototype -- <paths>`, then checks the copy with `git diff --cached --exit-code`. The file contents are also embedded below for review.
  - Some files exist in a smaller intermediate form until a later task completes them: crate roots, `log/mod.rs`, `main.rs` and `tests/ui.rs`. The plan gives those intermediate files in full; write them exactly.
  - Binary files (fonts, fixture fonts, screenshots) and the generated assets are not embedded. They come from the branch, or from `cargo xtask import-ui`.
  - `README.md` and `docs/app-shell-checklist.md` are not on the branch. Task 11 gives them in full.
- **Simulated:** the task order was replayed on a fresh clone of `master`. Every step was green with fmt, clippy (with and without the `screenshots` feature), docs and tests. The expected test counts in each task come from that run, on Rust 1.96 and macOS arm64.
- **Always build with `--locked`.** The Zed commit is pinned only in `Cargo.lock`. A `rev =` in `Cargo.toml` would create a second gpui source, because gpui-kit asks for Zed's default branch. Never run `cargo update`. On this machine, fetching git dependencies needs `CARGO_NET_GIT_FETCH_WITH_CLI=true` because the global git config rewrites HTTPS to SSH.
- **`runtime_shaders`.** The `gpui_platform` dependency enables it, so Macs without full Xcode build without the Metal shader compiler. Keep it.
- **Never glob-import `gpui` in test modules** (`use gpui::*;`). `#[gpui::test]` then expands recursively and crashes rustc. Import the names you need.
- **The first CI run is the spike for Windows and Linux.**
  - Windows: Zed's gpui needs `llvm-rc` for its embedded manifest. If the job fails there, install LLVM in the workflow (`choco install llvm`, or put the runner's preinstalled `llvm-rc` on `PATH`).
  - Linux: the system-package list in `ci.yml` was derived from the `-sys` crates in `Cargo.lock` and has never run.
  - Fix CI in a follow-up commit on the branch before merging. Record anything learned in `docs/tech-debt.md`.

## Scope

- **In scope:**
  - **Windows:** the welcome window, with Open…, recent projects, Clone shown disabled, and the git-missing banner with a git path field. One project window per opened folder, covering all git roots it contains.
  - **Chrome and menus:**
    - New UI tool-window stripes and docking.
    - A Git tool window with Log and Console tabs.
    - A status bar showing each root's branch.
    - Menus: native on macOS, in-window on Windows and Linux. File, View (including "Show read commands in Console"), Git → Refresh Log, Help → About.
  - **Read-only Log:**
    - Columns: graph and subject with ref labels, author, date, hash.
    - A virtualized table with keyboard selection and multi-select.
    - Long-edge arrow hover and click-to-jump.
    - Refresh on demand, on window focus, and after IdeaGit's own commands.
  - **Git Console:** IntelliJ's format, non-silent commands only, plus the debug toggle.
  - **Theme:** light and dark, following the OS.
  - **Screenshots:** `cargo xtask screenshots` renders the Log headless.
- **Out of scope (later plans):** the details pane, changes tree, filters and context actions; editor and diff; the commit tool window; the branches popup; fetch, pull and push; clone; file watching; settings UI beyond the git path; packaging.

## Tests

- **`xtask`:** unit tests for the XML reader, theme loading, token resolution, keymaps and strings. The `import_ui` integration test runs the import against a fixture upstream tree under `xtask/tests/fixtures/upstream`, including the missing-key failure that leaves the workspace untouched.
- **`ig-ui-kit`:** palette mapping, checked against `DefaultColorGenerator` values derived by hand; metrics; icon lookup.
- **`ig-app`, unit tests:** strings, settings, the keymap, console formatting, relative dates, ref groups, selection, the metadata cache, and the graph and label painter geometry.
- **`ig-app`, headless UI tests** (`tests/ui.rs`, GPUI `TestAppContext`, no display or GPU): the Log loads, selects and scrolls; an arrow click jumps; Refresh reaches the worker; the no-repositories state; the Git tool window toggles.
- **`ig-git`, `ig-log`:** one IdeaGit test each, for `silent_command` and `is_terminal`.
- **By hand:** `cargo xtask screenshots` and `docs/app-shell-checklist.md`.

## Deviations (also recorded in `reference/upstream-map.toml`)

- **Shared crates:** `EdgePrintElementImpl::is_terminal` replaces an upstream `instanceof`, and equality ignores it. `ConsoleSink::silent_command` reports read commands; upstream never logs them.
- **Import:**
  - Only `os.default` theme values are used. A missing color key falls back to the longest matching `*` pattern; upstream takes the first match in hash order.
  - Hard-coded upstream fallbacks are literals in `assets/theme.keys`, each citing its source.
  - Icons are picked explicitly in `assets/icons.list`.
  - Strings are copied raw.
  - Keymap: mouse and gesture shortcuts are ignored.
- **Look:**
  - The theme follows the OS appearance, with no chooser.
  - Status-bar metrics are estimates.
  - Stripes have no drag and drop and no names under icons.
  - The dock has no gear menu, view modes or tab drag.
  - Balloons have no animation, timeout or event log.
- **Log:**
  - The table is our own, on GPUI's `uniform_list`. Column widths are fixed, and there is no Root column, current-branch highlight or tooltips.
  - Selection is kept as a set of commits.
  - Dates use the en_US short format.
  - Labels are right-aligned only, with no bookmarks.
  - Tracked-branch groups are matched by name only.
  - Painting: the dash pattern has no phase, and OUTLINE nodes use the ordinary line thickness.
- **Worker:**
  - Two threads (the refresher, and snapshots with metadata) replace upstream's coroutine jobs.
  - A LIFO metadata queue keeps the newest 10 requests.
- **App:**
  - One `settings.toml` replaces `recentProjects.xml` and `git.xml`.
  - Only implemented actions are bound.
  - The console has no ANSI decoding and a bounded buffer.

## Known gaps (kept for later plans)

- **Log table:** column resize, the Root column, current-branch highlighting and tooltips. These belong to the Log UI plan.
- **Git missing after startup:** a missing git is shown only in the welcome window. A project window opened from the command line with no git gets no notification. Schedule this with the settings work.
- **Never checked by eye:** the welcome window and the native menus. `docs/app-shell-checklist.md` covers them; run it once on macOS before merging.
- **Windows and Linux** are unverified until the first CI run (see above).

## Notes for the Log UI plan

- **Worker interface:** the worker (`ig_app::log::worker`) sends immutable `LogSnapshot`s. The model (`ig_app::log::model::LogModel`) owns the selection and the metadata cache.
  - Details and changes should be a second request kind on the same worker, not a second refresher.
- **Arrow actions:** these go through `ActionController` on the visible graph. Re-fetch print elements after any answer with `is_repaint_required`. Plan 2b's BEK, filters and collapsing plug in at the `VisibleGraph` built per snapshot.
- **Refresh:** triggers are coalesced in the worker. File watching should send the same refresh request, and force a full reload on `.git/shallow`, grafts or `refs/replace` changes.
- **Deferred decision:** the tag-fetch refresh strategy, from Plan 2c's notes. Each new tag still costs one `git log`.

## Task order

1. Workspace and MSRV (this makes the lock final).
2. `xtask import-ui` and the assets.
3. `ig-ui-kit` logic.
4. `ig-ui-kit` widgets.
5. The small `ig-log` / `ig-git` additions, which `ig-app`'s console and graph painter use.
6. `ig-app` basics.
7. Log painters and helpers.
8. Worker, model and headless UI tests.
9. View, windows, menus and `main`. These modules import each other, so they land together.
10. Screenshots.
11. Map, README, checklist and full verification.

---

### Task 1: Workspace, MSRV 1.95, crate skeletons and CI

**Files:**
- Create: `.cargo/config.toml`, `crates/ig-ui-kit/Cargo.toml`, `crates/ig-app/Cargo.toml`, `xtask/Cargo.toml`, `crates/ig-ui-kit/src/lib.rs`, `crates/ig-app/src/lib.rs`, `crates/ig-app/src/main.rs`, `xtask/src/lib.rs`, `xtask/src/main.rs`
- Modify: `Cargo.toml`, `.gitignore`, `.github/workflows/ci.yml`, `Cargo.lock`, `crates/ig-git/src/config.rs`, `crates/ig-git/src/log/util.rs`, `crates/ig-git/src/refs.rs`, `crates/ig-git/tests/log_perf.rs`, `crates/ig-log/src/permanent/permanent_commits_info_impl.rs`, `crates/ig-log/src/utils/flags.rs`

**Review focus:** the `Cargo.toml` pin comments (Zed rev only in `Cargo.lock`), the CI package list, and the let-chain lint fixes (behavior unchanged).

This task makes `Cargo.lock` final. The three new crates get their final manifests and empty sources, so every later task builds with `--locked` against the same dependency tree. It is also the first gpui build, which takes several minutes.

- [ ] **Step 1: Workspace files.** `Cargo.toml` adds the three members and the gpui dependencies, and raises `rust-version` to 1.95. `.cargo/config.toml` adds the `cargo xtask` alias. CI installs GPUI's Linux packages, uses `--locked`, lints the `screenshots` feature, and moves the MSRV job to 1.95.

```bash
git checkout origin/wip/app-shell-prototype -- Cargo.toml .cargo/config.toml .gitignore .github/workflows/ci.yml
```

`Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/ig-git", "crates/ig-log", "crates/ig-ui-kit", "crates/ig-app", "xtask"]
default-members = ["crates/ig-git", "crates/ig-log", "crates/ig-ui-kit", "crates/ig-app", "xtask"]

[workspace.package]
edition = "2024"
license = "Apache-2.0"
# 1.95: Zed's gpui (f66ed399) uses `std::hint::cold_path`, stable since 1.95 (1.94 fails).
rust-version = "1.95"

[workspace.dependencies]
regex = "1"
thiserror = "2"
tempfile = "3"
# GPUI: Zed's official gpui from git, and gpui-component from the gpui-kit repository at the last
# commit that builds against Zed's git gpui (later gpui-kit commits use the third-party `gpui-pre`
# snapshots).
#
# Exact pins:
# - gpui-kit: rev c0946e6acdc9e2f984f317ef7f998ee2c79f1a87 (below).
# - Zed: f66ed399cdde86092af8af3dc7b418abf45f37f8 (the rev gpui-kit's own Cargo.lock had at that
#   commit), pinned in Cargo.lock, NOT here: gpui-kit asks for Zed's default branch without a rev,
#   and a `rev = ...` here would be a different source, i.e. a second gpui in the tree. A [patch]
#   can't fix that either (Cargo ignores the rev when comparing patch sources: "patch for `gpui`
#   points to the same source"). Builds must use `--locked`; bump with
#   `cargo update -p gpui --precise <zed sha>` (moves every Zed crate together).
gpui = { git = "https://github.com/zed-industries/zed", version = "=0.2.2" }
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit", "runtime_shaders", "wayland", "x11"] }
gpui-component = { git = "https://github.com/longbridge/gpui-kit", rev = "c0946e6acdc9e2f984f317ef7f998ee2c79f1a87" }
```

`.cargo/config.toml`:

```toml
[alias]
xtask = "run -q -p xtask --"
```

`.gitignore`:

```text
/target
/target-*
```

`.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main, master]
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
      # GPUI's Linux backends (X11, Wayland, Vulkan via blade/wgpu, fontconfig/freetype text).
      # Package list derived from the -sys crates in Cargo.lock; not yet verified on CI.
      - if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y pkg-config libxkbcommon-dev libxkbcommon-x11-dev \
            libwayland-dev libvulkan-dev libfontconfig-dev libfreetype-dev \
            libx11-xcb-dev libxcb1-dev
      - run: cargo fmt --all --check
      # --locked: the Zed rev is pinned only in Cargo.lock (see Cargo.toml)
      - run: cargo clippy --locked --all-targets -- -D warnings
      - run: cargo clippy --locked -p ig-app --all-targets --features screenshots -- -D warnings
      - run: cargo doc --no-deps --all
        env:
          RUSTDOCFLAGS: -D warnings
      # The GPUI UI tests use TestAppContext (no display, no GPU); `cargo xtask screenshots`
      # needs a GPU and is run by hand.
      - run: cargo test --locked --all

  msrv:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      # Zed's gpui needs 1.95 (`std::hint::cold_path`); see Cargo.toml.
      - uses: dtolnay/rust-toolchain@1.95
      - run: |
          sudo apt-get update
          sudo apt-get install -y pkg-config libxkbcommon-dev libxkbcommon-x11-dev \
            libwayland-dev libvulkan-dev libfontconfig-dev libfreetype-dev \
            libx11-xcb-dev libxcb1-dev
      - run: cargo check --locked --all --all-targets
```

- [ ] **Step 2: `Cargo.lock`.** Take it from the prototype. Never regenerate it: it is the only place the Zed rev is pinned.

```bash
git checkout origin/wip/app-shell-prototype -- Cargo.lock
```

- [ ] **Step 3: Final crate manifests.** `ig-app` builds the `ideagit` binary. Its optional `screenshots` feature turns on GPUI test support and `image`.

```bash
git checkout origin/wip/app-shell-prototype -- crates/ig-ui-kit/Cargo.toml crates/ig-app/Cargo.toml xtask/Cargo.toml
```

`crates/ig-ui-kit/Cargo.toml`:

```toml
[package]
name = "ig-ui-kit"
version = "0.1.0"
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[dependencies]
gpui.workspace = true
gpui-component.workspace = true
```

`crates/ig-app/Cargo.toml`:

```toml
[package]
name = "ig-app"
version = "0.1.0"
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[[bin]]
name = "ideagit"
path = "src/main.rs"

[features]
# `cargo xtask screenshots`: headless rendering of the Log to PNG (GPUI test support).
screenshots = ["gpui/test-support", "gpui_platform/test-support", "dep:image"]

[dependencies]
ig-git = { path = "../ig-git" }
ig-log = { path = "../ig-log" }
ig-ui-kit = { path = "../ig-ui-kit" }
gpui.workspace = true
gpui_platform.workspace = true
gpui-component.workspace = true
# All of the following are already in GPUI's dependency tree (same versions): no new crates.
chrono = { version = "0.4", default-features = false, features = ["clock", "std"] }
dirs = "6"
flume = "0.12"
log = "0.4"
lyon = "1"
serde = { version = "1", features = ["derive"] }
toml = "0.8"
image = { version = "0.25", default-features = false, features = ["png"], optional = true }

[dev-dependencies]
gpui = { workspace = true, features = ["test-support"] }
tempfile.workspace = true
```

`xtask/Cargo.toml`:

```toml
[package]
name = "xtask"
version = "0.1.0"
edition.workspace = true
license.workspace = true
rust-version.workspace = true
publish = false

[dependencies]
# `preserve_order`: theme JSON key order matters (later `*` patterns override earlier keys).
serde_json = { version = "1", features = ["preserve_order"] }

[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 4: Skeleton:** `crates/ig-ui-kit/src/lib.rs`. Write exactly:

```rust
//! IntelliJ New UI look for IdeaGit. The modules land in the next tasks of the app-shell plan.
```

- [ ] **Step 5: Skeleton:** `crates/ig-app/src/lib.rs`. Write exactly:

```rust
//! IdeaGit's application. The modules land in the next tasks of the app-shell plan.
```

- [ ] **Step 6: Skeleton:** `crates/ig-app/src/main.rs`. Write exactly:

```rust
//! `cargo run -p ig-app -- [path]`. The application lands in a later task of the app-shell plan.

fn main() {}
```

- [ ] **Step 7: Skeleton:** `xtask/src/lib.rs`. Write exactly:

```rust
//! Developer tasks for IdeaGit. `import-ui` lands in the next task of the app-shell plan.
```

- [ ] **Step 8: Skeleton:** `xtask/src/main.rs`. Write exactly:

```rust
fn main() {}
```

- [ ] **Step 9: Lint fixes.** With `rust-version = 1.95`, clippy's `collapsible_if` suggests let chains and `manual_is_multiple_of` fires. These are mechanical rewrites with no behavior change.

```bash
git checkout origin/wip/app-shell-prototype -- crates/ig-git/src/config.rs crates/ig-git/src/log/util.rs crates/ig-git/src/refs.rs crates/ig-git/tests/log_perf.rs crates/ig-log/src/permanent/permanent_commits_info_impl.rs crates/ig-log/src/utils/flags.rs
```

`crates/ig-git/src/config.rs`:

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
        if let Some(sv) = parse_section_name_and_variable(key)
            && !sections.contains(&sv)
        {
            sections.push(sv);
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

`crates/ig-git/src/log/util.rs`:

```rust
//! Reading commits, metadata and refs with `git log`.
//!
//! Port of the timed-commit, metadata and ref parts of
//! `plugins/git4idea/backend/src/history/GitLogUtil.java`, plus `GitLogProvider.DONT_CONSIDER_SHA`
//! and `GitBranchUtil.getAllTags` (`plugins/git4idea/backend/src/branch/GitBranchUtil.java`).
//!
//! Not ported here: full details with changes (`readFullDetails*`, `GitFullDetailsCollector`,
//! `GitCommitRequirements`), file paths for filters.

use std::collections::HashSet;
use std::path::Path;

use ig_log::data::{
    DetailedLogData, Hash, TimedVcsCommit, VcsCommitMetadata, VcsRef, VcsRefType, VcsRoot, VcsUser,
};

use crate::branch::ORIGIN_HEAD;
use crate::command::GitCommand;
use crate::error::{GitError, Result};
use crate::handler::GitHandler;
use crate::log::parser::{GitLogOption, GitLogParser, NameStatus};
use crate::log::record::{DefaultGitLogRecordBuilder, GitLogRecord};
use crate::log::ref_manager::get_ref_type;
use crate::log::splitter::run_and_split;
use crate::refs::strip_refs_prefix;
use crate::runner::Git;
use crate::version::{GitVersion, GitVersionSpecialty};

pub const GRAFTED: &str = "grafted";
pub const REPLACED: &str = "replaced";
/// Equivalent to `--all`, without the index and the stash (`LOG_ALL`).
pub const LOG_ALL: [&str; 4] = ["HEAD", "--branches", "--remotes", "--tags"];
pub const STDIN: &str = "--stdin";
pub const COMMIT_METADATA_OPTIONS: [GitLogOption; 11] = [
    GitLogOption::Hash,
    GitLogOption::Parents,
    GitLogOption::CommitTime,
    GitLogOption::CommitterName,
    GitLogOption::CommitterEmail,
    GitLogOption::AuthorName,
    GitLogOption::AuthorTime,
    GitLogOption::AuthorEmail,
    GitLogOption::Subject,
    GitLogOption::Body,
    GitLogOption::RawBody,
];

/// `GitVersionSpecialty.STARTED_USING_RAW_BODY_IN_FORMAT`.
pub fn supports_raw_body(version: &GitVersion) -> bool {
    GitVersionSpecialty::StartedUsingRawBodyInFormat.exists_in(version)
}

/// `createDefaultParser(project, options)`.
pub fn create_default_parser(
    git: &Git,
    options: &[GitLogOption],
) -> GitLogParser<DefaultGitLogRecordBuilder> {
    GitLogParser::new(
        DefaultGitLogRecordBuilder,
        supports_raw_body(&git.version()),
        NameStatus::None,
        options,
    )
}

/// `getNoWalkParameter`.
pub fn get_no_walk_parameter(version: &GitVersion) -> &'static str {
    if GitVersionSpecialty::NoWalkUnsorted.exists_in(version) {
        "--no-walk=unsorted"
    } else {
        "--no-walk"
    }
}

/// `createGitHandler`: a `git log` handler for `root`.
pub fn create_git_handler(git: &Git, root: &Path, config_parameters: &[&str]) -> GitHandler {
    git.handler_with_config(root, GitCommand::LOG, config_parameters)
}

/// `sendHashesToStdin`: one hash per line, then an empty line.
pub fn send_hashes_to_stdin(hashes: &[String], handler: &mut GitHandler) {
    let mut input = String::new();
    for hash in hashes {
        input.push_str(hash);
        input.push('\n');
    }
    input.push('\n');
    handler.set_input(input.into_bytes());
}

fn build_hash(s: &str) -> Result<Hash> {
    Hash::try_build(s).map_err(|e| GitError::Command(e.to_string()))
}

fn build_hashes(hashes: &[&str]) -> Result<Vec<Hash>> {
    hashes.iter().map(|h| build_hash(h)).collect()
}

/// `readTimedCommits`: runs `git log` with `parameters` (a `--` in them separates paths) and
/// reports each commit, plus its author and refs when the consumers are given.
pub fn read_timed_commits(
    git: &Git,
    root: &VcsRoot,
    config_parameters: &[&str],
    parameters: &[&str],
    mut user_consumer: Option<&mut dyn FnMut(VcsUser)>,
    mut ref_consumer: Option<&mut dyn FnMut(VcsRef)>,
    commit_consumer: &mut dyn FnMut(TimedVcsCommit),
) -> Result<()> {
    let mut handler = create_git_handler(git, root, config_parameters);
    let mut options = vec![
        GitLogOption::Hash,
        GitLogOption::Parents,
        GitLogOption::CommitTime,
    ];
    if user_consumer.is_some() {
        options.push(GitLogOption::AuthorName);
        options.push(GitLogOption::AuthorEmail);
    }
    if ref_consumer.is_some() {
        options.push(GitLogOption::RefNames);
    }
    let mut parser = create_default_parser(git, &options);
    handler.set_stdout_suppressed(true);
    handler.add_parameters([parser.get_pretty(), "--encoding=UTF-8"]);
    handler.add_parameters(["--decorate=full"]);
    match parameters.iter().position(|p| *p == "--") {
        Some(index) => {
            handler.add_parameters(parameters[..index].iter().copied());
            handler.end_options();
            handler.add_parameters(parameters[index + 1..].iter().copied());
        }
        None => {
            handler.add_parameters(parameters.iter().copied());
            handler.end_options();
        }
    }

    let mut error = None;
    run_and_split(git, handler, &mut parser, |record| {
        if error.is_some() {
            return;
        }
        let converted = (|| -> Result<()> {
            let hash = build_hash(record.hash())?;
            let parents = build_hashes(&record.parents_hashes())?;
            commit_consumer(TimedVcsCommit::new(
                hash.clone(),
                parents,
                record.commit_time(),
            ));
            if let Some(ref_consumer) = ref_consumer.as_mut() {
                for vcs_ref in parse_refs(&record.refs(), &hash, root) {
                    ref_consumer(vcs_ref);
                }
            }
            if let Some(user_consumer) = user_consumer.as_mut() {
                user_consumer(VcsUser::create_user(
                    record.author_name(),
                    record.author_email(),
                ));
            }
            Ok(())
        })();
        if let Err(e) = converted {
            error = Some(e);
        }
    })?;
    error.map_or(Ok(()), Err)
}

/// `collectMetadata(project, root, hashes, consumer)`: the metadata of the given commits, read
/// with `git log --no-walk --stdin`.
pub fn collect_metadata(
    git: &Git,
    root: &VcsRoot,
    hashes: &[String],
    consumer: &mut dyn FnMut(VcsCommitMetadata),
) -> Result<()> {
    if hashes.is_empty() {
        return Ok(());
    }
    let mut handler = create_git_handler(git, root, &[]);
    let mut parser = create_default_parser(git, &COMMIT_METADATA_OPTIONS);
    handler.set_silent(true);
    // git show can show either -p, or --name-status, or --name-only, but we need nothing, just
    // details => using git log --no-walk
    handler.add_parameters([get_no_walk_parameter(&git.version())]);
    handler.add_parameters([parser.get_pretty(), "--encoding=UTF-8"]);
    handler.add_parameters([STDIN]);
    handler.end_options();
    send_hashes_to_stdin(hashes, &mut handler);

    let mut error = None;
    run_and_split(git, handler, &mut parser, |record| {
        if error.is_none() {
            match create_metadata(root, &record) {
                Ok(metadata) => consumer(metadata),
                Err(e) => error = Some(e),
            }
        }
    })?;
    error.map_or(Ok(()), Err)
}

/// `collectMetadata(project, root, params...)`: commits with metadata and the refs on them, read
/// with `git log <params> --decorate=full`. Refs are unique by name and type. If the command
/// fails after some commits were read, those are returned (upstream logs a warning).
pub fn collect_metadata_with_params(
    git: &Git,
    root: &VcsRoot,
    params: &[&str],
) -> Result<DetailedLogData> {
    let mut refs = RefsByNameAndType::default();
    let mut commits = Vec::new();
    let result = (|| -> Result<()> {
        let mut handler = create_git_handler(git, root, &[]);
        let mut options = COMMIT_METADATA_OPTIONS.to_vec();
        options.push(GitLogOption::RefNames);
        let mut parser = create_default_parser(git, &options);
        handler.set_stdout_suppressed(true);
        handler.add_parameters(params.iter().copied());
        handler.add_parameters([parser.get_pretty(), "--encoding=UTF-8"]);
        handler.add_parameters(["--decorate=full"]);
        handler.end_options();
        let mut error = None;
        run_and_split(git, handler, &mut parser, |record| {
            if error.is_some() {
                return;
            }
            match create_metadata(root, &record) {
                Ok(commit) => {
                    for vcs_ref in parse_refs(&record.refs(), commit_id(&commit), root) {
                        // upstream logs "Adding duplicate element" and keeps the first
                        refs.add(vcs_ref);
                    }
                    commits.push(commit);
                }
                Err(e) => error = Some(e),
            }
        })?;
        error.map_or(Ok(()), Err)
    })();
    if let Err(e) = result
        && commits.is_empty()
    {
        return Err(e);
    }
    Ok(DetailedLogData {
        commits,
        refs: refs.into_vec(),
    })
}

fn commit_id(commit: &VcsCommitMetadata) -> &Hash {
    use ig_log::GraphCommit;
    commit.id()
}

/// `parseRefs`: refs of a `%d` decoration, without `grafted`, `replaced` and `origin/HEAD`.
pub fn parse_refs(refs: &[String], hash: &Hash, root: &VcsRoot) -> Vec<VcsRef> {
    refs.iter()
        .filter(|name| *name != GRAFTED && *name != REPLACED)
        .filter_map(|ref_name| {
            let ref_type: VcsRefType = get_ref_type(ref_name);
            let name = strip_refs_prefix(ref_name);
            (name != ORIGIN_HEAD).then(|| VcsRef::new(hash.clone(), name, ref_type, root.clone()))
        })
        .collect()
}

/// `createMetadata`.
pub fn create_metadata(root: &VcsRoot, record: &GitLogRecord) -> Result<VcsCommitMetadata> {
    Ok(VcsCommitMetadata::new(
        build_hash(record.hash())?,
        build_hashes(&record.parents_hashes())?,
        record.commit_time(),
        root.clone(),
        record.subject().to_string(),
        VcsUser::create_user(record.author_name(), record.author_email()),
        record.full_message(),
        VcsUser::create_user(record.committer_name(), record.committer_email()),
        record.author_time_stamp(),
    ))
}

/// `GitBranchUtil.getAllTags`: `git tag -l --no-column`.
pub fn get_all_tags(git: &Git, root: &Path) -> Result<Vec<String>> {
    let mut handler = git.handler(root, GitCommand::TAG);
    // `--no-column`: `column.ui=always` would print several tags per line.
    handler.add_parameters(["-l", "--no-column"]);
    handler.set_silent(true);
    let mut tags = Vec::new();
    let result = git.run_command_without_collecting_output(handler, &mut |line, kind| {
        if kind == crate::console::OutputKind::Stdout && !line.is_empty() {
            tags.push(line.to_string());
        }
    });
    result.throw_on_error()?;
    Ok(tags)
}

/// A set of refs unique by name and type, in insertion order (upstream
/// `ObjectOpenCustomHashSet(GitLogProvider.DONT_CONSIDER_SHA)`).
#[derive(Default, Debug, Clone)]
pub struct RefsByNameAndType {
    keys: HashSet<(String, VcsRefType)>,
    refs: Vec<VcsRef>,
}

impl RefsByNameAndType {
    pub fn from_refs(refs: impl IntoIterator<Item = VcsRef>) -> Self {
        let mut set = RefsByNameAndType::default();
        for r in refs {
            set.add(r);
        }
        set
    }

    /// Adds `vcs_ref` unless a ref with the same name and type is present; returns whether added.
    pub fn add(&mut self, vcs_ref: VcsRef) -> bool {
        if self
            .keys
            .insert((vcs_ref.name().to_string(), vcs_ref.ref_type()))
        {
            self.refs.push(vcs_ref);
            true
        } else {
            false
        }
    }

    pub fn contains(&self, vcs_ref: &VcsRef) -> bool {
        self.keys
            .contains(&(vcs_ref.name().to_string(), vcs_ref.ref_type()))
    }

    pub fn iter(&self) -> impl Iterator<Item = &VcsRef> {
        self.refs.iter()
    }

    pub fn into_vec(self) -> Vec<VcsRef> {
        self.refs
    }
}
```

`crates/ig-git/src/refs.rs`:

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
        if check_ref_name(&branch_name)
            && let Ok(value) = try_load_file(&path)
        {
            result.insert(branch_name, value);
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

`crates/ig-git/tests/log_perf.rs`:

```rust
//! IdeaGit performance smoke test: a synthetic repository of ~50,000 commits with feature
//! branches, merges and tags (generated with `git fast-import`), loaded into the log.
//!
//! Ignored by default. Run in release mode to see the timings:
//! `cargo test --release -p ig-git --test log_perf -- --ignored --nocapture`

mod common;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::log_helpers::{commit_at, provider};
use common::{git, git_command};
use ig_log::data::graph_data::build_data;
use ig_log::data::refresher::compact_commit;
use ig_log::data::{
    InMemoryStorage, LogProviders, RootRefsModel, VcsLogProvider, VcsLogRefresher, VcsLogStorage,
    vcs_root,
};
use ig_log::{Options, PermanentGraph, VisibleGraph};

const TOTAL_COMMITS: usize = 50_000;

struct Stream {
    s: String,
    mark: usize,
    time: i64,
}

impl Stream {
    fn commit(&mut self, r: &str, from: Option<usize>, merge: Option<usize>, msg: &str) -> usize {
        self.mark += 1;
        self.time += 1;
        let (mark, time) = (self.mark, self.time);
        self.s.push_str(&format!(
            "commit {r}\nmark :{mark}\ncommitter Perf <perf@example.com> {time} +0000\ndata {}\n{msg}\n",
            msg.len()
        ));
        if let Some(from) = from {
            self.s.push_str(&format!("from :{from}\n"));
        }
        if let Some(merge) = merge {
            self.s.push_str(&format!("merge :{merge}\n"));
        }
        self.s.push_str(&format!(
            "M 644 inline f{}\ndata 2\n{}\n\n",
            mark % 97,
            mark % 10
        ));
        mark
    }
}

/// Writes a fast-import stream: a main line where every 40th commit forks a feature branch of
/// 3..20 commits; two of three features are merged back, the others stay as branches. A tag every
/// 1000 main commits. Returns the stream, the number of commits and of branch refs.
fn fast_import_stream() -> (String, usize, usize) {
    let mut st = Stream {
        s: String::with_capacity(TOTAL_COMMITS * 120),
        mark: 0,
        time: 1_500_000_000,
    };
    let mut main_head: Option<usize> = None;
    let mut branches = 0;
    let mut main_count = 0usize;
    while st.mark < TOTAL_COMMITS {
        main_count += 1;
        main_head = Some(st.commit(
            "refs/heads/master",
            main_head,
            None,
            &format!("main {main_count}"),
        ));
        if main_count.is_multiple_of(40) {
            let feature = main_count / 40;
            let name = format!("refs/heads/feature/{feature}");
            let len = 3 + feature % 18;
            let mut head = main_head;
            for k in 0..len {
                head = Some(st.commit(&name, head, None, &format!("feature {feature} commit {k}")));
            }
            if !feature.is_multiple_of(3) {
                main_head = Some(st.commit(
                    "refs/heads/master",
                    main_head,
                    head,
                    &format!("merge feature {feature}"),
                ));
                st.s.push_str(&format!(
                    "reset {name}\nfrom 0000000000000000000000000000000000000000\n\n"
                ));
            } else {
                branches += 1;
            }
        }
        if main_count.is_multiple_of(1000) {
            st.s.push_str(&format!(
                "reset refs/tags/v{}\nfrom :{}\n\n",
                main_count / 1000,
                main_head.unwrap()
            ));
        }
    }
    (st.s, st.mark, branches + 1)
}

fn generate_repository(dir: &Path) -> (usize, usize) {
    // `init -b` needs git 2.28
    git(dir, &["init", "-q"]);
    git(dir, &["symbolic-ref", "HEAD", "refs/heads/master"]);
    let (stream, commits, branches) = fast_import_stream();
    let mut child = git_command(dir, &["fast-import", "--quiet"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stream.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
    // fast-import leaves loose refs; real clones have packed refs, which changes the cost of
    // every per-ref git command, so measure against that layout
    git(dir, &["pack-refs", "--all"]);
    git(dir, &["reset", "-q", "--hard", "master"]);
    (commits, branches)
}

/// The `git log` of `read_all_hashes`, output discarded.
fn git_log_alone(dir: &Path) -> Duration {
    let start = Instant::now();
    let status = git_command(
        dir,
        &[
            "log",
            "HEAD",
            "--branches",
            "--remotes",
            "--tags",
            "--date-order",
            "--decorate=full",
            "--pretty=format:%x01%x01%H%x02%x02%P%x02%x02%ct%x02%x02%an%x02%x02%ae%x02%x02%d%x03%x03",
        ],
    )
    .stdout(Stdio::null())
    .status()
    .unwrap();
    assert!(status.success());
    start.elapsed()
}

fn ms(d: Duration) -> String {
    format!("{:>8.1} ms", d.as_secs_f64() * 1000.0)
}

#[test]
#[ignore = "performance smoke test; run with --release -- --ignored --nocapture"]
fn load_50k_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("big");
    std::fs::create_dir_all(&dir).unwrap();
    let start = Instant::now();
    let (commits, branches) = generate_repository(&dir);
    eprintln!(
        "generated {commits} commits, {branches} branches with fast-import in {}",
        ms(start.elapsed())
    );
    let root = vcs_root(&dir);
    let provider = Arc::new(provider());

    // git alone, same arguments as read_all_hashes: the first run warms the caches, the second
    // (warm) one is reported
    git_log_alone(&dir);
    let t_git = git_log_alone(&dir);

    // step by step
    let start = Instant::now();
    let mut timed = Vec::with_capacity(commits);
    let data = provider
        .read_all_hashes(&root, &mut |c| timed.push(c))
        .unwrap();
    let t_read = start.elapsed();
    assert_eq!(timed.len(), commits);

    let storage: Arc<dyn VcsLogStorage> = Arc::new(InMemoryStorage::new());
    let start = Instant::now();
    let graph_commits: Vec<_> = timed
        .iter()
        .map(|c| compact_commit(storage.as_ref(), c, &root))
        .collect();
    let refs = RootRefsModel::create(data.refs.iter().cloned(), storage.as_ref());
    let t_compact = start.elapsed();

    let mut providers = LogProviders::new();
    providers.insert(root.clone(), provider.clone() as Arc<dyn VcsLogProvider>);
    let start = Instant::now();
    let pack = build_data(
        &graph_commits,
        BTreeMap::from([(root.clone(), Arc::new(refs))]),
        &providers,
        &storage,
        true,
    );
    let t_build = start.elapsed();

    let graph = pack.permanent_graph().unwrap();
    let start = Instant::now();
    let visible = graph
        .create_visible_graph(Options::DEFAULT, None, None)
        .unwrap();
    let t_visible = start.elapsed();
    assert_eq!(visible.visible_commit_count() as usize, commits);

    let start = Instant::now();
    let mut elements = 0;
    for row in 0..1000 {
        elements += visible.get_row_info(row).print_elements().len();
    }
    let t_rows = start.elapsed();
    assert!(elements > 1000);

    let start = Instant::now();
    let width = visible.recommended_width();
    let t_width = start.elapsed();

    // end to end, as the UI does it
    let storage = Arc::new(InMemoryStorage::new());
    let mut refresher = VcsLogRefresher::new(storage.clone(), providers.clone(), 1000);
    let start = Instant::now();
    let first_pack: Arc<Mutex<Option<Duration>>> = Arc::new(Mutex::new(None));
    let sink = first_pack.clone();
    refresher.set_data_pack_handler(move |_pack| {
        sink.lock().unwrap().get_or_insert(start.elapsed());
    });
    let full = refresher.initialize();
    let t_initialize = start.elapsed();
    let t_first_block = first_pack.lock().unwrap().expect("a pack was published");
    assert_eq!(full.commits_count(), commits);

    commit_at(&dir, "new.txt", 2_000_000_000);
    let start = Instant::now();
    let refreshed = refresher.refresh(std::slice::from_ref(&root));
    let t_refresh = start.elapsed();
    assert_eq!(refreshed.commits_count(), commits + 1);
    assert!(refreshed.is_full() && refreshed.load_error().is_none());

    // ~50 new commits on top of the head (measured: joined within the recent block of 1000,
    // no full reload)
    let mut t = 2_000_000_000;
    for i in 0..50 {
        t += 10;
        commit_at(&dir, &format!("many{i}.txt"), t);
    }
    let start = Instant::now();
    let refreshed = refresher.refresh(std::slice::from_ref(&root));
    let t_refresh_many = start.elapsed();
    assert_eq!(refreshed.commits_count(), commits + 51);
    assert!(refreshed.is_full() && refreshed.load_error().is_none());

    // tag-heavy: new lightweight tags on old commits (a fetch of many tags)
    //
    // Known upstream limitation: the joiner runs one `git log` per new tag (about 13 ms each with
    // packed refs), so a fetch of hundreds of tags makes a refresh take seconds.
    const NEW_TAGS: usize = 50;
    let old = git(&dir, &["rev-list", "--skip=2000", "--max-count=50", "HEAD"]);
    let targets: Vec<&str> = old.lines().collect();
    assert_eq!(targets.len(), NEW_TAGS);
    for (i, hash) in targets.iter().enumerate() {
        git(&dir, &["tag", &format!("perf-tag-{i}"), hash]);
    }
    let start = Instant::now();
    let refreshed = refresher.refresh(std::slice::from_ref(&root));
    let t_refresh_tags = start.elapsed();
    assert_eq!(refreshed.commits_count(), commits + 51);
    assert!(refreshed.is_full() && refreshed.load_error().is_none());

    eprintln!("git log alone (warm, discarded)    {}", ms(t_git));
    eprintln!("read_all_hashes (git log + parse)     {}", ms(t_read));
    eprintln!("intern ids + refs model               {}", ms(t_compact));
    eprintln!("build data pack (permanent graph)     {}", ms(t_build));
    eprintln!("create visible graph (lazy)           {}", ms(t_visible));
    eprintln!("print elements of 1000 rows           {}", ms(t_rows));
    eprintln!(
        "recommended width (width {width:>3})        {}",
        ms(t_width)
    );
    eprintln!(
        "time to first block (first pack)      {}",
        ms(t_first_block)
    );
    eprintln!("refresher.initialize (block + full)   {}", ms(t_initialize));
    eprintln!("refresher.refresh (1 new commit)      {}", ms(t_refresh));
    eprintln!(
        "refresher.refresh (50 new commits)    {}",
        ms(t_refresh_many)
    );
    eprintln!(
        "refresher.refresh ({NEW_TAGS} new tags)       {}  ({} per tag)",
        ms(t_refresh_tags),
        ms(t_refresh_tags / NEW_TAGS as u32)
    );
}
```

`crates/ig-log/src/permanent/permanent_commits_info_impl.rs`:

```rust
//! Node id to commit id (and timestamp) mapping of the permanent graph.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentCommitsInfoImpl.kt`
//! and `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/RowsMapping.kt`.
//!
//! Deviations:
//! - No `Int`-specialised storage (`isIntegerCase`); `Vec<CommitId>` is already compact.
//! - `get_node_id`, `convert_to_node_ids` and `contains_all` use hash indexes (commit id -> row and
//!   commit id -> not-loaded id) instead of linear scans (upstream: "todo optimize with special
//!   map"), so they are O(1) per commit instead of O(rows). Results are unchanged, including for
//!   duplicate commit ids: `get_node_id` returns the first row, `convert_to_node_ids` every row.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::api::permanent::{PermanentCommitsInfo, VcsLogGraphNodeId};
use crate::graph_commit::GraphCommit;
use crate::utils::TimestampGetter;

/// Commit ids and timestamps by row (`RowsMapping`).
#[derive(Clone, Debug)]
pub struct RowsMapping<CommitId> {
    commit_id_mapping: Vec<CommitId>,
    timestamps_mapping: Vec<i64>,
}

impl<CommitId: Clone> RowsMapping<CommitId> {
    pub fn new(size: usize) -> RowsMapping<CommitId> {
        RowsMapping {
            commit_id_mapping: Vec::with_capacity(size),
            timestamps_mapping: Vec::with_capacity(size),
        }
    }

    pub fn add(&mut self, commit_id: CommitId, timestamp: i64) {
        self.commit_id_mapping.push(commit_id);
        self.timestamps_mapping.push(timestamp);
    }

    pub fn commit_id_mapping(&self) -> &[CommitId] {
        &self.commit_id_mapping
    }

    pub fn get_commit_id(&self, row: i32) -> CommitId {
        self.commit_id_mapping[row as usize].clone()
    }
}

impl<CommitId> TimestampGetter for RowsMapping<CommitId> {
    fn get_timestamp(&self, row: i32) -> i64 {
        self.timestamps_mapping[row as usize]
    }
}

#[derive(Clone, Debug)]
pub struct PermanentCommitsInfoImpl<CommitId> {
    rows_mapping: RowsMapping<CommitId>,
    not_loaded_commits: HashMap<i32, CommitId>,
    /// Commit id -> its first row.
    node_ids: HashMap<CommitId, i32>,
    /// Commit id -> the rows after the first, for commit ids listed more than once (normally empty).
    duplicate_node_ids: HashMap<CommitId, Vec<i32>>,
    /// Commit id -> its not-loaded id. The graph generator guarantees each not-loaded commit gets
    /// exactly one id (checked by a `debug_assert` in `new_instance`).
    not_loaded_node_ids: HashMap<CommitId, i32>,
}

impl<CommitId: Clone + Eq + Hash + Send + Sync> PermanentCommitsInfoImpl<CommitId> {
    /// `newInstance`: rows in the order of `graph_commits`; `not_loaded_commits` maps synthetic
    /// negative ids to the commits they stand for.
    pub fn new_instance<G: GraphCommit<CommitId>>(
        graph_commits: &[G],
        not_loaded_commits: HashMap<i32, CommitId>,
    ) -> PermanentCommitsInfoImpl<CommitId> {
        let mut rows_mapping = RowsMapping::new(graph_commits.len());
        let mut node_ids = HashMap::with_capacity(graph_commits.len());
        let mut duplicate_node_ids: HashMap<CommitId, Vec<i32>> = HashMap::new();
        for (row, commit) in graph_commits.iter().enumerate() {
            rows_mapping.add(commit.id().clone(), commit.timestamp());
            match node_ids.entry(commit.id().clone()) {
                Entry::Vacant(slot) => {
                    slot.insert(row as i32);
                }
                Entry::Occupied(slot) => {
                    duplicate_node_ids
                        .entry(slot.key().clone())
                        .or_default()
                        .push(row as i32);
                }
            }
        }
        let not_loaded_node_ids = not_loaded_commits
            .iter()
            .map(|(&node_id, commit_id)| (commit_id.clone(), node_id))
            .collect::<HashMap<CommitId, i32>>();
        debug_assert_eq!(
            not_loaded_node_ids.len(),
            not_loaded_commits.len(),
            "each not-loaded commit must have exactly one id"
        );
        PermanentCommitsInfoImpl {
            rows_mapping,
            not_loaded_commits,
            node_ids,
            duplicate_node_ids,
            not_loaded_node_ids,
        }
    }

    pub fn timestamp_getter(&self) -> &dyn TimestampGetter {
        &self.rows_mapping
    }

    fn get_not_load_node_id(&self, commit_id: &CommitId) -> i32 {
        self.not_loaded_node_ids
            .get(commit_id)
            .copied()
            .unwrap_or(-1)
    }

    pub fn convert_to_commit_id_list(
        &self,
        commit_indexes: impl IntoIterator<Item = VcsLogGraphNodeId>,
    ) -> Vec<CommitId> {
        commit_indexes
            .into_iter()
            .map(|node_id| self.get_commit_id(node_id))
            .collect()
    }

    pub fn convert_to_commit_id_set(
        &self,
        commit_indexes: impl IntoIterator<Item = VcsLogGraphNodeId>,
    ) -> HashSet<CommitId> {
        commit_indexes
            .into_iter()
            .map(|node_id| self.get_commit_id(node_id))
            .collect()
    }

    /// `convertToNodeIds(commitIds, skipNotLoadedCommits)`.
    pub fn convert_to_node_ids_skipping(
        &self,
        commit_ids: &HashSet<CommitId>,
        skip_not_loaded_commits: bool,
    ) -> HashSet<VcsLogGraphNodeId> {
        let mut result: HashSet<i32> = HashSet::with_capacity(commit_ids.len());
        for commit_id in commit_ids {
            if let Some(&row) = self.node_ids.get(commit_id) {
                result.insert(row);
                if let Some(rows) = self.duplicate_node_ids.get(commit_id) {
                    result.extend(rows.iter().copied());
                }
            }
            if !skip_not_loaded_commits
                && let Some(&node_id) = self.not_loaded_node_ids.get(commit_id)
            {
                result.insert(node_id);
            }
        }
        result
    }

    pub fn contains_all<'a>(&self, commit_ids: impl IntoIterator<Item = &'a CommitId>) -> bool
    where
        CommitId: 'a,
    {
        commit_ids
            .into_iter()
            .all(|id| self.node_ids.contains_key(id))
    }
}

impl<CommitId: Clone + Eq + Hash + Send + Sync> PermanentCommitsInfo<CommitId>
    for PermanentCommitsInfoImpl<CommitId>
{
    /// # Panics
    /// If `node_id` is neither a row nor a known not-loaded id.
    fn get_commit_id(&self, node_id: VcsLogGraphNodeId) -> CommitId {
        if node_id < 0 {
            return self
                .not_loaded_commits
                .get(&node_id)
                .cloned()
                .unwrap_or_else(|| panic!("Unknown not loaded node id: {node_id}"));
        }
        self.rows_mapping.get_commit_id(node_id)
    }

    fn try_get_commit_id(&self, node_id: VcsLogGraphNodeId) -> Option<CommitId> {
        if node_id < 0 {
            return self.not_loaded_commits.get(&node_id).cloned();
        }
        self.rows_mapping
            .commit_id_mapping()
            .get(node_id as usize)
            .cloned()
    }

    fn get_timestamp(&self, node_id: VcsLogGraphNodeId) -> i64 {
        if node_id < 0 {
            return 0;
        }
        self.rows_mapping.get_timestamp(node_id)
    }

    /// The node id assigned to `commit_id`:
    /// - `0..nodes_count`: the row of a loaded commit;
    /// - `<= -2`: the synthetic id of a commit referenced as a parent but not loaded;
    /// - `-1`: unknown to this permanent graph.
    fn get_node_id(&self, commit_id: &CommitId) -> VcsLogGraphNodeId {
        if let Some(&row) = self.node_ids.get(commit_id) {
            return row;
        }
        self.get_not_load_node_id(commit_id)
    }

    fn convert_to_node_ids(&self, commit_ids: &HashSet<CommitId>) -> HashSet<VcsLogGraphNodeId> {
        self.convert_to_node_ids_skipping(commit_ids, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct C(&'static str);

    impl GraphCommit<&'static str> for C {
        fn id(&self) -> &&'static str {
            &self.0
        }
        fn parents(&self) -> &[&'static str] {
            &[]
        }
        fn timestamp(&self) -> i64 {
            0
        }
    }

    fn info(
        loaded: &[&'static str],
        not_loaded: &[(i32, &'static str)],
    ) -> PermanentCommitsInfoImpl<&'static str> {
        let commits: Vec<C> = loaded.iter().map(|&id| C(id)).collect();
        PermanentCommitsInfoImpl::new_instance(&commits, not_loaded.iter().copied().collect())
    }

    fn set<T: Eq + Hash>(items: impl IntoIterator<Item = T>) -> HashSet<T> {
        items.into_iter().collect()
    }

    fn standard() -> PermanentCommitsInfoImpl<&'static str> {
        info(&["a", "b", "a", "c"], &[(-2, "x"), (-3, "y")])
    }

    #[test]
    fn get_node_id_returns_first_row_of_duplicate() {
        assert_eq!(standard().get_node_id(&"a"), 0);
    }

    #[test]
    fn convert_to_node_ids_returns_every_duplicate_row() {
        assert_eq!(standard().convert_to_node_ids(&set(["a"])), set([0, 2]));
    }

    #[test]
    fn get_node_id_not_loaded_and_unknown() {
        let info = standard();
        assert_eq!(info.get_node_id(&"x"), -2);
        assert_eq!(info.get_node_id(&"zzz"), -1);
    }

    #[test]
    fn convert_to_node_ids_mixed_and_skipping() {
        let info = standard();
        let ids = set(["x", "b"]);
        assert_eq!(info.convert_to_node_ids(&ids), set([1, -2]));
        assert_eq!(info.convert_to_node_ids_skipping(&ids, true), set([1]));
        assert_eq!(info.convert_to_node_ids_skipping(&ids, false), set([1, -2]));
    }

    #[test]
    fn contains_all_checks_loaded_commits_only() {
        let info = standard();
        assert!(info.contains_all([&"a", &"a", &"b"]));
        assert!(!info.contains_all([&"a", &"x"]));
        assert!(info.contains_all([]));
    }

    #[test]
    fn commit_both_loaded_and_not_loaded() {
        let info = info(&["b"], &[(-4, "b")]);
        assert_eq!(info.get_node_id(&"b"), 0);
        assert_eq!(info.convert_to_node_ids(&set(["b"])), set([0, -4]));
        assert_eq!(
            info.convert_to_node_ids_skipping(&set(["b"]), true),
            set([0])
        );
    }
}
```

`crates/ig-log/src/utils/flags.rs`:

```rust
//! Fixed-size boolean flags.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/Flags.java` and
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/BitSetFlags.java`.

use std::fmt;

/// A fixed number of boolean flags indexed `0..size()`.
pub trait Flags {
    fn size(&self) -> i32;

    fn get(&self, index: i32) -> bool;

    fn set(&mut self, index: i32, value: bool);

    fn set_all(&mut self, value: bool);
}

/// [`Flags`] backed by a bit set. All flags start `false`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct BitSetFlags {
    size: i32,
    words: Vec<u64>,
}

impl BitSetFlags {
    /// # Panics
    /// If `size < 0` (upstream `NegativeArraySizeException`).
    pub fn new(size: i32) -> BitSetFlags {
        assert!(size >= 0, "size < 0: {size}");
        BitSetFlags {
            size,
            words: vec![0; (size as usize).div_ceil(64)],
        }
    }

    pub fn with_default(size: i32, default_value: bool) -> BitSetFlags {
        let mut flags = BitSetFlags::new(size);
        if default_value {
            flags.set_all(true);
        }
        flags
    }

    fn check_range(&self, index: i32) {
        assert!(index >= 0, "index is {index} which is less than zero");
        assert!(
            index < self.size,
            "index is {index} and set size is {}",
            self.size
        );
    }
}

impl Flags for BitSetFlags {
    fn size(&self) -> i32 {
        self.size
    }

    /// # Panics
    /// If `index` is out of `0..size()` (upstream `IndexOutOfBoundsException`).
    fn get(&self, index: i32) -> bool {
        self.check_range(index);
        let index = index as usize;
        self.words[index / 64] & (1 << (index % 64)) != 0
    }

    /// # Panics
    /// If `index` is out of `0..size()`.
    fn set(&mut self, index: i32, value: bool) {
        self.check_range(index);
        let index = index as usize;
        if value {
            self.words[index / 64] |= 1 << (index % 64);
        } else {
            self.words[index / 64] &= !(1 << (index % 64));
        }
    }

    fn set_all(&mut self, value: bool) {
        let fill = if value { u64::MAX } else { 0 };
        self.words.iter_mut().for_each(|word| *word = fill);
        let tail = self.size as usize % 64;
        if value
            && tail != 0
            && let Some(last) = self.words.last_mut()
        {
            *last &= (1 << tail) - 1;
        }
    }
}

impl fmt::Debug for BitSetFlags {
    /// Like upstream `toString`: `{set indexes}, size = N`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let set: Vec<i32> = (0..self.size).filter(|&i| self.get(i)).collect();
        let set: Vec<String> = set.iter().map(i32::to_string).collect();
        write!(f, "{{{}}}, size = {}", set.join(", "), self.size)
    }
}
```

- [ ] **Step 10: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code origin/wip/app-shell-prototype -- Cargo.toml .cargo/config.toml .gitignore .github/workflows/ci.yml Cargo.lock crates/ig-ui-kit/Cargo.toml crates/ig-app/Cargo.toml xtask/Cargo.toml crates/ig-git/src/config.rs crates/ig-git/src/log/util.rs crates/ig-git/src/refs.rs crates/ig-git/tests/log_perf.rs crates/ig-log/src/permanent/permanent_commits_info_impl.rs crates/ig-log/src/utils/flags.rs
```

Expected: no output. The intermediate files written above differ from the branch until their final task.

- [ ] **Step 11: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 398 passed, 1 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 12: Commit**

```bash
git add -A
git commit -m "build: GPUI workspace (MSRV 1.95), ig-ui-kit/ig-app/xtask skeletons, CI for GPUI"
```

---

### Task 2: `xtask import-ui` and the imported assets

**Files:**
- Create: `xtask/src/fonts.rs`, `xtask/src/icons.rs`, `xtask/src/keymap.rs`, `xtask/src/lib.rs`, `xtask/src/main.rs`, `xtask/src/output.rs`, `xtask/src/source.rs`, `xtask/src/strings.rs`, `xtask/src/theme.rs`, `xtask/src/tokens.rs`, `xtask/src/xml.rs`, `xtask/tests/fixtures/**`, `xtask/tests/import_ui.rs`, `assets/theme.keys`, `assets/icons.list`, `assets/strings.list`, `assets/keymap.list`, `crates/ig-ui-kit/src/theme/tokens.rs`, `assets/themes/dark.json`, `assets/themes/light.json`, `assets/icons/**`, `assets/strings/GitBundle.properties`, `assets/strings/UtilBundle.properties`, `assets/strings/VcsBundle.properties`, `assets/strings/VcsLogBundle.properties`, `assets/keymaps/default.tsv`, `assets/keymaps/macos.tsv`, `assets/fonts/**`
- Modify: `NOTICE`

**Review focus:** theme parent-chain resolution and `*` pattern order vs upstream `UITheme`/`UIManager` loading; keymap parsing vs `KeymapImpl`; missing-key failure leaves the workspace untouched.

`import-ui` reads a pinned intellij-community checkout through `git show` (`GitSource`) or a plain directory (`FsSource`, used by the fixture test). It resolves everything before writing anything.

- [ ] **Step 1: The `xtask` sources.**

```bash
git checkout origin/wip/app-shell-prototype -- xtask/src
```

`xtask/src/fonts.rs`:

```rust
//! Font copying from `platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts/`
//! (the Inter and JetBrains Mono files Jewel's standalone Int UI theme bundles), plus their SIL
//! OFL 1.1 license texts.
//!
//! Upstream ships no license file next to these fonts, so each `*-OFL.txt` is assembled from the
//! font's own copyright notice (OpenType `name` table, name ID 0, read from the Regular face) and
//! the OFL 1.1 text of the one OFL license file in the upstream tree,
//! `platform/platform-tests/testData/fonts/Inconsolata_LICENSE.txt` (from its "This Font Software
//! is licensed" line on). Font files are copied unchanged.

use crate::source::Source;
use crate::{Error, Result, bail};

pub const FONTS_DIR: &str = "platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts/";
pub const OFL_SOURCE: &str = "platform/platform-tests/testData/fonts/Inconsolata_LICENSE.txt";
const OFL_START: &str = "This Font Software is licensed under the SIL Open Font License";

/// `(family license file, [(upstream path under FONTS_DIR, output file name)])`; the first
/// face of each family provides the copyright notice.
pub const FAMILIES: [(&str, &[(&str, &str)]); 2] = [
    (
        "Inter-OFL.txt",
        &[
            ("inter/Inter-Regular.ttf", "Inter-Regular.ttf"),
            ("inter/Inter-Medium.ttf", "Inter-Medium.ttf"),
            ("inter/Inter-SemiBold.ttf", "Inter-SemiBold.ttf"),
            ("inter/Inter-Bold.ttf", "Inter-Bold.ttf"),
        ],
    ),
    (
        "JetBrainsMono-OFL.txt",
        &[
            (
                "jetbrains-mono/JetBrainsMono-Regular.ttf",
                "JetBrainsMono-Regular.ttf",
            ),
            (
                "jetbrains-mono/JetBrainsMono-Bold.ttf",
                "JetBrainsMono-Bold.ttf",
            ),
        ],
    ),
];

fn be16(d: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_be_bytes([*d.get(at)?, *d.get(at + 1)?]) as usize)
}

fn be32(d: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_be_bytes([
        *d.get(at)?,
        *d.get(at + 1)?,
        *d.get(at + 2)?,
        *d.get(at + 3)?,
    ]) as usize)
}

/// The string with name ID `id` from an sfnt (TrueType/OpenType) `name` table, preferring
/// Windows Unicode (platform 3, UTF-16BE) over Macintosh Roman (platform 1, read as ASCII).
pub fn name_string(font: &[u8], id: usize) -> Option<String> {
    let tables = be16(font, 4)?;
    let name = (0..tables).find(|i| font.get(12 + 16 * i..16 + 16 * i) == Some(b"name"))?;
    let off = be32(font, 12 + 16 * name + 8)?;
    let count = be16(font, off + 2)?;
    let strings = off + be16(font, off + 4)?;
    let mut fallback = None;
    for r in 0..count {
        let rec = off + 6 + 12 * r;
        let (platform, name_id) = (be16(font, rec)?, be16(font, rec + 6)?);
        let (len, at) = (be16(font, rec + 8)?, strings + be16(font, rec + 10)?);
        if name_id != id {
            continue;
        }
        let bytes = font.get(at..at + len)?;
        match platform {
            3 | 0 => {
                let (pairs, _) = bytes.as_chunks::<2>();
                let units: Vec<u16> = pairs.iter().map(|c| u16::from_be_bytes(*c)).collect();
                return String::from_utf16(&units).ok();
            }
            1 if bytes.is_ascii() => fallback = Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => {}
        }
    }
    fallback
}

/// Reads the fonts and builds the license files; returns `(output path, bytes)` pairs.
pub fn collect(src: &dyn Source) -> Result<Vec<(String, Vec<u8>)>> {
    let ofl = src.read_text(OFL_SOURCE)?;
    let Some(start) = ofl.find(OFL_START) else {
        bail!("{OFL_SOURCE}: no OFL text ({OFL_START:?})")
    };
    let ofl_text = &ofl[start..];

    let mut out = Vec::new();
    for (license, faces) in FAMILIES {
        let mut copyright = None;
        for (path, file) in faces {
            let full = format!("{FONTS_DIR}{path}");
            let bytes = src.read(&full)?;
            if copyright.is_none() {
                let notice = name_string(&bytes, 0).ok_or_else(|| {
                    Error(format!(
                        "{full}: no copyright notice (name ID 0) in the name table"
                    ))
                })?;
                copyright = Some(notice);
            }
            out.push((format!("assets/fonts/{file}"), bytes));
        }
        let mut text = copyright.unwrap_or_default();
        text.push_str("\n\n");
        text.push_str(ofl_text);
        if !text.ends_with('\n') {
            text.push('\n');
        }
        out.push((format!("assets/fonts/{license}"), text.into_bytes()));
    }
    Ok(out)
}
```

`xtask/src/icons.rs`:

```rust
//! Icon copying from `assets/icons.list` (`<logical/name> <upstream path to the light svg>`).
//! The dark variant is the `<name>_dark.svg` file next to the light one, the naming convention
//! of `platform/util/ui/src/com/intellij/ui/icons/ImageDescriptor.kt` (`_dark` suffix). Files are
//! copied byte for byte.

use crate::source::Source;
use crate::{Result, bail};

/// One `icons.list` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconEntry {
    pub name: String,
    pub path: String,
}

/// Parses `icons.list`; `#` starts a comment.
pub fn parse_list(text: &str) -> Result<Vec<IconEntry>> {
    let mut entries: Vec<IconEntry> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(name), Some(path), None) = (parts.next(), parts.next(), parts.next()) else {
            bail!(
                "icons.list:{}: expected `<logical/name> <upstream path>`",
                i + 1
            )
        };
        let valid = name.split('/').all(|seg| {
            !seg.is_empty()
                && seg != "."
                && seg != ".."
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
        });
        if !valid {
            bail!("icons.list:{}: bad logical name {name:?}", i + 1);
        }
        if !path.ends_with(".svg") || path.ends_with("_dark.svg") {
            bail!("icons.list:{}: {path} is not a light .svg", i + 1);
        }
        if entries.iter().any(|e| e.name == name) {
            bail!("icons.list:{}: {name} is listed twice", i + 1);
        }
        entries.push(IconEntry {
            name: name.to_string(),
            path: path.to_string(),
        });
    }
    Ok(entries)
}

/// Reads every listed icon and its dark variant; returns `(output path, bytes)` pairs.
pub fn collect(src: &dyn Source, list: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    let mut missing = Vec::new();
    for e in parse_list(list)? {
        if !src.exists(&e.path)? {
            missing.push(format!("{} ({})", e.name, e.path));
            continue;
        }
        out.push((format!("assets/icons/{}.svg", e.name), src.read(&e.path)?));
        let dark = format!("{}_dark.svg", e.path.trim_end_matches(".svg"));
        if src.exists(&dark)? {
            out.push((
                format!("assets/icons/{}_dark.svg", e.name),
                src.read(&dark)?,
            ));
        }
    }
    if !missing.is_empty() {
        bail!(
            "icons.list: missing upstream icons in {}: {}",
            src.describe(),
            missing.join(", ")
        );
    }
    Ok(out)
}
```

`xtask/src/keymap.rs`:

```rust
//! Keymap extraction for the actions in `assets/keymap.list`, a port of the shortcut lookup in
//! `platform/platform-impl/src/com/intellij/openapi/keymap/impl/KeymapImpl.kt`
//! (`readExternal`, `getShortcutList`, `getParentShortcuts`) and
//! `platform/platform-impl/src/com/intellij/openapi/keymap/impl/MacOSDefaultKeymap.kt`:
//!
//! - an `<action>` listed in a keymap (even with no shortcuts) replaces its parent's shortcuts;
//! - an action without its own entry uses the shortcuts its `use-shortcut-of` target has in the
//!   same keymap, else its parent keymap's shortcuts;
//! - a keymap whose name starts with `Mac OS X` swaps `control`/`meta` in shortcuts it inherits
//!   (`MacOSDefaultKeymap.convertShortcutFromParent`) and, like upstream on macOS, skips
//!   keystrokes containing `INSERT` (`readExternal`'s `skipInserts`).
//!
//! `keymap.list` lines: `<action id> [<upstream XML declaring the action>]`; the optional file is
//! read for the action's `use-shortcut-of` (upstream `ActionManagerEx.getActionBinding`).
//! Mouse and gesture shortcuts are ignored. Keystrokes keep the upstream text.

use std::collections::HashMap;

use crate::source::Source;
use crate::xml::{self, Element};
use crate::{Error, Result, bail};

pub const KEYMAPS_DIR: &str = "platform/platform-resources/src/keymaps/";
/// `(keymap name, output file)`.
pub const OUTPUTS: [(&str, &str); 2] = [
    ("$default", "assets/keymaps/default.tsv"),
    ("Mac OS X 10.5+", "assets/keymaps/macos.tsv"),
];

/// `(first keystroke, second keystroke)`.
pub type Shortcut = (String, Option<String>);

struct Keymap {
    parent: Option<String>,
    /// Own shortcuts per listed action, in file order.
    actions: HashMap<String, Vec<Shortcut>>,
}

fn is_mac(name: &str) -> bool {
    name.starts_with("Mac OS X")
}

fn read_keymap(root: &Element, mac: bool) -> Keymap {
    let mut actions: HashMap<String, Vec<Shortcut>> = HashMap::new();
    for action in root.children_named("action") {
        let Some(id) = action.attr("id") else {
            continue;
        };
        let list = actions.entry(id.to_string()).or_default();
        for s in action.children_named("keyboard-shortcut") {
            let Some(first) = s.attr("first-keystroke") else {
                continue;
            };
            if mac && first.contains("INSERT") {
                continue;
            }
            list.push((
                first.to_string(),
                s.attr("second-keystroke").map(str::to_string),
            ));
        }
    }
    Keymap {
        parent: root.attr("parent").map(str::to_string),
        actions,
    }
}

/// Swaps `control`/`ctrl` and `meta` modifiers, as `MacOSDefaultKeymap.mapModifiers`.
pub fn convert_for_mac(keystroke: &str) -> String {
    keystroke
        .split(' ')
        .map(|t| match t {
            "control" | "ctrl" => "meta",
            "meta" => "control",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

struct Keymaps {
    by_name: HashMap<String, Keymap>,
    /// `use-shortcut-of` bindings.
    bindings: HashMap<String, String>,
}

impl Keymaps {
    fn shortcuts(&self, keymap: &str, id: &str, depth: usize) -> Result<Vec<Shortcut>> {
        if depth > 16 {
            bail!("keymap parent chain too deep at {keymap:?}");
        }
        let Some(k) = self.by_name.get(keymap) else {
            bail!("no keymap named {keymap:?} in {KEYMAPS_DIR}")
        };
        if let Some(own) = k.actions.get(id) {
            return Ok(own.clone());
        }
        if let Some(bound) = self.bindings.get(id).and_then(|b| k.actions.get(b)) {
            return Ok(bound.clone());
        }
        let Some(parent) = &k.parent else {
            return Ok(Vec::new());
        };
        let inherited = self.shortcuts(parent, id, depth + 1)?;
        if !is_mac(keymap) {
            return Ok(inherited);
        }
        Ok(inherited
            .into_iter()
            .filter(|(first, _)| !first.contains("INSERT"))
            .map(|(first, second)| {
                (
                    convert_for_mac(&first),
                    second.as_deref().map(convert_for_mac),
                )
            })
            .collect())
    }
}

/// Resolves the listed actions in each keymap of [`OUTPUTS`]; returns `(output path, bytes)`.
pub fn collect(src: &dyn Source, list: &str, commit: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let mut by_name = HashMap::new();
    for path in src.files_under(KEYMAPS_DIR)? {
        if !path.ends_with(".xml") || path[KEYMAPS_DIR.len()..].contains('/') {
            continue;
        }
        let root = xml::parse(&src.read_text(path)?).map_err(|e| Error(format!("{path}: {e}")))?;
        if root.name != "keymap" {
            continue;
        }
        let Some(name) = root.attr("name") else {
            continue;
        };
        if by_name
            .insert(name.to_string(), read_keymap(&root, is_mac(name)))
            .is_some()
        {
            bail!("keymap {name:?} is defined twice in {KEYMAPS_DIR}");
        }
    }

    let mut ids: Vec<String> = Vec::new();
    let mut bindings = HashMap::new();
    for (i, raw) in list.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(id), decl, None) = (parts.next(), parts.next(), parts.next()) else {
            bail!(
                "keymap.list:{}: expected `<action id> [<declaring xml>]`",
                i + 1
            )
        };
        if ids.iter().any(|x| x == id) {
            bail!("keymap.list:{}: {id} is listed twice", i + 1);
        }
        if let Some(decl) = decl {
            let root =
                xml::parse(&src.read_text(decl)?).map_err(|e| Error(format!("{decl}: {e}")))?;
            let Some(action) = find_action(&root, id) else {
                bail!("keymap.list:{}: no <action id=\"{id}\"> in {decl}", i + 1)
            };
            if let Some(target) = action.attr("use-shortcut-of") {
                bindings.insert(id.to_string(), target.to_string());
            }
        }
        ids.push(id.to_string());
    }

    let keymaps = Keymaps { by_name, bindings };
    let mut out = Vec::new();
    for (name, file) in OUTPUTS {
        let mut text = format!(
            "# generated by `cargo xtask import-ui` from intellij-community {commit} keymap {name:?}\n\
             # <action id>\\t<first keystroke>[\\t<second keystroke>]\n"
        );
        for id in &ids {
            for (first, second) in keymaps.shortcuts(name, id, 0)? {
                text.push_str(id);
                text.push('\t');
                text.push_str(&first);
                if let Some(second) = second {
                    text.push('\t');
                    text.push_str(&second);
                }
                text.push('\n');
            }
        }
        out.push((file.to_string(), text.into_bytes()));
    }
    Ok(out)
}

fn find_action<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if el.name == "action" && el.attr("id") == Some(id) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_action(c, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_conversion_swaps_control_and_meta() {
        assert_eq!(convert_for_mac("control shift F5"), "meta shift F5");
        assert_eq!(convert_for_mac("meta C"), "control C");
        assert_eq!(convert_for_mac("ctrl A"), "meta A");
        assert_eq!(convert_for_mac("PAGE_UP"), "PAGE_UP");
    }
}
```

`xtask/src/lib.rs`:

```rust
//! Developer tasks for IdeaGit, run via `cargo xtask <task>`.
//!
//! `import-ui` copies UI resources from a pinned intellij-community checkout: theme colors
//! ([`theme`], [`tokens`]), icons ([`icons`]), message bundles ([`strings`]), keymaps
//! ([`keymap`]) and fonts ([`fonts`]). Everything is resolved before anything is written, so a
//! failing import leaves the workspace untouched; writes are deterministic and only touch files
//! whose content changed ([`output`]).

pub mod fonts;
pub mod icons;
pub mod keymap;
pub mod output;
pub mod source;
pub mod strings;
pub mod theme;
pub mod tokens;
pub mod xml;

use std::fmt;
use std::path::Path;

pub use source::{FsSource, GitSource, Source};

/// An import failure, carrying a message meant for the person running the task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Returns early with an [`Error`] built from a format string.
#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::Error(format!($($arg)*)))
    };
}

/// Hand-written input lists, relative to the output workspace root.
pub const THEME_KEYS: &str = "assets/theme.keys";
pub const ICONS_LIST: &str = "assets/icons.list";
pub const STRINGS_LIST: &str = "assets/strings.list";
pub const KEYMAP_LIST: &str = "assets/keymap.list";

/// Generated outputs, relative to the output workspace root.
pub const TOKENS_RS: &str = "crates/ig-ui-kit/src/theme/tokens.rs";

/// What an import produced, for the CLI to print.
#[derive(Debug, Default)]
pub struct Summary {
    pub written: Vec<String>,
    pub unchanged: usize,
    pub removed: Vec<String>,
}

/// Runs `import-ui`: reads the input lists under `<out>/assets`, resolves them against `source`
/// and writes the generated files under `out`. `commit` is recorded in generated headers.
pub fn import_ui(source: &dyn Source, out: &Path, commit: &str) -> Result<Summary> {
    let read_input = |rel: &str| -> Result<String> {
        std::fs::read_to_string(out.join(rel)).map_err(|e| Error(format!("cannot read {rel}: {e}")))
    };
    let keys = tokens::parse_keys(&read_input(THEME_KEYS)?)?;
    let light = theme::Theme::load(source, theme::LIGHT_THEME)?;
    let dark = theme::Theme::load(source, theme::DARK_THEME)?;
    let resolved = tokens::resolve(&keys, &light, &dark)?;
    let icons = icons::collect(source, &read_input(ICONS_LIST)?)?;
    let strings = strings::collect(source, &read_input(STRINGS_LIST)?)?;
    let keymaps = keymap::collect(source, &read_input(KEYMAP_LIST)?, commit)?;
    let fonts = fonts::collect(source)?;

    let mut o = output::Output::new(out);
    o.text(
        TOKENS_RS,
        &tokens::render_rust(&resolved, &light, &dark, commit),
    );
    o.text(
        "assets/themes/light.json",
        &tokens::render_json(&resolved, tokens::Variant::Light),
    );
    o.text(
        "assets/themes/dark.json",
        &tokens::render_json(&resolved, tokens::Variant::Dark),
    );
    for (rel, bytes) in icons.into_iter().chain(fonts).chain(strings).chain(keymaps) {
        o.bytes(&rel, &bytes);
    }
    o.finish(&[
        "assets/icons",
        "assets/strings",
        "assets/keymaps",
        "assets/themes",
        "assets/fonts",
    ])
}

/// Reads `commit = "..."` from `reference/UPSTREAM` under `root`.
pub fn pinned_commit(root: &Path) -> Result<String> {
    let path = root.join("reference/UPSTREAM");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| Error(format!("cannot read {}: {e}", path.display())))?;
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("commit")
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            return Ok(value.trim().trim_matches('"').to_string());
        }
    }
    bail!("no `commit = \"...\"` line in {}", path.display())
}
```

`xtask/src/main.rs`:

```rust
//! `cargo xtask <task>`: see [`xtask`] for the tasks.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use xtask::{Error, FsSource, GitSource, Result, Source};

const USAGE: &str = "\
usage: cargo xtask <task>

tasks:
  import-ui [--upstream <dir> | --fs <dir>] [--rev <rev>] [--out <workspace root>]
      Copy theme tokens, icons, strings, keymaps and fonts from intellij-community.
      --upstream  intellij-community git clone (default: $IDEAGIT_UPSTREAM)
      --fs        read upstream files from a plain directory tree instead of git
      --rev       upstream revision (default: `commit` in reference/UPSTREAM)
      --out       workspace root to write to (default: this workspace)
  screenshots [--repo <dir>] [--out <dir>]
      Renders the Log of <repo> (default: this workspace) in the light and dark themes to
      <out>/ideagit-log-{light,dark}.png (default: docs/ui-reference), headless (no display or
      screen-recording permission needed). Builds ig-app with its `screenshots` feature.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("import-ui") => import_ui(&args[1..]),
        Some("screenshots") => screenshots(&args[1..]),
        Some("-h" | "--help") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => Err(Error(USAGE.to_string())),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn import_ui(args: &[String]) -> Result<()> {
    let (mut upstream, mut fs, mut rev, mut out) = (None, None, None, None);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let slot = match flag.as_str() {
            "--upstream" => &mut upstream,
            "--fs" => &mut fs,
            "--rev" => &mut rev,
            "--out" => &mut out,
            other => {
                return Err(Error(format!(
                    "import-ui: unknown argument {other:?}\n\n{USAGE}"
                )));
            }
        };
        let Some(value) = it.next() else {
            return Err(Error(format!("import-ui: {flag} needs a value")));
        };
        *slot = Some(value.clone());
    }
    let out = out
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(".."));
    let rev = match rev {
        Some(rev) => rev,
        None => xtask::pinned_commit(&out)?,
    };
    let source: Box<dyn Source> = match (
        fs,
        upstream.or_else(|| std::env::var("IDEAGIT_UPSTREAM").ok()),
    ) {
        (Some(_), Some(_)) if args.iter().any(|a| a == "--upstream") => {
            return Err(Error(
                "import-ui: --fs and --upstream are mutually exclusive".into(),
            ));
        }
        (Some(dir), _) => Box::new(FsSource::new(Path::new(&dir))?),
        (None, Some(dir)) => Box::new(GitSource::new(Path::new(&dir), &rev)?),
        (None, None) => {
            return Err(Error(
                "import-ui: no upstream clone given. Pass --upstream <intellij-community clone> or set \
                 IDEAGIT_UPSTREAM (or --fs <dir> for a plain directory tree)."
                    .into(),
            ));
        }
    };
    eprintln!("import-ui: reading {}", source.describe());
    let summary = xtask::import_ui(source.as_ref(), &out, &rev)?;
    for rel in &summary.written {
        eprintln!("  wrote {rel}");
    }
    for rel in &summary.removed {
        eprintln!("  removed {rel}");
    }
    eprintln!(
        "import-ui: {} written, {} unchanged, {} removed",
        summary.written.len(),
        summary.unchanged,
        summary.removed.len()
    );
    Ok(())
}

/// `screenshots`: runs `ideagit --screenshots <out> <repo>` (feature `screenshots`, release).
fn screenshots(args: &[String]) -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut repo = root.clone();
    let mut out = root.join("docs/ui-reference");
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let value = || Error(format!("{arg} needs a value"));
        match arg.as_str() {
            "--repo" => repo = it.next().ok_or_else(value)?.into(),
            "--out" => out = it.next().ok_or_else(value)?.into(),
            other => return Err(Error(format!("unknown argument {other}\n{USAGE}"))),
        }
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = std::process::Command::new(cargo)
        .current_dir(&root)
        .args([
            "run",
            "-q",
            // the Zed rev is pinned only in Cargo.lock
            "--locked",
            "--release",
            "-p",
            "ig-app",
            "--features",
            "screenshots",
            "--",
            "--screenshots",
        ])
        .arg(&out)
        .arg(&repo)
        .status()
        .map_err(|e| Error(format!("running cargo: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error(format!("ideagit --screenshots failed: {status}")))
    }
}
```

`xtask/src/output.rs`:

```rust
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
```

`xtask/src/source.rs`:

```rust
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
```

`xtask/src/strings.rs`:

```rust
//! Message bundle extraction from `assets/strings.list` (`<BundleName> <key>`). Bundles are
//! located by file name (`<BundleName>.properties`, which must be unique in the upstream tree) and
//! read as Java `.properties` files (`java.util.Properties` line rules: `=`, `:` or whitespace
//! separators, `\` continuation lines, `#`/`!` comments). Values are copied with their upstream
//! escaping unchanged (they are read upstream by `platform/util/src/com/intellij/AbstractBundle.kt`).

use std::collections::BTreeMap;

use crate::source::Source;
use crate::{Result, bail};

/// One property: the key and the raw value text (continuation lines included verbatim).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub key: String,
    pub raw_value: String,
}

/// Parses `.properties` text, keeping raw values.
pub fn parse_properties(text: &str) -> Vec<Property> {
    let mut props = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        // The key ends at the first unescaped `=`, `:` or whitespace.
        let mut end = trimmed.len();
        let mut escaped = false;
        for (i, c) in trimmed.char_indices() {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '=' || c == ':' || c.is_whitespace() {
                end = i;
                break;
            }
        }
        let key = trimmed[..end].to_string();
        let mut rest = trimmed[end..].trim_start_matches([' ', '\t', '\u{c}']);
        if let Some(r) = rest.strip_prefix(['=', ':']) {
            rest = r.trim_start_matches([' ', '\t', '\u{c}']);
        }
        let mut raw_value = rest.to_string();
        let mut continued = ends_with_continuation(rest);
        while continued {
            let Some(next) = lines.next() else { break };
            raw_value.push('\n');
            raw_value.push_str(next);
            continued = ends_with_continuation(next);
        }
        props.push(Property { key, raw_value });
    }
    props
}

fn ends_with_continuation(line: &str) -> bool {
    line.bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1
}

/// Reads every listed key; returns `(assets/strings/<Bundle>.properties, bytes)` per bundle.
pub fn collect(src: &dyn Source, list: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let mut bundles: BTreeMap<String, (String, Vec<Property>, String)> = BTreeMap::new();
    let mut missing = Vec::new();
    for (i, raw) in list.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(bundle), Some(key), None) = (parts.next(), parts.next(), parts.next()) else {
            bail!("strings.list:{}: expected `<BundleName> <key>`", i + 1)
        };
        if !bundles.contains_key(bundle) {
            let suffix = format!("/{bundle}.properties");
            let found: Vec<&str> = src
                .files()?
                .iter()
                .map(String::as_str)
                .filter(|p| p.ends_with(&suffix))
                .collect();
            let path = match found.as_slice() {
                [one] => one.to_string(),
                [] => bail!(
                    "strings.list:{}: no {bundle}.properties in {}",
                    i + 1,
                    src.describe()
                ),
                many => bail!(
                    "strings.list:{}: {bundle}.properties is ambiguous: {}",
                    i + 1,
                    many.join(", ")
                ),
            };
            let props = parse_properties(&src.read_text(&path)?);
            bundles.insert(bundle.to_string(), (path, props, String::new()));
        }
        let Some((path, props, out)) = bundles.get_mut(bundle) else {
            continue;
        };
        if out.is_empty() {
            out.push_str(&format!(
                "# generated by `cargo xtask import-ui` from {path}\n"
            ));
        }
        let entry = format!("{key}=");
        if out.lines().any(|l| l.starts_with(&entry)) {
            bail!("strings.list:{}: {bundle} {key} is listed twice", i + 1);
        }
        match props.iter().rev().find(|p| p.key == key) {
            Some(p) => out.push_str(&format!("{key}={}\n", p.raw_value)),
            None => missing.push(format!("{bundle} {key}")),
        }
    }
    if !missing.is_empty() {
        bail!(
            "strings.list: keys not found upstream: {}",
            missing.join(", ")
        );
    }
    Ok(bundles
        .into_iter()
        .map(|(bundle, (_, _, text))| {
            (
                format!("assets/strings/{bundle}.properties"),
                text.into_bytes(),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_raw_values_and_continuations() {
        let props = parse_properties(
            "# c\na.b=Loading\\u2026\nc.d : x\\\n   y\n! bang\ne.f {0,choice,1#a|2#b}\nesc\\=k=v\n",
        );
        let pairs: Vec<(&str, &str)> = props
            .iter()
            .map(|p| (p.key.as_str(), p.raw_value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("a.b", "Loading\\u2026"),
                ("c.d", "x\\\n   y"),
                ("e.f", "{0,choice,1#a|2#b}"),
                ("esc\\=k", "v")
            ]
        );
    }
}
```

`xtask/src/theme.rs`:

```rust
//! Theme and editor color scheme resolution, a simplified port of IntelliJ's UI theme loading:
//!
//! - `platform/platform-impl/src/com/intellij/ide/ui/UIThemeBean.kt` (`readFlatMapFromJson`,
//!   `putEntry`, `importFromParentTheme`): `ui` objects are flattened to dotted keys (a `UI`
//!   segment is appended without a dot), `os.default` values are kept, and a theme's `ui` and
//!   `colors` maps override its `parentTheme`'s.
//! - `platform/platform-impl/src/com/intellij/ide/ui/UITheme.kt` (`applyTheme`, `addPattern`):
//!   the merged `ui` map is applied in order; a value naming a `colors` entry becomes that color,
//!   `*.x` overrides every key applied so far that ends with `.x` and is remembered as a pattern.
//! - `platform/platform-impl/src/com/intellij/ide/ui/uiThemeParser.kt` and
//!   `platform/util/src/com/intellij/ui/ColorHexUtil.java`: `#RGB[A]` / `#RRGGBB[AA]` colors,
//!   alpha last.
//! - `platform/util/ui/src/com/intellij/ui/JBColor.java` (`findPatternMatch`): a color key that
//!   is not set falls back to a `*` pattern it ends with.
//! - `platform/editor-ui-ex/src/com/intellij/openapi/editor/colors/impl/AbstractColorsScheme.java`
//!   and `ValueElementReader.java`: scheme `<colors>`/`<attributes>`, `parent_scheme` chains,
//!   short hex values.
//!
//! Deviations: only `os.default` values are used (upstream prefers the running OS's `os.*` key);
//! LaF defaults that come from Java code are absent; a theme without `parentTheme` has no parent;
//! when several `*` patterns match a missing key, the longest wins (upstream: hash-map order).

use std::collections::HashMap;

use serde_json::{Map, Value as Json};

use crate::source::Source;
use crate::xml::{self, Element};
use crate::{Error, Result, bail};

pub const THEMES_DIR: &str = "platform/platform-resources/src/themes/";
pub const DEFAULT_SCHEMES: &str = "platform/platform-resources/src/DefaultColorSchemesManager.xml";
/// The New UI themes the tokens are generated for (theme `"name"`s).
pub const LIGHT_THEME: &str = "Light";
pub const DARK_THEME: &str = "Dark";

/// A theme `ui` value after parsing.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `0xRRGGBBAA`.
    Color(u32),
    Number(f64),
    Text(String),
    Bool(bool),
}

/// A resolved theme: its applied `ui` defaults plus its editor scheme chain.
pub struct Theme {
    pub name: String,
    /// Theme files from the theme itself up to the root parent.
    pub files: Vec<String>,
    defaults: HashMap<String, Value>,
    /// `*.x` patterns, keyed by `x`.
    patterns: HashMap<String, Value>,
    /// Editor schemes from the theme's `editorScheme` up to the root `parent_scheme`.
    pub schemes: Vec<(String, Element)>,
}

/// Parses `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA` (the `#` is optional), as `ColorHexUtil`.
pub fn parse_hex(s: &str) -> Option<u32> {
    let h = s.strip_prefix('#').or(s.strip_prefix("0x")).unwrap_or(s);
    if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok();
    // Expands nibble `i` (from the right) to a byte placed at byte position `at` (from the right).
    let byte = |i: u32, at: u32| v.map(|v| (((v >> (4 * i)) & 0xF) * 17) << (8 * at));
    match h.len() {
        3 => Some(byte(2, 3)? | byte(1, 2)? | byte(0, 1)? | 0xFF),
        4 => Some(byte(3, 3)? | byte(2, 2)? | byte(1, 1)? | byte(0, 0)?),
        6 => Some((v? << 8) | 0xFF),
        8 => v,
        _ => None,
    }
}

/// Parses a scheme color value as `ValueElementReader.toColor`: 6+ digits via `ColorHexUtil`,
/// otherwise a (left-padded) hex RGB integer.
pub fn parse_scheme_hex(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.len() >= 6
        && let Some(c) = parse_hex(s)
    {
        return Some(c);
    }
    let rgb = u32::from_str_radix(s, 16).ok()?;
    Some(((rgb & 0xFF_FFFF) << 8) | 0xFF)
}

fn is_color_like(s: &str) -> bool {
    s.len() <= 9 && s.starts_with('#')
}

/// A theme file: its path and parsed JSON object.
type ThemeFile = (String, Map<String, Json>);

/// The `name -> files` index of the theme files under [`THEMES_DIR`].
fn theme_index(src: &dyn Source) -> Result<HashMap<String, Vec<ThemeFile>>> {
    let mut index: HashMap<String, Vec<_>> = HashMap::new();
    for path in src.files_under(THEMES_DIR)? {
        if !path.ends_with(".theme.json") {
            continue;
        }
        let json: Json = serde_json::from_str(&src.read_text(path)?)
            .map_err(|e| Error(format!("{path}: invalid JSON: {e}")))?;
        let Json::Object(obj) = json else {
            bail!("{path}: not a JSON object")
        };
        if let Some(Json::String(name)) = obj.get("name") {
            index
                .entry(name.clone())
                .or_default()
                .push((path.to_string(), obj));
        }
    }
    Ok(index)
}

/// Flattens a theme's `ui` object as `readFlatMapFromJson` does, keeping key order.
fn flatten(obj: &Map<String, Json>, prefix: &mut Vec<String>, out: &mut Vec<(String, Value)>) {
    for (key, value) in obj {
        let path = |key: Option<&str>| {
            let mut path = String::new();
            for (i, e) in prefix.iter().enumerate() {
                if i > 0 && e != "UI" {
                    path.push('.');
                }
                path.push_str(e);
            }
            if let Some(key) = key {
                if key != "UI" && !path.is_empty() {
                    path.push('.');
                }
                path.push_str(key);
            }
            path
        };
        let parsed = match value {
            Json::Object(inner) => {
                prefix.push(key.clone());
                flatten(inner, prefix, out);
                prefix.pop();
                continue;
            }
            Json::Array(items) => match items.iter().rev().find_map(Json::as_str) {
                Some(s) => Value::Text(s.to_string()),
                None => continue,
            },
            Json::String(s) if is_color_like(s) => parse_hex(s)
                .map(Value::Color)
                .unwrap_or_else(|| Value::Text(s.clone())),
            Json::String(s) => Value::Text(s.clone()),
            Json::Number(n) => Value::Number(n.as_f64().unwrap_or_default()),
            Json::Bool(b) => Value::Bool(*b),
            Json::Null => continue,
        };
        match key.as_str() {
            "os.default" => {
                let composite = path(None);
                if !out.iter().any(|(k, _)| *k == composite) {
                    out.push((composite, parsed));
                }
            }
            k if k.starts_with("os.") => {}
            k => {
                let full = path(Some(k));
                match out.iter_mut().find(|(k, _)| *k == full) {
                    Some(slot) => slot.1 = parsed,
                    None => out.push((full, parsed)),
                }
            }
        }
    }
}

/// `importMapFromParentTheme`: parent entries the child doesn't set, then the child's entries.
fn merge<T: Clone>(parent: &[(String, T)], child: Vec<(String, T)>) -> Vec<(String, T)> {
    let mut result: Vec<(String, T)> = parent
        .iter()
        .filter(|(k, _)| !child.iter().any(|(c, _)| c == k))
        .cloned()
        .collect();
    result.extend(child);
    result
}

impl Theme {
    /// Loads the theme named `name` with its parent chain and editor scheme chain.
    pub fn load(src: &dyn Source, name: &str) -> Result<Theme> {
        let index = theme_index(src)?;
        let lookup = |name: &str| -> Result<&ThemeFile> {
            match index.get(name).map(Vec::as_slice) {
                Some([one]) => Ok(one),
                Some(many) => bail!(
                    "theme name {name:?} is ambiguous: {}",
                    many.iter()
                        .map(|(p, _)| p.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                None => bail!(
                    "no theme named {name:?} under {THEMES_DIR} in {}",
                    src.describe()
                ),
            }
        };
        let mut chain = vec![lookup(name)?];
        while let Some(Json::String(parent)) = chain.last().and_then(|(_, o)| o.get("parentTheme"))
        {
            let next = lookup(parent)?;
            if chain.iter().any(|(p, _)| *p == next.0) {
                bail!("theme {name:?}: parentTheme cycle at {}", next.0);
            }
            chain.push(next);
        }

        let mut ui: Vec<(String, Value)> = Vec::new();
        let mut colors: Vec<(String, Value)> = Vec::new();
        for (_, obj) in chain.iter().rev() {
            let mut own = Vec::new();
            if let Some(Json::Object(map)) = obj.get("ui") {
                flatten(map, &mut Vec::new(), &mut own);
            }
            ui = merge(&ui, own);
            let mut own_colors = Vec::new();
            if let Some(Json::Object(map)) = obj.get("colors") {
                flatten(map, &mut Vec::new(), &mut own_colors);
            }
            colors = merge(&colors, own_colors);
        }

        let palette: HashMap<&str, &Value> = colors.iter().map(|(k, v)| (k.as_str(), v)).collect();
        let resolve_named = |v: &Value| -> Value {
            let mut v = v.clone();
            for _ in 0..16 {
                match &v {
                    Value::Text(s) if palette.contains_key(s.as_str()) => {
                        v = palette[s.as_str()].clone()
                    }
                    _ => break,
                }
            }
            v
        };

        let mut defaults: HashMap<String, Value> = HashMap::new();
        let mut patterns: HashMap<String, Value> = HashMap::new();
        for (key, value) in &colors {
            if let Value::Color(_) = resolve_named(value) {
                defaults.insert(format!("ColorPalette.{key}"), resolve_named(value));
            }
        }
        for (key, value) in &ui {
            let value = resolve_named(value);
            if let Some(tail) = key.strip_prefix('*').filter(|t| t.starts_with('.')) {
                patterns.insert(tail[1..].to_string(), value.clone());
                for (k, v) in defaults.iter_mut() {
                    if k.ends_with(tail) {
                        *v = value.clone();
                    }
                }
            } else {
                defaults.insert(key.clone(), value);
            }
        }

        let schemes = match chain[0].1.get("editorScheme") {
            Some(Json::String(scheme)) => scheme_chain(src, scheme)?,
            _ => Vec::new(),
        };
        Ok(Theme {
            name: name.to_string(),
            files: chain.iter().map(|(p, _)| p.clone()).collect(),
            defaults,
            patterns,
            schemes,
        })
    }

    /// The color for a `ui` key, and the `*` pattern it came from if it wasn't set directly.
    pub fn color(&self, key: &str) -> Option<(u32, Option<String>)> {
        if let Some(Value::Color(c)) = self.defaults.get(key) {
            return Some((*c, None));
        }
        self.patterns
            .iter()
            .filter(|(tail, v)| key.ends_with(tail.as_str()) && matches!(v, Value::Color(_)))
            .max_by(|(a, _), (b, _)| a.len().cmp(&b.len()).then_with(|| b.cmp(a)))
            .and_then(|(tail, v)| match v {
                Value::Color(c) => Some((*c, Some(format!("*.{tail}")))),
                _ => None,
            })
    }

    /// The number for a `ui` key (set directly; numbers don't use `*` patterns).
    pub fn number(&self, key: &str) -> Option<f64> {
        match self.defaults.get(key) {
            Some(Value::Number(n)) => Some(*n),
            _ => None,
        }
    }

    /// A scheme `<colors>` option, from the nearest scheme in the chain that sets it.
    pub fn scheme_color(&self, option: &str) -> Option<u32> {
        for (_, scheme) in &self.schemes {
            let found = scheme
                .children_named("colors")
                .flat_map(|c| c.children_named("option"))
                .find(|o| o.attr("name") == Some(option));
            if let Some(o) = found {
                return o.attr("value").and_then(parse_scheme_hex);
            }
        }
        None
    }

    /// An attribute of a scheme `<attributes>` entry (e.g. `FOREGROUND`), following
    /// `baseAttributes` inheritance.
    pub fn scheme_attribute(&self, attr: &str, field: &str) -> Option<u32> {
        let mut name = attr.to_string();
        for _ in 0..16 {
            let found = self.schemes.iter().find_map(|(_, scheme)| {
                scheme
                    .children_named("attributes")
                    .flat_map(|a| a.children_named("option"))
                    .find(|o| o.attr("name") == Some(name.as_str()))
            })?;
            if let Some(value) = found.child("value") {
                let opt = value
                    .children_named("option")
                    .find(|o| o.attr("name") == Some(field))?;
                return opt.attr("value").and_then(parse_scheme_hex);
            }
            name = found.attr("baseAttributes")?.to_string();
        }
        None
    }
}

/// The scheme named `name` and its `parent_scheme` ancestors, from the XML files under
/// [`THEMES_DIR`] and [`DEFAULT_SCHEMES`].
fn scheme_chain(src: &dyn Source, name: &str) -> Result<Vec<(String, Element)>> {
    let mut schemes: HashMap<String, Vec<(String, Element)>> = HashMap::new();
    let mut paths: Vec<&str> = src
        .files_under(THEMES_DIR)?
        .into_iter()
        .filter(|p| p.ends_with(".xml"))
        .collect();
    paths.push(DEFAULT_SCHEMES);
    for path in paths {
        let root = xml::parse(&src.read_text(path)?).map_err(|e| Error(format!("{path}: {e}")))?;
        let found: Vec<Element> = if root.name == "scheme" {
            vec![root]
        } else {
            root.children_named("scheme").cloned().collect()
        };
        for scheme in found {
            if let Some(n) = scheme.attr("name") {
                schemes
                    .entry(n.to_string())
                    .or_default()
                    .push((path.to_string(), scheme));
            }
        }
    }
    let mut chain: Vec<(String, Element)> = Vec::new();
    let mut next = Some(name.to_string());
    while let Some(n) = next.take() {
        let (path, scheme) = match schemes.get(&n).map(Vec::as_slice) {
            Some([one]) => one.clone(),
            Some(_) => bail!("editor scheme {n:?} is defined more than once"),
            None => bail!(
                "no editor scheme named {n:?} in {} or {DEFAULT_SCHEMES}",
                THEMES_DIR
            ),
        };
        if chain
            .iter()
            .any(|(_, s)| s.attr("name") == Some(n.as_str()))
        {
            bail!("editor scheme parent cycle at {n:?}");
        }
        next = scheme.attr("parent_scheme").map(str::to_string);
        chain.push((path, scheme));
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_put_alpha_last() {
        assert_eq!(parse_hex("#3574F0"), Some(0x3574F0FF));
        assert_eq!(parse_hex("#FFFFFF1A"), Some(0xFFFFFF1A));
        assert_eq!(parse_hex("#abc"), Some(0xAABBCCFF));
        assert_eq!(parse_hex("#1234"), Some(0x11223344));
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("Gray1"), None);
        assert_eq!(parse_scheme_hex("0"), Some(0x000000FF));
        assert_eq!(parse_scheme_hex("7f0000"), Some(0x7F0000FF));
        assert_eq!(parse_scheme_hex("ffffff4d"), Some(0xFFFFFF4D));
        assert_eq!(parse_scheme_hex(""), None);
    }

    #[test]
    fn flatten_follows_upstream_key_rules() {
        let json: Json = serde_json::from_str(
            r##"{"A":{"B":{"c":"#000000"},"UI":"cls","x":{"os.mac":"#111111","os.default":"#222222"}},
                 "D.e":1.5,"Label":{"UI":{"x":"#333333"}}}"##,
        )
        .unwrap();
        let mut out = Vec::new();
        flatten(json.as_object().unwrap(), &mut Vec::new(), &mut out);
        let keys: Vec<&str> = out.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["A.B.c", "AUI", "A.x", "D.e", "LabelUI.x"]);
        assert_eq!(out[2].1, Value::Color(0x222222FF));
        assert_eq!(out[3].1, Value::Number(1.5));
    }
}
```

`xtask/src/tokens.rs`:

```rust
//! `assets/theme.keys` parsing and generation of `ThemeTokens` (`tokens.rs`) plus the
//! `assets/themes/{light,dark}.json` review files. Key lookups use [`crate::theme`].
//!
//! `theme.keys` lines: `[f32 ]<field> = <alt> | <alt> ... [# comment]`. An alternative is a
//! theme `ui` key, `scheme:<OPTION>` (scheme `<colors>`), `scheme:<ATTR>.FOREGROUND` (or another
//! upper-case attribute field), or a literal: `#RRGGBB[AA]`, a float, or `<light>/<dark>` for a
//! `JBColor(light, dark)`-style pair. Literals need a trailing comment saying where upstream
//! hardcodes the value. A comment starts at a `#` that begins the line or is followed by a space.

use std::fmt::Write as _;

use serde_json::{Map, Value as Json};

use crate::theme::{Theme, parse_hex};
use crate::{Error, Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Color,
    F32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Alt {
    Key(String),
    SchemeColor(String),
    SchemeAttribute(String, String),
    /// Light and dark literal (the same text when the literal isn't a pair).
    Literal(String, String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyLine {
    pub line: usize,
    pub field: String,
    pub kind: Kind,
    pub alts: Vec<Alt>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Resolved {
    Color(u32),
    Float(f64),
}

/// A field with the alternative that resolved for each theme (`(description, value)`).
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub light: (String, Resolved),
    pub dark: (String, Resolved),
}

#[derive(Debug, Clone, Copy)]
pub enum Variant {
    Light,
    Dark,
}

fn split_comment(line: &str) -> (&str, &str) {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        let starts = i == 0 || bytes[i - 1].is_ascii_whitespace();
        let ends = bytes.get(i + 1).is_none_or(u8::is_ascii_whitespace);
        if b == b'#' && starts && (ends || line[..i].trim().is_empty()) {
            return (&line[..i], line[i + 1..].trim());
        }
    }
    (line, "")
}

fn is_literal(s: &str) -> bool {
    s.starts_with('#') || s.starts_with(|c: char| c.is_ascii_digit()) || s.contains('/')
}

fn parse_alt(s: &str) -> Alt {
    if let Some(rest) = s.strip_prefix("scheme:") {
        if let Some((attr, field)) = rest.rsplit_once('.')
            && !field.is_empty()
            && field.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
        {
            return Alt::SchemeAttribute(attr.to_string(), field.to_string());
        }
        return Alt::SchemeColor(rest.to_string());
    }
    if is_literal(s) {
        let (light, dark) = s.split_once('/').unwrap_or((s, s));
        return Alt::Literal(light.trim().to_string(), dark.trim().to_string());
    }
    Alt::Key(s.to_string())
}

/// Parses `theme.keys`.
pub fn parse_keys(text: &str) -> Result<Vec<KeyLine>> {
    let mut lines: Vec<KeyLine> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let (body, comment) = split_comment(raw);
        let body = body.trim();
        if body.is_empty() {
            continue;
        }
        let Some((lhs, rhs)) = body.split_once('=') else {
            bail!("theme.keys:{line}: expected `<field> = <keys>`")
        };
        let lhs = lhs.trim();
        let (kind, field) = match lhs.strip_prefix("f32 ") {
            Some(f) => (Kind::F32, f.trim()),
            None => (Kind::Color, lhs),
        };
        if field.is_empty()
            || !field.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
            || !field
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            bail!("theme.keys:{line}: {field:?} is not a snake_case field name");
        }
        if lines.iter().any(|l| l.field == field) {
            bail!("theme.keys:{line}: field `{field}` is listed twice");
        }
        let alts: Vec<Alt> = rhs
            .split('|')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(parse_alt)
            .collect();
        if alts.is_empty() {
            bail!("theme.keys:{line}: field `{field}` has no keys");
        }
        if alts.iter().any(|a| matches!(a, Alt::Literal(..))) && comment.is_empty() {
            bail!(
                "theme.keys:{line}: field `{field}` uses a literal without a comment citing its upstream source"
            );
        }
        lines.push(KeyLine {
            line,
            field: field.to_string(),
            kind,
            alts,
        });
    }
    Ok(lines)
}

fn describe(alt: &Alt) -> String {
    match alt {
        Alt::Key(k) => k.clone(),
        Alt::SchemeColor(o) => format!("scheme:{o}"),
        Alt::SchemeAttribute(a, f) => format!("scheme:{a}.{f}"),
        Alt::Literal(l, d) if l == d => l.clone(),
        Alt::Literal(l, d) => format!("{l}/{d}"),
    }
}

fn resolve_one(line: &KeyLine, theme: &Theme, variant: Variant) -> Result<(String, Resolved)> {
    for alt in &line.alts {
        let found = match (alt, line.kind) {
            (Alt::Key(k), Kind::Color) => theme.color(k).map(|(c, via)| {
                let desc = match via {
                    Some(p) if p != *k => format!("{k} ({p})"),
                    _ => k.clone(),
                };
                (desc, Resolved::Color(c))
            }),
            (Alt::Key(k), Kind::F32) => theme.number(k).map(|n| (k.clone(), Resolved::Float(n))),
            (Alt::SchemeColor(o), Kind::Color) => theme
                .scheme_color(o)
                .map(|c| (describe(alt), Resolved::Color(c))),
            (Alt::SchemeAttribute(a, f), Kind::Color) => theme
                .scheme_attribute(a, f)
                .map(|c| (describe(alt), Resolved::Color(c))),
            (Alt::Literal(l, d), kind) => {
                let text = match variant {
                    Variant::Light => l,
                    Variant::Dark => d,
                };
                let value = match kind {
                    Kind::Color => parse_hex(text)
                        .filter(|_| text.starts_with('#'))
                        .map(Resolved::Color),
                    Kind::F32 => text.parse::<f64>().ok().map(Resolved::Float),
                };
                match value {
                    Some(v) => Some((format!("literal {text}"), v)),
                    None => bail!(
                        "theme.keys:{}: field `{}`: bad literal {text:?}",
                        line.line,
                        line.field
                    ),
                }
            }
            (_, Kind::F32) => bail!(
                "theme.keys:{}: f32 field `{}` can't use {}",
                line.line,
                line.field,
                describe(alt)
            ),
        };
        if let Some(found) = found {
            return Ok(found);
        }
    }
    let tried: Vec<String> = line
        .alts
        .iter()
        .map(|a| format!("`{}`", describe(a)))
        .collect();
    Err(Error(format!(
        "theme.keys:{}: field `{}`: none of {} resolved for theme {:?} ({})",
        line.line,
        line.field,
        tried.join(", "),
        theme.name,
        theme.files.first().map(String::as_str).unwrap_or_default(),
    )))
}

/// Resolves every field for both themes; fails on the first field that doesn't resolve.
pub fn resolve(lines: &[KeyLine], light: &Theme, dark: &Theme) -> Result<Vec<Field>> {
    lines
        .iter()
        .map(|line| {
            Ok(Field {
                name: line.field.clone(),
                light: resolve_one(line, light, Variant::Light)?,
                dark: resolve_one(line, dark, Variant::Dark)?,
            })
        })
        .collect()
}

fn rust_value(v: Resolved) -> String {
    match v {
        Resolved::Color(c) => format!("0x{c:08X}"),
        Resolved::Float(f) => format!("{:?}", f as f32),
    }
}

/// Renders `tokens.rs` (rustfmt-clean as generated).
pub fn render_rust(fields: &[Field], light: &Theme, dark: &Theme, commit: &str) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "//! @generated by `cargo xtask import-ui` from intellij-community {commit}. Do not edit."
    );
    s.push_str("//!\n//! Colors are `0xRRGGBBAA` (alpha last). Each field's doc names the upstream key that\n");
    s.push_str("//! resolved for the light / dark theme (see `assets/theme.keys`).\n\n");
    s.push_str("/// Colors and metrics of an IntelliJ New UI theme.\n");
    s.push_str("#[derive(Clone, Copy, Debug, PartialEq)]\npub struct ThemeTokens {\n");
    for f in fields {
        let ty = if matches!(f.light.1, Resolved::Float(_)) {
            "f32"
        } else {
            "u32"
        };
        let _ = writeln!(s, "    /// `{}` / `{}`", f.light.0, f.dark.0);
        let _ = writeln!(s, "    pub {}: {ty},", f.name);
    }
    s.push_str("}\n");
    for (name, theme, variant) in [
        ("LIGHT", light, Variant::Light),
        ("DARK", dark, Variant::Dark),
    ] {
        let file = theme.files.first().map(String::as_str).unwrap_or_default();
        let _ = writeln!(s, "\n/// The `{}` theme (`{file}`).", theme.name);
        let _ = writeln!(s, "pub const {name}: ThemeTokens = ThemeTokens {{");
        for f in fields {
            let v = match variant {
                Variant::Light => f.light.1,
                Variant::Dark => f.dark.1,
            };
            let _ = writeln!(s, "    {}: {},", f.name, rust_value(v));
        }
        s.push_str("};\n");
    }
    s
}

/// Renders `assets/themes/<variant>.json`: `{ field: { "key": ..., "value": ... } }`.
pub fn render_json(fields: &[Field], variant: Variant) -> String {
    let mut map = Map::new();
    for f in fields {
        let (key, value) = match variant {
            Variant::Light => &f.light,
            Variant::Dark => &f.dark,
        };
        let value = match *value {
            Resolved::Color(c) => Json::String(format!("#{c:08X}")),
            Resolved::Float(n) => serde_json::Number::from_f64(n)
                .map(Json::Number)
                .unwrap_or(Json::Null),
        };
        let mut entry = Map::new();
        entry.insert("key".into(), Json::String(key.clone()));
        entry.insert("value".into(), value);
        map.insert(f.name.clone(), Json::Object(entry));
    }
    let mut text = serde_json::to_string_pretty(&Json::Object(map)).unwrap_or_default();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_alternatives_comments_and_literals() {
        let keys = parse_keys(
            "# header\n\nf32 sat = A.b | 0.4 # DefaultColorGenerator.kt\nbg = *.background | scheme:X.FOREGROUND\n\
             err = Banner.e | #FFF7F7/#402929 # JBUI.java\nsb = scheme:ScrollBar.Mac.thumbColor\n",
        )
        .unwrap();
        assert_eq!(keys.len(), 4);
        assert_eq!(keys[0].kind, Kind::F32);
        assert_eq!(keys[0].alts[1], Alt::Literal("0.4".into(), "0.4".into()));
        assert_eq!(
            keys[1].alts[1],
            Alt::SchemeAttribute("X".into(), "FOREGROUND".into())
        );
        assert_eq!(
            keys[2].alts[1],
            Alt::Literal("#FFF7F7".into(), "#402929".into())
        );
        assert_eq!(
            keys[3].alts[0],
            Alt::SchemeColor("ScrollBar.Mac.thumbColor".into())
        );
    }

    #[test]
    fn literal_without_comment_is_rejected() {
        let err = parse_keys("x = A.b | #FFFFFF\n").unwrap_err();
        assert!(err.0.contains("`x`"), "{err}");
        assert!(parse_keys("Bad = A\n").is_err());
        assert!(parse_keys("x = A\nx = B\n").is_err());
    }
}
```

`xtask/src/xml.rs`:

```rust
//! A minimal XML reader for the upstream color scheme and keymap files: elements and attributes
//! only (text, comments, processing instructions, DOCTYPE and CDATA are skipped). Not a general
//! XML parser; it accepts the well-formed subset those files use.

use crate::{Error, Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }
}

/// Parses `text` and returns its root element.
pub fn parse(text: &str) -> Result<Element> {
    let mut p = Parser { s: text, pos: 0 };
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    while let Some(lt) = p.s[p.pos..].find('<') {
        p.pos += lt;
        let rest = &p.s[p.pos..];
        if rest.starts_with("<!--") {
            p.skip_past("-->")?;
        } else if rest.starts_with("<![CDATA[") {
            p.skip_past("]]>")?;
        } else if rest.starts_with("<?") {
            p.skip_past("?>")?;
        } else if rest.starts_with("<!") {
            p.skip_past(">")?;
        } else if rest.starts_with("</") {
            p.pos += 2;
            let name = p.name()?;
            p.skip_ws();
            p.expect('>')?;
            let Some(el) = stack.pop() else {
                bail!("unexpected </{name}> at byte {}", p.pos)
            };
            if el.name != name {
                bail!("</{name}> closes <{}> at byte {}", el.name, p.pos);
            }
            match stack.last_mut() {
                Some(parent) => parent.children.push(el),
                None => root = Some(el),
            }
        } else {
            p.pos += 1;
            let (el, closed) = p.start_tag()?;
            if !closed {
                stack.push(el);
            } else if let Some(parent) = stack.last_mut() {
                parent.children.push(el);
            } else {
                root = Some(el);
            }
        }
        if root.is_some() {
            break;
        }
    }
    if let Some(open) = stack.last() {
        bail!("unclosed <{}>", open.name);
    }
    root.ok_or_else(|| Error("no root element".into()))
}

struct Parser<'a> {
    s: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn skip_past(&mut self, end: &str) -> Result<()> {
        match self.s[self.pos..].find(end) {
            Some(i) => {
                self.pos += i + end.len();
                Ok(())
            }
            None => bail!(
                "unterminated markup at byte {} (expected {end:?})",
                self.pos
            ),
        }
    }

    fn skip_ws(&mut self) {
        let rest = &self.s[self.pos..];
        self.pos += rest.len() - rest.trim_start().len();
    }

    fn expect(&mut self, c: char) -> Result<()> {
        if self.s[self.pos..].starts_with(c) {
            self.pos += c.len_utf8();
            Ok(())
        } else {
            bail!("expected {c:?} at byte {}", self.pos)
        }
    }

    fn name(&mut self) -> Result<String> {
        let rest = &self.s[self.pos..];
        let len = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '=' | '/' | '>' | '"' | '\''))
            .unwrap_or(rest.len());
        if len == 0 {
            bail!("expected a name at byte {}", self.pos);
        }
        self.pos += len;
        Ok(rest[..len].to_string())
    }

    /// Parses after `<` up to and including `>`; returns the element and whether it self-closed.
    fn start_tag(&mut self) -> Result<(Element, bool)> {
        let name = self.name()?;
        let mut el = Element {
            name,
            attrs: Vec::new(),
            children: Vec::new(),
        };
        loop {
            self.skip_ws();
            let rest = &self.s[self.pos..];
            if rest.starts_with("/>") {
                self.pos += 2;
                return Ok((el, true));
            }
            if rest.starts_with('>') {
                self.pos += 1;
                return Ok((el, false));
            }
            let key = self.name()?;
            self.skip_ws();
            self.expect('=')?;
            self.skip_ws();
            let quote = match self.s[self.pos..].chars().next() {
                Some(q @ ('"' | '\'')) => q,
                _ => bail!("expected a quoted value for {key} at byte {}", self.pos),
            };
            self.pos += 1;
            let Some(end) = self.s[self.pos..].find(quote) else {
                bail!("unterminated value for {key} at byte {}", self.pos)
            };
            let raw = &self.s[self.pos..self.pos + end];
            self.pos += end + 1;
            el.attrs.push((key, unescape(raw)?));
        }
    }
}

fn unescape(raw: &str) -> Result<String> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest.find(';') else {
            bail!("bad entity in {raw:?}")
        };
        let entity = &rest[1..semi];
        let c = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code =
                    if let Some(hex) = entity.strip_prefix("#x").or(entity.strip_prefix("#X")) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        entity.strip_prefix('#').and_then(|d| d.parse().ok())
                    };
                match code.and_then(char::from_u32) {
                    Some(c) => c,
                    None => bail!("unknown entity &{entity}; in {raw:?}"),
                }
            }
        };
        out.push(c);
        rest = &rest[semi + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_elements_attributes_and_entities() {
        let doc = parse(
            "<?xml version=\"1.0\"?>\n<!-- c --><keymap name=\"$default\" parent='x &amp; y'>\
             <action id=\"A\"><keyboard-shortcut first-keystroke=\"control &#65;\"/></action>\
             <action id=\"B\"/>text</keymap>",
        )
        .unwrap();
        assert_eq!(doc.name, "keymap");
        assert_eq!(doc.attr("parent"), Some("x & y"));
        let actions: Vec<_> = doc.children_named("action").collect();
        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions[0].children[0].attr("first-keystroke"),
            Some("control A")
        );
        assert!(actions[1].children.is_empty());
    }

    #[test]
    fn rejects_mismatched_tags() {
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a>").is_err());
    }
}
```

- [ ] **Step 2: Tests and fixtures.** The fixture `.ttf` files are tiny binary stand-ins; copy them from the branch.

```bash
git checkout origin/wip/app-shell-prototype -- xtask/tests
```

`xtask/tests/fixtures/inputs/icons.list`:

```text
general/add platform/icons/src/expui/general/add.svg
vcs/branch platform/icons/src/expui/vcs/branch.svg
```

`xtask/tests/fixtures/inputs/keymap.list`:

```text
Vcs.Log.Refresh platform/vcs-log/resources/vcs-log.xml
Refresh
$Copy
Exit
HideActiveWindow
GotoClass
```

`xtask/tests/fixtures/inputs/strings.list`:

```text
TestBundle vcs.log.is.loading
TestBundle multi.line
TestBundle vcs.log.column.subject
```

`xtask/tests/fixtures/inputs/theme.keys`:

```text
# Fixture theme keys.
window_background = *.background
panel_background = Panel.background
foreground = Label.foreground
disabled_foreground = Label.disabledForeground
link = Link.activeForeground
table_selection_background = Table.selectionBackground
tree_background = Tree.background
f32 graph_saturation = VersionControl.Log.Graph.saturation | 0.4  # DefaultColorGenerator.kt: namedFloat(..., 0.4f)
banner_error_background = Banner.errorBackground | #FFF7F7/#402929  # JBUI.java: Banner.ERROR_BACKGROUND
console_background = scheme:CONSOLE_BACKGROUND_KEY
console_normal = scheme:CONSOLE_NORMAL_OUTPUT.FOREGROUND
console_error = scheme:CONSOLE_ERROR_OUTPUT.FOREGROUND
console_system = scheme:CONSOLE_SYSTEM_OUTPUT.FOREGROUND
```

`xtask/tests/fixtures/upstream/platform/icons/src/expui/general/add.svg`:

```xml
<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><path d="M8 2v12M2 8h12" stroke="#6C707E"/></svg>
```

`xtask/tests/fixtures/upstream/platform/icons/src/expui/general/add_dark.svg`:

```xml
<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><path d="M8 2v12M2 8h12" stroke="#CED0D6"/></svg>
```

`xtask/tests/fixtures/upstream/platform/icons/src/expui/vcs/branch.svg`:

```xml
<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><circle cx="8" cy="8" r="3"/></svg>
```

`xtask/tests/fixtures/upstream/platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts/inter/Inter-Bold.ttf`: binary, from the branch.

`xtask/tests/fixtures/upstream/platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts/inter/Inter-Medium.ttf`: binary, from the branch.

`xtask/tests/fixtures/upstream/platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts/inter/Inter-Regular.ttf`: binary, from the branch.

`xtask/tests/fixtures/upstream/platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts/inter/Inter-SemiBold.ttf`: binary, from the branch.

`xtask/tests/fixtures/upstream/platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts/jetbrains-mono/JetBrainsMono-Bold.ttf`: binary, from the branch.

`xtask/tests/fixtures/upstream/platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf`: binary, from the branch.

`xtask/tests/fixtures/upstream/platform/platform-resources/src/DefaultColorSchemesManager.xml`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<list>
  <scheme name="Default" default_scheme="true" version="1">
    <colors>
      <option name="CONSOLE_BACKGROUND_KEY" value="ffffff"/>
    </colors>
    <attributes>
      <option name="CONSOLE_NORMAL_OUTPUT">
        <value>
          <option name="FOREGROUND" value="0"/>
        </value>
      </option>
      <option name="CONSOLE_ERROR_OUTPUT">
        <value>
          <option name="FOREGROUND" value="7f0000" deuteranopia="d95716"/>
        </value>
      </option>
      <option name="CONSOLE_SYSTEM_OUTPUT">
        <value>
          <option name="FOREGROUND" value="00007f"/>
        </value>
      </option>
    </attributes>
  </scheme>
  <scheme name="Darcula" version="1" parent_scheme="Default">
    <colors>
      <option name="CONSOLE_BACKGROUND_KEY" value="2B2B2B" />
    </colors>
    <attributes>
      <option name="CONSOLE_NORMAL_OUTPUT">
        <value>
          <option name="FOREGROUND" value="bbbbbb" />
        </value>
      </option>
    </attributes>
  </scheme>
</list>
```

`xtask/tests/fixtures/upstream/platform/platform-resources/src/keymaps/$default.xml`:

```xml
<keymap name="$default" version="1" disable-mnemonics="false">
  <action id="Refresh">
    <keyboard-shortcut first-keystroke="control F5"/>
  </action>
  <action id="$Copy">
    <keyboard-shortcut first-keystroke="control C"/>
    <keyboard-shortcut first-keystroke="control INSERT"/>
  </action>
  <action id="HideActiveWindow">
    <keyboard-shortcut first-keystroke="shift ESCAPE"/>
  </action>
  <action id="GotoClass">
    <keyboard-shortcut first-keystroke="control N"/>
    <keyboard-shortcut first-keystroke="control shift A" second-keystroke="C"/>
  </action>
</keymap>
```

`xtask/tests/fixtures/upstream/platform/platform-resources/src/keymaps/Mac OS X 10.5+.xml`:

```xml
<keymap name="Mac OS X 10.5+" parent="$default" version="1" disable-mnemonics="true">
  <action id="Refresh">
    <keyboard-shortcut first-keystroke="meta R"/>
  </action>
  <action id="Exit">
    <keyboard-shortcut first-keystroke="meta Q"/>
  </action>
  <action id="HideActiveWindow"/>
</keymap>
```

`xtask/tests/fixtures/upstream/platform/platform-resources/src/themes/base.theme.json`:

```json
{
  "name": "Base",
  "dark": true,
  "colors": {
    "grey": "#808080"
  },
  "ui": {
    "*": {
      "background": "#111111",
      "foreground": "grey"
    },
    "Panel": { "background": "#222222" },
    "Label": { "foreground": "#333333", "disabledForeground": { "os.mac": "#444444", "os.default": "#555555" } },
    "Table.selectionBackground": "#666666"
  }
}
```

`xtask/tests/fixtures/upstream/platform/platform-resources/src/themes/expUI/dark.theme.json`:

```json
{
  "name": "Dark",
  "dark": true,
  "parentTheme": "Base",
  "editorScheme": "Dark",
  "ui": {
    "Link": { "activeForeground": "#ABC" }
  }
}
```

`xtask/tests/fixtures/upstream/platform/platform-resources/src/themes/expUI/darkScheme.xml`:

```xml
<scheme name="Dark" version="142" parent_scheme="Darcula">
  <attributes>
    <option name="CONSOLE_SYSTEM_OUTPUT" baseAttributes="CONSOLE_NORMAL_OUTPUT" />
  </attributes>
</scheme>
```

`xtask/tests/fixtures/upstream/platform/platform-resources/src/themes/expUI/light.theme.json`:

```json
{
  "name": "Light",
  "dark": false,
  "parentTheme": "Base",
  "editorScheme": "Light",
  "colors": {
    "grey": "#999999",
    "Accent": "Blue",
    "Blue": "#3574F0"
  },
  "ui": {
    "*": { "background": "#FAFAFA" },
    "Label": { "foreground": "grey" },
    "Link": { "activeForeground": "#11223344" },
    "Table.selectionBackground": "Accent",
    "VersionControl": { "Log": { "Graph": { "saturation": 0.5 } } }
  }
}
```

`xtask/tests/fixtures/upstream/platform/platform-resources/src/themes/expUI/lightScheme.xml`:

```xml
<scheme name="Light" version="142" parent_scheme="Default">
  <colors>
    <option name="CONSOLE_BACKGROUND_KEY" value="fafafa" />
  </colors>
</scheme>
```

`xtask/tests/fixtures/upstream/platform/platform-tests/testData/fonts/Inconsolata_LICENSE.txt`:

```text
Copyright (c) 2011, Fixture Font Authors

This Font Software is licensed under the SIL Open Font License, Version 1.1.
Fixture OFL text.
```

`xtask/tests/fixtures/upstream/platform/test/resources/messages/TestBundle.properties`:

```text
# A test bundle
vcs.log.column.subject=Subject
vcs.log.is.loading=Loading VCS Log…
multi.line=first \
  second
date.format.minutes.ago={0,choice, 0#Moments|1#A minute} ago
```

`xtask/tests/fixtures/upstream/platform/vcs-log/resources/vcs-log.xml`:

```xml
<idea-plugin>
  <actions>
    <group id="Vcs.Log.Toolbar">
      <action class="RefreshLogAction" id="Vcs.Log.Refresh" use-shortcut-of="Refresh"/>
    </group>
  </actions>
</idea-plugin>
```

`xtask/tests/import_ui.rs`:

```rust
//! `import-ui` over the fixture tree in `tests/fixtures/upstream` (read as a plain directory).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use xtask::{FsSource, import_ui};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// A temporary workspace with the fixture input lists under `assets/`.
fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    for name in ["theme.keys", "icons.list", "strings.list", "keymap.list"] {
        std::fs::copy(
            fixtures().join("inputs").join(name),
            dir.path().join("assets").join(name),
        )
        .unwrap();
    }
    dir
}

fn run(out: &Path) -> xtask::Result<xtask::Summary> {
    let source = FsSource::new(&fixtures().join("upstream")).unwrap();
    import_ui(&source, out, COMMIT)
}

fn read(out: &Path, rel: &str) -> String {
    std::fs::read_to_string(out.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Every file under `dir` with its bytes.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, &mut out);
    out
}

/// The `field: value,` line of `const NAME` in the generated tokens.
fn const_field(tokens: &str, name: &str, field: &str) -> String {
    let start = tokens
        .find(&format!("pub const {name}: ThemeTokens"))
        .unwrap();
    let body = &tokens[start..start + tokens[start..].find("};").unwrap()];
    let prefix = format!("    {field}: ");
    body.lines()
        .find_map(|l| l.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("{name}.{field} missing"))
        .trim_end_matches(',')
        .to_string()
}

#[test]
fn full_import_writes_expected_outputs() {
    let ws = workspace();
    let out = ws.path();
    run(out).unwrap();

    let tokens = read(out, "crates/ig-ui-kit/src/theme/tokens.rs");
    assert!(tokens.starts_with(&format!(
        "//! @generated by `cargo xtask import-ui` from intellij-community {COMMIT}. Do not edit.\n"
    )));
    assert!(
        tokens
            .contains("    /// `*.background` / `*.background`\n    pub window_background: u32,\n")
    );
    assert!(
        tokens.contains(
            "    /// `Tree.background (*.background)` / `Tree.background (*.background)`\n"
        )
    );
    assert!(tokens.contains(
        "    /// `VersionControl.Log.Graph.saturation` / `literal 0.4`\n    pub graph_saturation: f32,\n"
    ));
    assert!(!tokens.contains("#!["));
    let expect = [
        // `*` pattern; the child's `*.background` overrides the parent's.
        ("window_background", "0xFAFAFAFF", "0x111111FF"),
        // Light: the child's later `*.background` overrides the parent's `Panel.background`;
        // dark: the parent's `Panel.background` comes after its own `*.background`.
        ("panel_background", "0xFAFAFAFF", "0x222222FF"),
        // Parent override of a `ui` key and of the named color it uses.
        ("foreground", "0x999999FF", "0x333333FF"),
        // `os.default` wins over other `os.*` keys.
        ("disabled_foreground", "0x555555FF", "0x555555FF"),
        // `#RRGGBBAA` keeps alpha last; `#RGB` expands.
        ("link", "0x11223344", "0xAABBCCFF"),
        // Named color resolved through another named color.
        ("table_selection_background", "0x3574F0FF", "0x666666FF"),
        // Missing key: `*` pattern fallback.
        ("tree_background", "0xFAFAFAFF", "0x111111FF"),
        ("graph_saturation", "0.5", "0.4"),
        ("banner_error_background", "0xFFF7F7FF", "0x402929FF"),
        // Scheme colors and attributes through the parent_scheme chain; short hex is padded.
        ("console_background", "0xFAFAFAFF", "0x2B2B2BFF"),
        ("console_normal", "0x000000FF", "0xBBBBBBFF"),
        ("console_error", "0x7F0000FF", "0x7F0000FF"),
        // Dark: `baseAttributes` inheritance.
        ("console_system", "0x00007FFF", "0xBBBBBBFF"),
    ];
    for (field, light, dark) in expect {
        assert_eq!(const_field(&tokens, "LIGHT", field), light, "LIGHT.{field}");
        assert_eq!(const_field(&tokens, "DARK", field), dark, "DARK.{field}");
    }

    let light_json: serde_json::Value =
        serde_json::from_str(&read(out, "assets/themes/light.json")).unwrap();
    assert_eq!(light_json["link"]["key"], "Link.activeForeground");
    assert_eq!(light_json["link"]["value"], "#11223344");
    assert_eq!(light_json["graph_saturation"]["value"], 0.5);
    let dark_json: serde_json::Value =
        serde_json::from_str(&read(out, "assets/themes/dark.json")).unwrap();
    assert_eq!(
        dark_json["banner_error_background"]["key"],
        "literal #402929"
    );

    assert_eq!(
        read(out, "assets/icons/general/add.svg"),
        read(
            &fixtures(),
            "upstream/platform/icons/src/expui/general/add.svg"
        )
    );
    assert!(out.join("assets/icons/general/add_dark.svg").is_file());
    assert!(out.join("assets/icons/vcs/branch.svg").is_file());
    assert!(!out.join("assets/icons/vcs/branch_dark.svg").exists());

    assert_eq!(
        read(out, "assets/strings/TestBundle.properties"),
        "# generated by `cargo xtask import-ui` from platform/test/resources/messages/TestBundle.properties\n\
         vcs.log.is.loading=Loading VCS Log…\n\
         multi.line=first \\\n  second\n\
         vcs.log.column.subject=Subject\n"
    );

    let header = |name: &str| {
        format!(
            "# generated by `cargo xtask import-ui` from intellij-community {COMMIT} keymap {name:?}\n\
             # <action id>\\t<first keystroke>[\\t<second keystroke>]\n"
        )
    };
    assert_eq!(
        read(out, "assets/keymaps/default.tsv"),
        header("$default")
            + "Vcs.Log.Refresh\tcontrol F5\nRefresh\tcontrol F5\n$Copy\tcontrol C\n$Copy\tcontrol INSERT\n\
               HideActiveWindow\tshift ESCAPE\nGotoClass\tcontrol N\nGotoClass\tcontrol shift A\tC\n"
    );
    // Inherited shortcuts swap control/meta and drop INSERT; an empty <action> clears the parent's.
    assert_eq!(
        read(out, "assets/keymaps/macos.tsv"),
        header("Mac OS X 10.5+")
            + "Vcs.Log.Refresh\tmeta R\nRefresh\tmeta R\n$Copy\tmeta C\nExit\tmeta Q\n\
               GotoClass\tmeta N\nGotoClass\tmeta shift A\tC\n"
    );

    for font in [
        "Inter-Regular",
        "Inter-Medium",
        "Inter-SemiBold",
        "Inter-Bold",
        "JetBrainsMono-Regular",
        "JetBrainsMono-Bold",
    ] {
        assert!(
            out.join(format!("assets/fonts/{font}.ttf")).is_file(),
            "{font}"
        );
    }
    assert_eq!(
        read(out, "assets/fonts/Inter-OFL.txt"),
        "Copyright 2016 Fixture Inter Authors\n\n\
         This Font Software is licensed under the SIL Open Font License, Version 1.1.\nFixture OFL text.\n"
    );
    assert!(
        read(out, "assets/fonts/JetBrainsMono-OFL.txt")
            .starts_with("Copyright 2020 Fixture Mono Authors\n")
    );
}

#[test]
fn import_is_idempotent_and_prunes_stale_outputs() {
    let ws = workspace();
    let out = ws.path();
    run(out).unwrap();
    let first = snapshot(out);
    std::fs::write(out.join("assets/icons/general/stale.svg"), "<svg/>").unwrap();

    let summary = run(out).unwrap();
    assert!(summary.written.is_empty(), "rewrote {:?}", summary.written);
    assert_eq!(summary.removed, ["assets/icons/general/stale.svg"]);
    assert_eq!(snapshot(out), first);
}

#[test]
fn missing_theme_key_names_the_field_keys_and_theme() {
    let ws = workspace();
    let keys = ws.path().join("assets/theme.keys");
    let mut text = std::fs::read_to_string(&keys).unwrap();
    text.push_str("ghost_color = Ghost.color | scheme:GHOST\n");
    std::fs::write(&keys, text).unwrap();

    let err = run(ws.path()).unwrap_err().to_string();
    assert!(err.contains("`ghost_color`"), "{err}");
    assert!(
        err.contains("`Ghost.color`") && err.contains("`scheme:GHOST`"),
        "{err}"
    );
    assert!(err.contains("\"Light\""), "{err}");
    // Nothing is written when resolution fails.
    assert!(!ws.path().join("crates").exists());
}

#[test]
fn missing_icon_fails() {
    let ws = workspace();
    std::fs::write(
        ws.path().join("assets/icons.list"),
        "general/nope platform/icons/src/expui/general/nope.svg\n",
    )
    .unwrap();
    let err = run(ws.path()).unwrap_err().to_string();
    assert!(err.contains("general/nope"), "{err}");
}

#[test]
fn missing_string_fails() {
    let ws = workspace();
    std::fs::write(
        ws.path().join("assets/strings.list"),
        "TestBundle no.such.key\n",
    )
    .unwrap();
    let err = run(ws.path()).unwrap_err().to_string();
    assert!(err.contains("TestBundle no.such.key"), "{err}");

    std::fs::write(ws.path().join("assets/strings.list"), "NoBundle some.key\n").unwrap();
    let err = run(ws.path()).unwrap_err().to_string();
    assert!(err.contains("NoBundle.properties"), "{err}");
}
```

- [ ] **Step 3: Hand-written input lists** under `assets/`.

```bash
git checkout origin/wip/app-shell-prototype -- assets/theme.keys assets/icons.list assets/strings.list assets/keymap.list
```

`assets/theme.keys`:

```text
# Theme tokens for `cargo xtask import-ui` -> crates/ig-ui-kit/src/theme/tokens.rs.
#
# `[f32 ]<rust_field> = <alt> | <alt> ...`, tried left to right for the New UI "Light" and "Dark"
# themes (platform/platform-resources/src/themes/expUI/). An alternative is a theme `ui` key
# (missing keys fall back to `*` patterns, as JBColor.namedColor does), `scheme:<OPTION>` (editor
# scheme <colors>), `scheme:<ATTR>.FOREGROUND` (editor scheme <attributes>), or a literal
# (`#RRGGBB[AA]`, a float, or `<light>/<dark>`), which needs a trailing comment citing where
# upstream hardcodes it. Generation fails if no alternative resolves for a theme.

# Base
window_background = *.background
panel_background = Panel.background
foreground = Label.foreground
secondary_foreground = Label.infoForeground | Component.infoForeground
disabled_foreground = Label.disabledForeground
border = Borders.color
focus_color = Component.focusColor
link = Link.activeForeground

# Tables (the commit log)
table_background = Table.background
table_foreground = Table.foreground
table_selection_background = Table.selectionBackground
table_selection_foreground = Table.selectionForeground
table_selection_inactive_background = Table.selectionInactiveBackground
table_selection_inactive_foreground = Table.selectionInactiveForeground
table_header_separator = TableHeader.bottomSeparatorColor

# VCS log
log_current_branch_background = VersionControl.Log.Commit.currentBranchBackground
log_hovered_background = VersionControl.Log.Commit.hoveredBackground
log_unmatched_foreground = VersionControl.Log.Commit.unmatchedForeground
log_reference_foreground = VersionControl.Log.Commit.Reference.foreground
f32 graph_saturation = VersionControl.Log.Graph.saturation | 0.4  # DefaultColorGenerator.kt: namedFloat("VersionControl.Log.Graph.saturation", 0.4f)
f32 graph_brightness = VersionControl.Log.Graph.brightness | 0.65  # DefaultColorGenerator.kt: namedFloat("VersionControl.Log.Graph.brightness", 0.65f)

# Ref labels
ref_head = VersionControl.GitLog.headIconColor
ref_local_branch = VersionControl.GitLog.localBranchIconColor
ref_remote_branch = VersionControl.GitLog.remoteBranchIconColor
ref_tag = VersionControl.GitLog.tagIconColor
ref_other = VersionControl.GitLog.otherIconColor

# Tool windows
tool_window_background = ToolWindow.background
tool_window_header_background = ToolWindow.Header.background
tool_window_header_inactive_background = ToolWindow.Header.inactiveBackground
tool_window_header_tab_hover = ToolWindow.HeaderTab.hoverBackground
# New UI content tabs are underlined, not filled: TabContentLayout paints them with
# JBTabPainter.TOOL_WINDOW, whose selected background is UIManager.getColor(
# "ToolWindow.HeaderTab.underlinedTabBackground"), which no New UI theme sets.
tool_window_header_tab_selected = ToolWindow.HeaderTab.underlinedTabBackground | #00000000  # JBUI.CurrentTheme.ToolWindow.underlinedTabBackground() returns null -> no fill (JBDefaultTabPainter.paintSelectedTab)
tool_window_tab_underline = ToolWindow.HeaderTab.underlineColor

# Tool window stripes
stripe_background = ToolWindow.Stripe.background
stripe_separator = ToolWindow.Stripe.separatorColor
stripe_button_hover_background = ToolWindow.Button.hoverBackground
stripe_button_selected_background = ToolWindow.Button.selectedBackground
stripe_button_selected_foreground = ToolWindow.Button.selectedForeground
stripe_button_foreground = ToolWindow.Button.foreground

# Main toolbar
main_toolbar_background = MainToolbar.background
main_toolbar_foreground = MainToolbar.foreground

# Status bar
status_bar_background = StatusBar.background
status_bar_border = StatusBar.borderColor
status_bar_widget_foreground = StatusBar.Widget.foreground
status_bar_widget_hover_background = StatusBar.Widget.hoverBackground

# Notifications (balloons)
notification_background = Notification.background
notification_foreground = Notification.foreground
notification_border = Notification.borderColor
notification_error_foreground = Label.errorForeground

# Banners: no theme sets Banner.*, so JBUI's JBColor.namedColor(key, light, dark) defaults apply.
banner_error_background = Banner.errorBackground | #FFF7F7/#402929  # JBUI.CurrentTheme.Banner.ERROR_BACKGROUND: hardcoded in JBUI.java
banner_error_border = Banner.errorBorderColor | #FAD4D8/#5E3838  # JBUI.CurrentTheme.Banner.ERROR_BORDER_COLOR: hardcoded in JBUI.java
banner_warning_background = Banner.warningBackground | #FFFAEB/#3D3223  # JBUI.CurrentTheme.Banner.WARNING_BACKGROUND: hardcoded in JBUI.java
banner_warning_border = Banner.warningBorderColor | #FED277/#5E4D33  # JBUI.CurrentTheme.Banner.WARNING_BORDER_COLOR: hardcoded in JBUI.java
banner_info_background = Banner.infoBackground | #F5F8FE/#25324D  # JBUI.CurrentTheme.Banner.INFO_BACKGROUND: hardcoded in JBUI.java
banner_info_border = Banner.infoBorderColor | #C2D6FC/#35538F  # JBUI.CurrentTheme.Banner.INFO_BORDER_COLOR: hardcoded in JBUI.java

# Buttons
button_background = Button.startBackground
button_foreground = Button.foreground
button_border = Button.startBorderColor
button_default_background = Button.default.startBackground
button_default_foreground = Button.default.foreground
button_default_border = Button.default.startBorderColor

# Inputs, lists, popups
text_field_background = TextField.background
text_field_border = Component.borderColor
list_hover_background = List.hoverBackground
list_selection_background = List.selectionBackground
list_selection_inactive_background = List.selectionInactiveBackground
popup_background = Popup.background
popup_border = Popup.borderColor
menu_selection_background = MenuItem.selectionBackground
tooltip_background = ToolTip.background
tooltip_foreground = ToolTip.foreground
# Scroll bar colors are editor scheme ColorKeys (ScrollBarPainter.java); the macOS keys are the
# ones the New UI schemes tune.
scrollbar_thumb = scheme:ScrollBar.Mac.thumbColor | scheme:ScrollBar.thumbColor
scrollbar_thumb_hover = scheme:ScrollBar.Mac.hoverThumbColor | scheme:ScrollBar.hoverThumbColor

# Welcome screen
welcome_background = WelcomeScreen.background
welcome_details_background = WelcomeScreen.Details.background
welcome_project_selection_background = WelcomeScreen.Projects.selectionBackground

# Console (editor scheme)
console_background = scheme:CONSOLE_BACKGROUND_KEY
console_normal = scheme:CONSOLE_NORMAL_OUTPUT.FOREGROUND
console_error = scheme:CONSOLE_ERROR_OUTPUT.FOREGROUND
console_system = scheme:CONSOLE_SYSTEM_OUTPUT.FOREGROUND
```

`assets/icons.list`:

```text
# Icons for `cargo xtask import-ui`: `<logical/name> <upstream path to the light svg>`.
# New UI ("expui") icons; `<name>_dark.svg` next to a light icon is copied as its dark variant.
# Old-UI equivalents are in platform/icons/src/PlatformIconMappings.json (expui -> classic).

# Tool windows
toolwindows/git platform/icons/src/expui/toolwindows/vcs.svg
toolwindows/commit platform/icons/src/expui/toolwindows/commit.svg
toolwindows/project platform/icons/src/expui/toolwindows/project.svg
toolwindows/structure platform/icons/src/expui/toolwindows/structure.svg
toolwindows/terminal plugins/terminal/resources/icons/expui/toolwindow/terminal.svg
toolwindows/problems platform/icons/src/expui/toolwindows/problems.svg
toolwindows/notifications platform/icons/src/expui/toolwindows/notifications.svg

# General
general/refresh platform/icons/src/expui/general/refresh.svg
general/settings platform/icons/src/expui/general/settings.svg
general/search platform/icons/src/expui/general/search.svg
general/moreVertical platform/icons/src/expui/general/moreVertical.svg
general/hide platform/icons/src/expui/general/hide.svg
general/close platform/icons/src/expui/general/close.svg
general/chevronDown platform/icons/src/expui/general/chevronDown.svg
general/chevronRight platform/icons/src/expui/general/chevronRight.svg
general/add platform/icons/src/expui/general/add.svg
general/warning platform/icons/src/expui/status/warning.svg
general/error platform/icons/src/expui/status/error.svg
general/information platform/icons/src/expui/status/info.svg
general/folder platform/icons/src/expui/nodes/folder.svg
general/collapseAll platform/icons/src/expui/general/collapseAll.svg
general/expandAll platform/icons/src/expui/general/expandAll.svg

# VCS
vcs/branch platform/icons/src/expui/general/vcs.svg
# New UI variant of the current-branch label (DvcsIconMappings.json maps icons/new/ for the New UI)
vcs/currentBranch platform/dvcs-impl/shared/resources/icons/new/currentBranchLabel.svg
vcs/tag platform/icons/src/expui/nodes/tag.svg
vcs/commit platform/icons/src/expui/vcs/commit.svg
vcs/fetch platform/icons/src/expui/vcs/fetch.svg
vcs/push platform/icons/src/expui/vcs/push.svg
vcs/update platform/icons/src/expui/vcs/update.svg

# Welcome screen
welcome/open platform/icons/src/expui/welcome/open.svg
welcome/clone platform/icons/src/expui/welcome/fromVCSTab.svg
welcome/newProject platform/icons/src/expui/welcome/createNewProjectTab.svg
nodes/ideaProject platform/icons/src/expui/nodes/ideaProject.svg
```

`assets/strings.list`:

```text
# Message bundle keys for `cargo xtask import-ui`: `<BundleName> <key>`.
# Output: assets/strings/<BundleName>.properties with the listed keys in this order.

# Log columns
VcsLogBundle vcs.log.column.subject
VcsLogBundle vcs.log.column.author
VcsLogBundle vcs.log.column.date
VcsLogBundle vcs.log.column.hash
# Log status texts
VcsLogBundle vcs.log.is.loading
VcsLogBundle vcs.log.loading.status
VcsLogBundle vcs.log.error.loading.commits.status
VcsLogBundle vcs.log.no.commits.status
VcsLogBundle vcs.log.is.not.available
VcsLogBundle vcs.log.refresh.status.action
# Refresh action
VcsLogBundle action.Vcs.Log.Refresh.text
VcsLogBundle action.Vcs.Log.Refresh.description
# Tabs and labels
VcsLogBundle vcs.log.tab.name
VcsLogBundle vcs.log.references.detached.head

# Dates (DateFormatUtil)
UtilBundle date.format.minutes.ago
UtilBundle date.format.today
UtilBundle date.format.yesterday

# Git
GitBundle git4idea.vcs.name
GitBundle executable.error.git.not.installed
GitBundle executable.error.git.not.installed.hint
GitBundle git.executable.validation.error.start.title
GitBundle git.executable.validation.error.version.title
GitBundle git.executable.validation.error.version.message

# VCS tool windows
VcsBundle vcs.console.toolwindow.display.name
VcsBundle toolwindow.stripe.Commit
```

`assets/keymap.list`:

```text
# IntelliJ action ids for `cargo xtask import-ui`: `<action id> [<upstream XML declaring it>]`.
# The declaring XML is read for `use-shortcut-of` bindings.
# Output: assets/keymaps/{default,macos}.tsv ($default and "Mac OS X 10.5+").

Vcs.Log.Refresh platform/vcs-log/impl/resources/intellij.platform.vcs.log.impl.xml
OpenFile
CloseProject
Exit
ActivateVersionControlToolWindow
ActivateProjectToolWindow
ActivateCommitToolWindow
About
$SelectAll
$Copy
EditorUp
EditorDown
EditorUpWithSelection
EditorDownWithSelection
EditorPageUp
EditorPageDown
EditorTextStart
EditorTextEnd
HideActiveWindow
Refresh
```

- [ ] **Step 4: `NOTICE`** gains the font attribution.

```bash
git checkout origin/wip/app-shell-prototype -- NOTICE
```

`NOTICE`:

```text
IdeaGit
Copyright 2026 The IdeaGit contributors

This product includes software ported from IntelliJ IDEA Community Edition
(https://github.com/JetBrains/intellij-community),
Copyright 2000-2026 JetBrains s.r.o. and contributors,
licensed under the Apache License, Version 2.0.

Ported Rust modules name their upstream sources in their documentation;
reference/upstream-map.toml lists all of them together with the upstream commit they match.

The NOTICE file of intellij-community reads:

    This software includes code from IntelliJ IDEA
    Copyright (C) JetBrains s.r.o.
    https://www.jetbrains.com/idea/

Test data under crates/ig-git/tests/fixtures/repo is copied unchanged from
intellij-community/plugins/git4idea/testData/repo.

Fonts under assets/fonts are copied unchanged from
intellij-community/platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts
(commit in reference/UPSTREAM) and are licensed under the SIL Open Font License, Version 1.1
(see assets/fonts/*-OFL.txt):
  - Inter (Inter-Regular/Medium/SemiBold/Bold.ttf): Copyright 2016 The Inter Project Authors
  - JetBrains Mono (JetBrainsMono-Regular/Bold.ttf): Copyright 2020 The JetBrains Mono Project
    Authors (https://github.com/JetBrains/JetBrainsMono)
```

- [ ] **Step 5: Generate the assets** from the pinned upstream clone. This writes `assets/{themes,icons,strings,keymaps,fonts}` and `crates/ig-ui-kit/src/theme/tokens.rs` (not yet compiled: `ig-ui-kit` declares `theme` in the next task). Expected: the generated files equal the prototype's. If the upstream clone can't be read, check them out from `origin/wip/app-shell-prototype` instead and say so in the report.

```bash
cargo xtask import-ui --upstream ~/Code/intellij-community
git add -A && git diff --cached --exit-code origin/wip/app-shell-prototype -- assets crates/ig-ui-kit/src/theme/tokens.rs
```

- [ ] **Step 6: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code origin/wip/app-shell-prototype -- xtask/src xtask/tests assets/theme.keys assets/icons.list assets/strings.list assets/keymap.list NOTICE crates/ig-ui-kit/src/theme/tokens.rs assets/themes assets/icons assets/strings assets/keymaps assets/fonts
```

Expected: no output. The intermediate files written above differ from the branch until their final task.

- [ ] **Step 7: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 411 passed, 1 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(xtask): import-ui (theme tokens, icons, strings, keymaps, fonts) and the imported assets"
```

---

### Task 3: `ig-ui-kit`: theme, fonts, icons, metrics, palette

**Files:**
- Create: `crates/ig-ui-kit/build.rs`, `crates/ig-ui-kit/src/fonts.rs`, `crates/ig-ui-kit/src/icons.rs`, `crates/ig-ui-kit/src/metrics.rs`, `crates/ig-ui-kit/src/palette.rs`, `crates/ig-ui-kit/src/theme/mod.rs`, `crates/ig-ui-kit/src/lib.rs`

**Review focus:** `palette.rs` vs upstream `DefaultColorGenerator`/`GraphColorManagerImpl`; `metrics.rs` constants vs their cited `JBUI` sources; `theme` installs into gpui-component's theme without leaking globals.

- [ ] **Step 1: The logic modules.** `build.rs` embeds `assets/icons/**/*.svg` into the binary.

```bash
git checkout origin/wip/app-shell-prototype -- crates/ig-ui-kit/build.rs crates/ig-ui-kit/src/fonts.rs crates/ig-ui-kit/src/icons.rs crates/ig-ui-kit/src/metrics.rs crates/ig-ui-kit/src/palette.rs crates/ig-ui-kit/src/theme/mod.rs
```

`crates/ig-ui-kit/build.rs`:

```rust
//! Embeds `assets/icons/**/*.svg` (written by `cargo xtask import-ui`) into the binary: generates
//! `$OUT_DIR/icons.rs` with a sorted `(asset path, bytes)` table read by `crate::icons`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, base, out);
        } else if path.extension().is_some_and(|e| e == "svg") {
            let rel = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((format!("icons/{rel}"), path));
        }
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let icons = manifest.join("../../assets/icons");
    println!("cargo::rerun-if-changed={}", icons.display());
    let mut files = Vec::new();
    collect(&icons, &icons, &mut files);
    files.sort();
    let mut code = String::from("pub(crate) static ICONS: &[(&str, &[u8])] = &[\n");
    for (name, path) in &files {
        println!("cargo::rerun-if-changed={}", path.display());
        writeln!(
            code,
            "    ({name:?}, include_bytes!({:?})),",
            path.display().to_string()
        )
        .unwrap();
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("icons.rs");
    std::fs::write(out, code).unwrap();
}
```

`crates/ig-ui-kit/src/fonts.rs`:

```rust
//! Inter (UI) and JetBrains Mono (console), embedded in the binary.
//!
//! Copied unchanged by `cargo xtask import-ui` from upstream's
//! `platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts/`; both are SIL OFL 1.1 (see
//! `NOTICE` and `assets/fonts/*-OFL.txt`).

use std::borrow::Cow;

use gpui::App;

/// UI font family (IntelliJ's New UI default on all platforms).
pub const UI_FAMILY: &str = "Inter";
/// Console font family (IntelliJ's default editor font).
pub const MONO_FAMILY: &str = "JetBrains Mono";

static FONTS: &[&[u8]] = &[
    include_bytes!("../../../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf"),
];

/// Registers the embedded fonts with GPUI's text system.
pub fn load(cx: &App) -> gpui::Result<()> {
    cx.text_system()
        .add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect())
}
```

`crates/ig-ui-kit/src/icons.rs`:

```rust
//! IntelliJ New UI (`expui`) icons, embedded in the binary, and the GPUI asset source serving them.
//!
//! The SVGs are written by `cargo xtask import-ui` from the `icons.list` entries (upstream
//! `platform/icons/src/expui/**` and friends), each with its `_dark` variant where upstream has one.
//! They are drawn with `img()`, which keeps their colors (GPUI's `svg()` would tint them).

use std::borrow::Cow;

use gpui::{AssetSource, ImageSource, Img, SharedString, Styled, img, px};

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

/// Serves `icons/<name>.svg` from the embedded table.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(lookup(path).map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::new_static(name))
            .collect())
    }
}

fn lookup(path: &str) -> Option<&'static [u8]> {
    ICONS
        .binary_search_by(|(name, _)| (*name).cmp(path))
        .ok()
        .map(|i| ICONS[i].1)
}

/// Whether an icon exists (`name` without `icons/` and `.svg`, e.g. `vcs/branch`).
pub fn exists(name: &str) -> bool {
    lookup(&format!("icons/{name}.svg")).is_some()
}

/// The asset path of `name` for the theme: the `_dark` variant in dark themes when there is one.
pub fn path(name: &str, dark: bool) -> SharedString {
    if dark {
        let dark_path = format!("icons/{name}_dark.svg");
        if lookup(&dark_path).is_some() {
            return dark_path.into();
        }
    }
    format!("icons/{name}.svg").into()
}

/// An icon element of `size` logical pixels.
pub fn icon(name: &str, dark: bool, size: f32) -> Img {
    img(ImageSource::from(path(name, dark)))
        .size(px(size))
        .flex_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_for_binary_search() {
        assert!(ICONS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn dark_variant_falls_back_to_light() {
        assert_eq!(path("no/such", true), "icons/no/such.svg");
        if let Some((name, _)) = ICONS.iter().find(|(n, _)| n.ends_with("_dark.svg")) {
            let base = name
                .trim_start_matches("icons/")
                .trim_end_matches("_dark.svg");
            assert_eq!(path(base, true), *name);
            assert_eq!(path(base, false), format!("icons/{base}.svg"));
        }
    }
}
```

`crates/ig-ui-kit/src/metrics.rs`:

```rust
//! New UI metrics, in logical pixels (GPUI `px`, which equal IntelliJ's unscaled `JBUI` pixels).
//!
//! Hand-copied from the pinned upstream sources; each constant names its source. Values that
//! upstream reads from a theme key fall back to the default given here when neither expUI theme
//! sets the key (checked at the pinned commit).

/// `JBUI.CurrentTheme.VersionControl.Log.defaultRowHeight()` (`VersionControl.Log.Commit.rowHeight`,
/// not set by the expUI themes).
pub const LOG_MIN_ROW_HEIGHT: f32 = 26.0;
/// `JBUI.CurrentTheme.VersionControl.Log.defaultVerticalPadding()`.
pub const LOG_VERTICAL_PADDING: f32 = 7.0;

/// The Log row height: `GraphCommitCellRenderer.calculateHeight()` in the New UI,
/// `max(fontHeight + verticalPadding, rowHeight)` (the label painter is never taller here).
pub fn log_row_height(font_line_height: f32) -> f32 {
    (font_line_height + LOG_VERTICAL_PADDING)
        .ceil()
        .max(LOG_MIN_ROW_HEIGHT)
}

/// `PaintParameters.ROW_HEIGHT`: the row height the graph metrics below are defined for.
pub const GRAPH_BASE_ROW_HEIGHT: f32 = 22.0;
/// `PaintParameters.WIDTH_NODE`.
pub const GRAPH_WIDTH_NODE: f32 = 16.0;
/// `PaintParameters.CIRCLE_RADIUS`.
pub const GRAPH_CIRCLE_RADIUS: f32 = 4.0;
/// `PaintParameters.THICK_LINE`.
pub const GRAPH_THICK_LINE: f32 = 1.5;
/// `PaintParameters.SELECT_THICK_LINE`.
pub const GRAPH_SELECT_THICK_LINE: f32 = 2.5;
/// `PaintParameters.GRAPH_TEXT_GAP`.
pub const GRAPH_TEXT_GAP: f32 = 2.0;
/// `GraphCommitCellUtil.MAX_GRAPH_WIDTH`.
pub const GRAPH_MAX_WIDTH_ELEMENTS: f32 = 6.0;

/// `PaintParameters.scaleWithRowHeight`.
pub fn scale_with_row_height(value: f32, row_height: f32) -> f32 {
    value * row_height / GRAPH_BASE_ROW_HEIGHT
}

/// `JBUI.CurrentTheme.ToolWindow.defaultHeaderHeight()` (`ToolWindow.Header.height`).
pub const TOOL_WINDOW_HEADER_HEIGHT: f32 = 41.0;
/// `JBUI.CurrentTheme.ToolWindow.headerLabelLeftRightInsets()`: `insets(0, 12, 0, 16)`.
pub const TOOL_WINDOW_HEADER_LABEL_LEFT: f32 = 12.0;
pub const TOOL_WINDOW_HEADER_LABEL_RIGHT: f32 = 16.0;
/// `JBUI.CurrentTheme.ToolWindow.headerToolbarLeftRightInsets()`: `insets(0, 12, 0, 8)`.
pub const TOOL_WINDOW_HEADER_TOOLBAR_LEFT: f32 = 12.0;
pub const TOOL_WINDOW_HEADER_TOOLBAR_RIGHT: f32 = 8.0;
/// `JBUI.CurrentTheme.ToolWindow.headerTabLeftRightInsets()` in the New UI: `insets(0, 12)`.
pub const TOOL_WINDOW_HEADER_TAB_PADDING: f32 = 12.0;
/// `JBUI.CurrentTheme.ToolWindow.headerTabUnderlineArc()` (`ToolWindow.HeaderTab.underlineArc`).
pub const TOOL_WINDOW_HEADER_TAB_ARC: f32 = 4.0;
/// `JBUI.CurrentTheme.ToolWindow.underlineHeight()`: `DefaultTabs.UNDERLINE_HEIGHT` (2).
pub const TOOL_WINDOW_TAB_UNDERLINE_HEIGHT: f32 = 2.0;

/// `JBUI.CurrentTheme.Toolbar.defaultStripeToolbarButtonSize()`: 40 x 40.
pub const STRIPE_BUTTON_SIZE: f32 = 40.0;
/// `JBUI.CurrentTheme.Toolbar.defaultStripeToolbarButtonIconSize()`.
pub const STRIPE_BUTTON_ICON_SIZE: f32 = 20.0;
/// The stripe button's selection/hover rounded rectangle: `SquareStripeButtonLook.paintBackground`
/// removes `JBUI.CurrentTheme.Toolbar.stripeToolbarButtonIconPadding` (default `insets(5)`) from
/// the 40x40 button, leaving 30x30.
pub const STRIPE_BUTTON_HIGHLIGHT_SIZE: f32 = 30.0;
/// `IdeaActionButtonLook.getButtonArc()`: `DarculaUIUtil.BUTTON_ARC` (`Button.arc`, 8 in the
/// expUI themes). Java arcs are diameters, so the corner radius is half.
pub const STRIPE_BUTTON_HIGHLIGHT_ARC: f32 = 8.0;
/// Stripe width: one button plus 1px separator (`ToolWindowToolbar`'s border).
pub const STRIPE_WIDTH: f32 = STRIPE_BUTTON_SIZE + 1.0;

/// Main toolbar (window header) height: `CustomHeader.kt` `HEADER_HEIGHT_NORMAL`.
pub const MAIN_TOOLBAR_HEIGHT: f32 = 40.0;

/// Status bar height. Upstream derives it from the label font plus widget insets; not ported
/// (IdeaGit estimate: 30; to be checked against the `docs/ui-reference` IntelliJ screenshots).
pub const STATUS_BAR_HEIGHT: f32 = 30.0;
/// Horizontal widget padding: `JBUI.CurrentTheme.StatusBar.hoverInsets()` (`insets(3, 4, 1, 4)`) plus
/// 2px gap (IdeaGit estimate).
pub const STATUS_BAR_WIDGET_PADDING: f32 = 6.0;

/// `LabelPainter` paddings (`JBValue`s, scaled with the UI): see `log` label painting.
pub mod label {
    pub const TOP_TEXT_PADDING: f32 = 1.0;
    pub const BOTTOM_TEXT_PADDING: f32 = 2.0;
    pub const RIGHT_PADDING: f32 = 4.0;
    pub const LEFT_PADDING: f32 = 4.0;
    pub const COMPACT_MIDDLE_PADDING: f32 = 6.0;
    pub const MIDDLE_PADDING: f32 = 12.0;
    pub const ICON_TEXT_PADDING: f32 = 1.0;
    pub const ICON_ADDITIONAL_PADDING: f32 = 3.0;
    pub const LABEL_ARC: f32 = 6.0;
    /// `LabelPainter.MAX_LENGTH`.
    pub const MAX_LENGTH: usize = 22;
    /// `LabelIcon.SIZE`.
    pub const ICON_SIZE_UNITS: f32 = 6.25;
    /// `GraphCommitCellRenderer.DISPLAYED_MESSAGE_PART`.
    pub const DISPLAYED_MESSAGE_PART: f32 = 80.0;
}

/// Default UI font size in the New UI (`UISettings` default font size 13).
pub const UI_FONT_SIZE: f32 = 13.0;
/// Editor/console font size (`EditorFontsConstants` default 13).
pub const MONO_FONT_SIZE: f32 = 13.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_height_never_below_the_new_ui_minimum() {
        assert_eq!(log_row_height(12.0), 26.0);
        assert_eq!(log_row_height(16.0), 26.0);
        assert_eq!(log_row_height(20.5), 28.0);
    }

    #[test]
    fn graph_metrics_scale_with_row_height() {
        assert_eq!(scale_with_row_height(GRAPH_WIDTH_NODE, 22.0), 16.0);
        assert_eq!(scale_with_row_height(GRAPH_WIDTH_NODE, 33.0), 24.0);
    }
}
```

`crates/ig-ui-kit/src/palette.rs`:

```rust
//! Graph branch colors: maps an `ig-log` color id onto a color.
//!
//! Port of `platform/vcs-log/impl/src/com/intellij/vcs/log/graph/DefaultColorGenerator.kt`, with the
//! `java.awt.Color.RGBtoHSB` / `HSBtoRGB` conversions it relies on.
//!
//! The color id is what `ig_log::data::GraphColorManagerImpl` computes (a Java string hash of the
//! branch name, or [`DEFAULT_COLOR_ID`]). Saturation and brightness come from the theme keys
//! `VersionControl.Log.Graph.saturation` / `.brightness` (upstream defaults 0.4 / 0.65).
//!
//! Deviations: no cache (`colorMap`): the computation is a few float operations and the UI calls it
//! for visible rows only. Colors are returned as `0xRRGGBB`; the default color is resolved by the
//! caller ([`GraphPalette::default_color`]) because it depends on the theme (`JBColor.BLACK` is
//! black in light themes and the label foreground in dark ones).

/// `GraphColorManagerImpl.DEFAULT_COLOR`.
pub const DEFAULT_COLOR_ID: i32 = 0;

/// Branch color generator (`DefaultColorGenerator`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphPalette {
    /// `VersionControl.Log.Graph.saturation`, clamped to `0..=1`.
    pub saturation: f32,
    /// `VersionControl.Log.Graph.brightness`, clamped to `0..=1`.
    pub brightness: f32,
    /// The color of [`DEFAULT_COLOR_ID`] (`JBColor.BLACK`), `0xRRGGBB`.
    pub default_color: u32,
}

impl GraphPalette {
    pub fn new(saturation: f32, brightness: f32, default_color: u32) -> GraphPalette {
        GraphPalette {
            saturation: saturation.clamp(0.0, 1.0),
            brightness: brightness.clamp(0.0, 1.0),
            default_color,
        }
    }

    /// `getColor(colorId)` as `0xRRGGBB`.
    pub fn color(&self, color_id: i32) -> u32 {
        if color_id == DEFAULT_COLOR_ID {
            return self.default_color;
        }
        // `calcColor`: Java int arithmetic wraps on overflow
        let r = color_id.wrapping_mul(200).wrapping_add(30);
        let g = color_id.wrapping_mul(130).wrapping_add(50);
        let b = color_id.wrapping_mul(90).wrapping_add(100);
        let [hue, _, _] = rgb_to_hsb(range_fix(r), range_fix(g), range_fix(b));
        hsb_to_rgb(hue, self.saturation, self.brightness) & 0x00FF_FFFF
    }

    /// The color of [`DEFAULT_COLOR_ID`].
    pub fn default_color(&self) -> u32 {
        self.default_color
    }
}

/// `rangeFix`: `abs(n % 100) + 70` (Java `%` keeps the dividend's sign).
fn range_fix(n: i32) -> i32 {
    (n % 100).abs() + 70
}

/// `java.awt.Color.RGBtoHSB`.
pub fn rgb_to_hsb(r: i32, g: i32, b: i32) -> [f32; 3] {
    let cmax = r.max(g).max(b);
    let cmin = r.min(g).min(b);
    let brightness = cmax as f32 / 255.0;
    let saturation = if cmax != 0 {
        (cmax - cmin) as f32 / cmax as f32
    } else {
        0.0
    };
    let hue = if saturation == 0.0 {
        0.0
    } else {
        let span = (cmax - cmin) as f32;
        let redc = (cmax - r) as f32 / span;
        let greenc = (cmax - g) as f32 / span;
        let bluec = (cmax - b) as f32 / span;
        let mut hue = if r == cmax {
            bluec - greenc
        } else if g == cmax {
            2.0 + redc - bluec
        } else {
            4.0 + greenc - redc
        };
        hue /= 6.0;
        if hue < 0.0 {
            hue += 1.0;
        }
        hue
    };
    [hue, saturation, brightness]
}

/// `java.awt.Color.HSBtoRGB`: `0xFFRRGGBB`.
pub fn hsb_to_rgb(hue: f32, saturation: f32, brightness: f32) -> u32 {
    let to_byte = |v: f32| (v * 255.0 + 0.5) as u32;
    let (r, g, b) = if saturation == 0.0 {
        let v = to_byte(brightness);
        (v, v, v)
    } else {
        let h = (hue - hue.floor()) * 6.0;
        let f = h - h.floor();
        let p = brightness * (1.0 - saturation);
        let q = brightness * (1.0 - saturation * f);
        let t = brightness * (1.0 - saturation * (1.0 - f));
        match h as i32 {
            0 => (to_byte(brightness), to_byte(t), to_byte(p)),
            1 => (to_byte(q), to_byte(brightness), to_byte(p)),
            2 => (to_byte(p), to_byte(brightness), to_byte(t)),
            3 => (to_byte(p), to_byte(q), to_byte(brightness)),
            4 => (to_byte(t), to_byte(p), to_byte(brightness)),
            5 => (to_byte(brightness), to_byte(p), to_byte(q)),
            _ => (0, 0, 0),
        }
    };
    0xFF00_0000 | (r << 16) | (g << 8) | b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_hsb_round_trip_matches_awt() {
        // Expected values worked out by hand from the JDK algorithm (not run against a JVM).
        assert_eq!(rgb_to_hsb(255, 0, 0), [0.0, 1.0, 1.0]);
        assert_eq!(hsb_to_rgb(0.0, 1.0, 1.0), 0xFFFF0000);
        assert_eq!(hsb_to_rgb(0.5, 0.5, 0.5), 0xFF408080);
        assert_eq!(hsb_to_rgb(0.0, 0.0, 0.65), 0xFFA6A6A6);
        let [h, s, b] = rgb_to_hsb(100, 120, 140);
        assert!((h - 0.583_333_3).abs() < 1e-6); // (4 + 0.5 - 1) / 6
        assert!((s - 0.285_714_3).abs() < 1e-6);
        assert!((b - 0.549_019_6).abs() < 1e-6);
    }

    #[test]
    fn range_fix_keeps_java_sign_semantics() {
        assert_eq!(range_fix(30), 100);
        assert_eq!(range_fix(-130), 100);
        assert_eq!(range_fix(i32::MIN), 118);
    }

    #[test]
    fn default_color_id_uses_theme_color() {
        let palette = GraphPalette::new(0.4, 0.65, 0x123456);
        assert_eq!(palette.color(DEFAULT_COLOR_ID), 0x123456);
    }

    #[test]
    fn colors_use_hue_of_the_id_and_theme_saturation_brightness() {
        let palette = GraphPalette::new(0.4, 0.65, 0);
        // colorId 1: r=230 -> 100, g=180 -> 150, b=190 -> 160; hue of (100,150,160) = 0.52777...
        let [h, _, _] = rgb_to_hsb(100, 150, 160);
        assert_eq!(palette.color(1), hsb_to_rgb(h, 0.4, 0.65) & 0xFFFFFF);
        // every generated color has the requested brightness (max channel = 0.65 * 255)
        for id in [
            1,
            -1,
            7,
            1_234_567,
            i32::MAX,
            i32::MIN + 1,
            "master".len() as i32,
        ] {
            let c = palette.color(id);
            let max = (c >> 16 & 0xFF).max(c >> 8 & 0xFF).max(c & 0xFF);
            assert_eq!(max, 166, "id {id} -> {c:06x}");
        }
    }

    #[test]
    fn saturation_and_brightness_are_clamped() {
        let palette = GraphPalette::new(2.0, -1.0, 0);
        assert_eq!((palette.saturation, palette.brightness), (1.0, 0.0));
        assert_eq!(palette.color(5), 0);
    }
}
```

`crates/ig-ui-kit/src/theme/mod.rs`:

```rust
//! IntelliJ New UI theme: generated tokens for the light and dark expUI themes, the active theme
//! as a GPUI global, and syncing it with the OS appearance.
//!
//! [`tokens`] is generated by `cargo xtask import-ui` from
//! `platform/platform-resources/src/themes/expUI/expUI_{light,dark}.theme.json` (parent themes
//! resolved) and their editor color schemes; it holds only the keys IdeaGit uses.
//!
//! The gpui-component theme is kept in step ([`IjTheme::apply`]) so its widgets (inputs, buttons,
//! popups) use IntelliJ's colors too.

pub mod tokens;

use gpui::{App, Global, Hsla, Window, WindowAppearance, rgba};

use crate::palette::GraphPalette;
pub use tokens::{DARK, LIGHT, ThemeTokens};

/// A `0xRRGGBBAA` token as a GPUI color.
pub fn color(token: u32) -> Hsla {
    rgba(token).into()
}

/// The active theme (a GPUI global).
#[derive(Clone, Copy)]
pub struct IjTheme {
    pub dark: bool,
    pub tokens: &'static ThemeTokens,
}

impl Global for IjTheme {}

/// Set by [`IjTheme::pin`].
struct PinnedTheme;

impl Global for PinnedTheme {}

impl IjTheme {
    pub fn light() -> IjTheme {
        IjTheme {
            dark: false,
            tokens: &LIGHT,
        }
    }

    pub fn dark() -> IjTheme {
        IjTheme {
            dark: true,
            tokens: &DARK,
        }
    }

    pub fn for_appearance(appearance: WindowAppearance) -> IjTheme {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => IjTheme::dark(),
            WindowAppearance::Light | WindowAppearance::VibrantLight => IjTheme::light(),
        }
    }

    /// The graph palette (`DefaultColorGenerator`); `JBColor.BLACK` is black in light themes and
    /// the label foreground in dark ones.
    pub fn graph_palette(&self) -> GraphPalette {
        let default_color = if self.dark {
            self.tokens.foreground >> 8
        } else {
            0x000000
        };
        GraphPalette::new(
            self.tokens.graph_saturation,
            self.tokens.graph_brightness,
            default_color,
        )
    }

    /// Makes `self` the active theme and restyles gpui-component's widgets.
    pub fn apply(self, cx: &mut App) {
        cx.set_global(self);
        let t = self.tokens;
        let mode = if self.dark {
            gpui_component::ThemeMode::Dark
        } else {
            gpui_component::ThemeMode::Light
        };
        gpui_component::Theme::change(mode, None, cx);
        let theme = gpui_component::Theme::global_mut(cx);
        theme.font_family = crate::fonts::UI_FAMILY.into();
        theme.font_size = gpui::px(crate::metrics::UI_FONT_SIZE);
        theme.mono_font_family = crate::fonts::MONO_FAMILY.into();
        theme.mono_font_size = gpui::px(crate::metrics::MONO_FONT_SIZE);
        let c = &mut theme.colors;
        c.background = color(t.panel_background);
        c.foreground = color(t.foreground);
        c.muted_foreground = color(t.secondary_foreground);
        c.border = color(t.border);
        c.input = color(t.text_field_border);
        c.ring = color(t.focus_color);
        c.primary = color(t.button_default_background);
        c.primary_foreground = color(t.button_default_foreground);
        c.secondary = color(t.button_background);
        c.secondary_foreground = color(t.button_foreground);
        c.accent = color(t.list_hover_background);
        c.accent_foreground = color(t.foreground);
        c.popover = color(t.popup_background);
        c.popover_foreground = color(t.foreground);
        c.list_active = color(t.list_selection_background);
        c.link = color(t.link);
        c.scrollbar_thumb = color(t.scrollbar_thumb);
        c.scrollbar_thumb_hover = color(t.scrollbar_thumb_hover);
    }

    /// Pins the theme: [`sync_with_window`](Self::sync_with_window) stops following the OS
    /// (screenshots render both themes regardless of the appearance).
    pub fn pin(self, cx: &mut App) {
        self.apply(cx);
        cx.set_global(PinnedTheme);
    }

    /// Follows the window's (OS) appearance; returns whether the theme changed.
    pub fn sync_with_window(window: &Window, cx: &mut App) -> bool {
        if cx.has_global::<PinnedTheme>() {
            return false;
        }
        let wanted = IjTheme::for_appearance(window.appearance());
        let changed = cx
            .try_global::<IjTheme>()
            .is_none_or(|current| current.dark != wanted.dark);
        if changed {
            wanted.apply(cx);
        }
        changed
    }
}

/// `cx.ij()`: the active theme (light if none was applied yet).
pub trait ActiveIjTheme {
    fn ij(&self) -> IjTheme;
}

impl ActiveIjTheme for App {
    fn ij(&self) -> IjTheme {
        self.try_global::<IjTheme>()
            .copied()
            .unwrap_or_else(IjTheme::light)
    }
}
```

- [ ] **Step 2: `crates/ig-ui-kit/src/lib.rs`**, without the widgets yet. Write exactly:

```rust
//! IntelliJ New UI look for IdeaGit: generated theme tokens, `expui` icons, the Inter and
//! JetBrains Mono fonts, New UI metrics, the graph palette, and IntelliJ-styled widgets (tool
//! window stripes and headers, status bar, banners and balloons, table colors).
//!
//! This crate knows nothing about Git: the app crate maps Git concepts onto these pieces. Every
//! module names the upstream sources it ports; see also `reference/upstream-map.toml`.

pub mod fonts;
pub mod icons;
pub mod metrics;
pub mod palette;
pub mod theme;

pub use theme::{ActiveIjTheme, IjTheme, color};

/// Registers fonts and installs the theme for `appearance`. Call once at startup, after
/// `gpui_component::init`.
pub fn init(appearance: gpui::WindowAppearance, cx: &mut gpui::App) -> gpui::Result<()> {
    fonts::load(cx)?;
    IjTheme::for_appearance(appearance).apply(cx);
    Ok(())
}
```

- [ ] **Step 3: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code origin/wip/app-shell-prototype -- crates/ig-ui-kit/build.rs crates/ig-ui-kit/src/fonts.rs crates/ig-ui-kit/src/icons.rs crates/ig-ui-kit/src/metrics.rs crates/ig-ui-kit/src/palette.rs crates/ig-ui-kit/src/theme/mod.rs
```

Expected: no output. The intermediate files written above differ from the branch until their final task.

- [ ] **Step 4: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 420 passed, 1 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(ig-ui-kit): theme tokens, fonts, icons, New UI metrics, graph palette"
```

---

### Task 4: `ig-ui-kit`: widgets

**Files:**
- Create: `crates/ig-ui-kit/src/notification.rs`, `crates/ig-ui-kit/src/status_bar.rs`, `crates/ig-ui-kit/src/stripe.rs`, `crates/ig-ui-kit/src/table.rs`, `crates/ig-ui-kit/src/tool_window.rs`, `crates/ig-ui-kit/src/lib.rs`

**Review focus:** `IjDockRenderer` against gpui-component 0.5.2's dock API; stripe and header layout vs `ToolWindowPane`/`ToolWindowHeader` New UI metrics.

- [ ] **Step 1: The widgets and the final `lib.rs`.**

```bash
git checkout origin/wip/app-shell-prototype -- crates/ig-ui-kit/src/notification.rs crates/ig-ui-kit/src/status_bar.rs crates/ig-ui-kit/src/stripe.rs crates/ig-ui-kit/src/table.rs crates/ig-ui-kit/src/tool_window.rs crates/ig-ui-kit/src/lib.rs
```

`crates/ig-ui-kit/src/notification.rs`:

```rust
//! Banners (inline notices at the top of a panel) and notification balloons.
//!
//! Look ported from `platform/platform-api/src/com/intellij/ui/InlineBanner.kt` /
//! `EditorNotificationPanel` (`Banner.*` colors: tinted background, 1px border, icon, text,
//! actions) and `platform/platform-impl/src/com/intellij/notification/impl/NotificationsManagerImpl.java`
//! balloons (`Notification.*` colors, title in bold, content, actions as links).
//!
//! Deviations: no balloon animation or timeout, no "Don't show again", no event log.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons::icon;
use crate::metrics::UI_FONT_SIZE;
use crate::theme::{ActiveIjTheme, color};

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Severity of a banner or balloon (`NotificationType` / `EditorNotificationPanel.Status`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    fn icon(self) -> &'static str {
        match self {
            Severity::Info => "general/information",
            Severity::Warning => "general/warning",
            Severity::Error => "general/error",
        }
    }
}

/// A link-style action (`ActionLink`).
pub struct Action {
    label: SharedString,
    handler: Handler,
}

impl Action {
    pub fn new(
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Action {
        Action {
            label: label.into(),
            handler: Rc::new(handler),
        }
    }
}

fn render_actions(id: &ElementId, actions: Vec<Action>, cx: &App) -> Vec<AnyElement> {
    let link = color(cx.ij().tokens.link);
    actions
        .into_iter()
        .enumerate()
        .map(|(ix, action)| {
            let handler = action.handler;
            div()
                .id((SharedString::from(format!("{id}-action")), ix))
                .text_color(link)
                .cursor_pointer()
                .hover(|d| d.underline())
                .child(action.label)
                .on_click(move |e, w, cx| handler(e, w, cx))
                .into_any_element()
        })
        .collect()
}

/// An inline banner (`InlineBanner`).
#[derive(IntoElement)]
pub struct Banner {
    id: ElementId,
    severity: Severity,
    text: SharedString,
    actions: Vec<Action>,
}

impl Banner {
    pub fn new(
        id: impl Into<ElementId>,
        severity: Severity,
        text: impl Into<SharedString>,
    ) -> Banner {
        Banner {
            id: id.into(),
            severity,
            text: text.into(),
            actions: Vec::new(),
        }
    }

    pub fn action(mut self, action: Action) -> Banner {
        self.actions.push(action);
        self
    }
}

impl RenderOnce for Banner {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.ij();
        let t = theme.tokens;
        let (bg, border) = match self.severity {
            Severity::Info => (t.banner_info_background, t.banner_info_border),
            Severity::Warning => (t.banner_warning_background, t.banner_warning_border),
            Severity::Error => (t.banner_error_background, t.banner_error_border),
        };
        let actions = render_actions(&self.id, self.actions, cx);
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(color(border))
            .bg(color(bg))
            .text_size(px(UI_FONT_SIZE))
            .text_color(color(t.foreground))
            .child(icon(self.severity.icon(), theme.dark, 16.0))
            .child(div().flex_1().child(self.text))
            .children(actions)
    }
}

/// A notification balloon.
#[derive(IntoElement)]
pub struct Balloon {
    id: ElementId,
    severity: Severity,
    title: SharedString,
    content: SharedString,
    actions: Vec<Action>,
}

impl Balloon {
    pub fn new(
        id: impl Into<ElementId>,
        severity: Severity,
        title: impl Into<SharedString>,
        content: impl Into<SharedString>,
    ) -> Balloon {
        Balloon {
            id: id.into(),
            severity,
            title: title.into(),
            content: content.into(),
            actions: Vec::new(),
        }
    }

    pub fn action(mut self, action: Action) -> Balloon {
        self.actions.push(action);
        self
    }
}

impl RenderOnce for Balloon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.ij();
        let t = theme.tokens;
        let actions = render_actions(&self.id, self.actions, cx);
        div()
            .id(self.id)
            .w(px(360.0))
            .flex()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(color(t.notification_border))
            .bg(color(t.notification_background))
            .text_color(color(t.notification_foreground))
            .text_size(px(UI_FONT_SIZE))
            .shadow_lg()
            .child(icon(self.severity.icon(), theme.dark, 16.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .flex_1()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.title),
                    )
                    .when(!self.content.is_empty(), |d| d.child(self.content))
                    .child(div().flex().gap(px(12.0)).children(actions)),
            )
    }
}
```

`crates/ig-ui-kit/src/status_bar.rs`:

```rust
//! The status bar at the bottom of a project window and its text widgets.
//!
//! Look ported from `platform/platform-impl/src/com/intellij/openapi/wm/impl/status/IdeStatusBarImpl.kt`
//! and `TextPanel.kt` with `JBUI.CurrentTheme.StatusBar` colors: a top border, widgets as plain
//! text with a hover background.
//!
//! Deviations: no breadcrumbs (no editor), no widget popups, no progress bar - background work is
//! shown as text ([`StatusWidget::progress`]).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px, svg,
};

use crate::metrics::{STATUS_BAR_HEIGHT, STATUS_BAR_WIDGET_PADDING, UI_FONT_SIZE};
use crate::theme::{ActiveIjTheme, color};

/// A text widget (`TextPanel`), optionally with an icon.
#[derive(IntoElement)]
pub struct StatusWidget {
    id: ElementId,
    text: SharedString,
    icon: Option<SharedString>,
    tooltip: Option<SharedString>,
}

impl StatusWidget {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> StatusWidget {
        StatusWidget {
            id: id.into(),
            text: text.into(),
            icon: None,
            tooltip: None,
        }
    }

    /// An icon name (see [`crate::icons`]) drawn before the text.
    pub fn icon(mut self, icon: impl Into<SharedString>) -> StatusWidget {
        self.icon = Some(icon.into());
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> StatusWidget {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Background work in progress, e.g. "Loading log…".
    pub fn progress(id: impl Into<ElementId>, text: impl Into<SharedString>) -> StatusWidget {
        StatusWidget::new(id, text).icon("general/refresh")
    }
}

impl RenderOnce for StatusWidget {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.ij();
        let t = theme.tokens;
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(4.0))
            .h(px(STATUS_BAR_HEIGHT - 6.0))
            .px(px(STATUS_BAR_WIDGET_PADDING))
            .rounded(px(4.0))
            .text_color(color(t.status_bar_widget_foreground))
            .hover(|d| d.bg(color(t.status_bar_widget_hover_background)))
            .when_some(self.icon, |d, icon| {
                d.child(
                    svg()
                        .path(crate::icons::path(&icon, theme.dark))
                        .size(px(16.0))
                        .text_color(color(t.status_bar_widget_foreground)),
                )
            })
            .child(self.text)
            .when_some(self.tooltip, |d, tooltip| {
                d.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
            })
    }
}

/// The status bar: `left` widgets (messages) and `right` widgets (branch, progress).
pub fn status_bar(left: Vec<AnyElement>, right: Vec<AnyElement>, cx: &App) -> impl IntoElement {
    let t = cx.ij().tokens;
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(STATUS_BAR_HEIGHT))
        .px(px(STATUS_BAR_WIDGET_PADDING))
        .border_t_1()
        .border_color(color(t.status_bar_border))
        .bg(color(t.status_bar_background))
        .text_size(px(UI_FONT_SIZE))
        .children(left)
        .child(div().flex_1())
        .children(right)
}
```

`crates/ig-ui-kit/src/stripe.rs`:

```rust
//! New UI tool-window stripes: the vertical bars of square tool-window buttons at the window edges.
//!
//! Look ported from `platform/platform-impl/src/com/intellij/toolWindow/ToolWindowToolbar.kt`,
//! `.../openapi/wm/impl/SquareStripeButton.kt` and `SquareStripeButtonLook.kt`: 40x40 buttons, a
//! 30x30 rounded highlight (hover, or the selected color while the tool window is open), 20px
//! icons, and a separator on the inner edge.
//!
//! Deviations: no drag and drop between stripes, no "more" button, no names under icons.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px, svg,
};

use crate::metrics::{
    STRIPE_BUTTON_HIGHLIGHT_ARC, STRIPE_BUTTON_HIGHLIGHT_SIZE, STRIPE_BUTTON_ICON_SIZE,
    STRIPE_BUTTON_SIZE, STRIPE_WIDTH,
};
use crate::theme::{ActiveIjTheme, color};

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Which window edge a stripe sits on; the separator is drawn on the inner edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StripeSide {
    Left,
    Right,
}

/// A square tool-window button (`SquareStripeButton`).
#[derive(IntoElement)]
pub struct StripeButton {
    id: ElementId,
    icon: SharedString,
    tooltip: SharedString,
    selected: bool,
    on_click: Option<ClickHandler>,
}

impl StripeButton {
    /// `icon` is an icon name (see [`crate::icons`]), e.g. `toolwindows/git`.
    pub fn new(id: impl Into<ElementId>, icon: impl Into<SharedString>) -> StripeButton {
        StripeButton {
            id: id.into(),
            icon: icon.into(),
            tooltip: SharedString::default(),
            selected: false,
            on_click: None,
        }
    }

    /// The tool window is open (upstream `PUSHED` state).
    pub fn selected(mut self, selected: bool) -> StripeButton {
        self.selected = selected;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> StripeButton {
        self.tooltip = tooltip.into();
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> StripeButton {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for StripeButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.ij();
        let t = theme.tokens;
        // The expui tool-window icons are single-color; draw them as a mask so the selected state
        // can use `ToolWindow.Button.selectedForeground`, as upstream swaps to a selected icon.
        let icon_color = if self.selected {
            t.stripe_button_selected_foreground
        } else {
            t.stripe_button_foreground
        };
        let on_click = self.on_click.clone();
        let tooltip = self.tooltip.clone();
        div()
            .id(self.id)
            .when(!tooltip.is_empty(), |d| {
                d.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
            })
            .size(px(STRIPE_BUTTON_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .size(px(STRIPE_BUTTON_HIGHLIGHT_SIZE))
                    .rounded(px(STRIPE_BUTTON_HIGHLIGHT_ARC / 2.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(self.selected, |d| {
                        d.bg(color(t.stripe_button_selected_background))
                    })
                    .when(!self.selected, |d| {
                        d.hover(|d| d.bg(color(t.stripe_button_hover_background)))
                    })
                    .child(
                        svg()
                            .path(crate::icons::path(&self.icon, theme.dark))
                            .size(px(STRIPE_BUTTON_ICON_SIZE))
                            .text_color(color(icon_color)),
                    ),
            )
            .when_some(on_click, |d, handler| {
                d.on_click(move |e, w, cx| handler(e, w, cx))
            })
    }
}

/// A stripe: buttons from the top, separator on the inner edge.
pub fn stripe(side: StripeSide, buttons: Vec<StripeButton>, cx: &App) -> impl IntoElement {
    let t = cx.ij().tokens;
    let d = div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(STRIPE_WIDTH))
        .h_full()
        .bg(color(t.stripe_background))
        .border_color(color(t.stripe_separator))
        .children(buttons);
    match side {
        StripeSide::Left => d.border_r_1(),
        StripeSide::Right => d.border_l_1(),
    }
}
```

`crates/ig-ui-kit/src/table.rs`:

```rust
//! Table styling shared by IntelliJ-like tables (the Log): colors for the row states and the
//! header cell.
//!
//! Colors follow `platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/VcsLogGraphTable.java`
//! in the New UI: the table background is the tool-window background (`getTableBackground`),
//! selection uses `VersionControl.Log.Commit.selectionBackground` (the list selection colors,
//! focused or not), hover uses `VersionControl.Log.Commit.hoveredBackground`. No grid lines
//! (`setShowGrid(false)`, `setIntercellSpacing(emptySize)`).

use gpui::{App, Hsla, IntoElement, ParentElement, SharedString, Styled, div, px};

use crate::metrics::UI_FONT_SIZE;
use crate::theme::{ActiveIjTheme, color};

/// The colors of a row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowColors {
    pub background: Hsla,
    pub foreground: Hsla,
}

/// Table colors of the active theme.
#[derive(Clone, Copy, Debug)]
pub struct TableColors {
    pub background: Hsla,
    pub foreground: Hsla,
    pub secondary_foreground: Hsla,
    pub selection: Hsla,
    pub selection_foreground: Hsla,
    pub selection_inactive: Hsla,
    pub selection_inactive_foreground: Hsla,
    pub hover: Hsla,
    pub header_separator: Hsla,
}

impl TableColors {
    pub fn get(cx: &App) -> TableColors {
        let t = cx.ij().tokens;
        TableColors {
            background: color(t.tool_window_background),
            foreground: color(t.table_foreground),
            secondary_foreground: color(t.secondary_foreground),
            selection: color(t.list_selection_background),
            selection_foreground: color(t.table_selection_foreground),
            selection_inactive: color(t.list_selection_inactive_background),
            selection_inactive_foreground: color(t.table_selection_inactive_foreground),
            hover: color(t.log_hovered_background),
            header_separator: color(t.table_header_separator),
        }
    }

    /// `VcsLogGraphTable.getStyle` base colors for a row.
    pub fn row(&self, selected: bool, focused: bool, hovered: bool) -> RowColors {
        match (selected, focused) {
            (true, true) => RowColors {
                background: self.selection,
                foreground: self.selection_foreground,
            },
            (true, false) => RowColors {
                background: self.selection_inactive,
                foreground: self.selection_inactive_foreground,
            },
            _ => RowColors {
                background: if hovered { self.hover } else { self.background },
                foreground: self.foreground,
            },
        }
    }
}

/// A header cell (`VcsLogGraphTable`'s header: secondary text, no grid).
pub fn header_cell(
    text: impl Into<SharedString>,
    width: Option<f32>,
    cx: &App,
) -> impl IntoElement {
    let colors = TableColors::get(cx);
    let cell = div()
        .flex()
        .items_center()
        .h_full()
        .px(px(8.0))
        .overflow_hidden()
        .text_size(px(UI_FONT_SIZE))
        .text_color(colors.secondary_foreground)
        .child(text.into());
    match width {
        Some(w) => cell.w(px(w)).flex_none(),
        None => cell.flex_1(),
    }
}
```

`crates/ig-ui-kit/src/tool_window.rs`:

```rust
//! IntelliJ New UI tool windows on top of gpui-component's dock (the unstyled `gpui-base` dock with
//! an IntelliJ renderer).
//!
//! Look ported from `platform/platform-impl/src/com/intellij/toolWindow/ToolWindowHeader.kt`,
//! `.../openapi/wm/impl/content/ContentTabLabel.java` / `TabContentLayout.java` and the
//! `JBUI.CurrentTheme.ToolWindow` metrics: a 41px header with the tool-window title, the content
//! tabs (12px side insets, a 2px underline under the selected tab) and a right-aligned toolbar
//! with the Hide button; a 1px border and a resize edge between the dock and the editor area.
//!
//! Deviations: one title per renderer (IdeaGit has one bottom tool window so far); no tab drag and
//! drop between docks, no gear/options menu, no split/float view modes.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Div, InteractiveElement, IntoElement, ParentElement,
    SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div, px, svg,
};
use gpui_component::dock::{
    DockAreaRenderer, DockContext, DockPlacement, PanelHandle, TabGroupContext, TabGroupRenderer,
    TileContext, TilesRenderer,
};

use crate::metrics::{
    TOOL_WINDOW_HEADER_HEIGHT, TOOL_WINDOW_HEADER_LABEL_LEFT, TOOL_WINDOW_HEADER_TAB_ARC,
    TOOL_WINDOW_HEADER_TAB_PADDING, TOOL_WINDOW_HEADER_TOOLBAR_RIGHT,
    TOOL_WINDOW_TAB_UNDERLINE_HEIGHT, UI_FONT_SIZE,
};
use crate::theme::{ActiveIjTheme, color};

type HideHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// Renders the dock area as IntelliJ tool windows.
pub struct IjDockRenderer {
    title: SharedString,
    on_hide: HideHandler,
    headerless: Vec<&'static str>,
}

impl IjDockRenderer {
    /// `title` is the tool window name shown in the header (e.g. "Git"); `on_hide` runs for the
    /// header's Hide button (upstream `HideToolWindowAction`).
    pub fn new(
        title: impl Into<SharedString>,
        on_hide: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Rc<IjDockRenderer> {
        IjDockRenderer::with_headerless(title, on_hide, Vec::new())
    }

    /// Like [`new`](Self::new); groups whose panels are all named in `headerless`
    /// (`Panel::panel_name`) get no header (e.g. the editor area).
    pub fn with_headerless(
        title: impl Into<SharedString>,
        on_hide: impl Fn(&mut Window, &mut App) + 'static,
        headerless: Vec<&'static str>,
    ) -> Rc<IjDockRenderer> {
        Rc::new(IjDockRenderer {
            title: title.into(),
            on_hide: Rc::new(on_hide),
            headerless,
        })
    }
}

/// Drag payload of the dock's resize edge.
#[derive(Clone)]
struct ResizeDock;

impl gpui::Render for ResizeDock {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
    }
}

impl DockAreaRenderer for IjDockRenderer {
    fn frame(&self, _: &mut Window, cx: &mut App) -> Stateful<Div> {
        div()
            .id("ij-dock-area")
            .bg(color(cx.ij().tokens.panel_background))
    }

    fn render_dock(
        &self,
        dock: &DockContext,
        content: AnyElement,
        _: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let t = cx.ij().tokens;
        let placement = dock.placement();
        let resize = dock.clone();
        let edge = div()
            .id(match placement {
                DockPlacement::Left => "dock-edge-left",
                DockPlacement::Right => "dock-edge-right",
                DockPlacement::Bottom => "dock-edge-bottom",
                DockPlacement::Center => "dock-edge-center",
            })
            .absolute()
            .map(|d| match placement {
                DockPlacement::Bottom => d.top_0().left_0().w_full().h(px(4.0)).cursor_row_resize(),
                DockPlacement::Left => d.top_0().right_0().h_full().w(px(4.0)).cursor_col_resize(),
                _ => d.top_0().left_0().h_full().w(px(4.0)).cursor_col_resize(),
            })
            .on_drag(ResizeDock, |_, _, _, cx| cx.new(|_| ResizeDock))
            .on_drag_move::<ResizeDock>(move |e, window, cx| {
                resize.resize_to(e.event.position, window, cx)
            });
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .border_color(color(t.border))
            .map(|d| match placement {
                DockPlacement::Bottom => d.border_t_1(),
                DockPlacement::Left => d.border_r_1(),
                DockPlacement::Right => d.border_l_1(),
                DockPlacement::Center => d,
            })
            .child(content)
            .child(edge)
            .into_any_element()
    }

    fn tiles_renderer(&self) -> Rc<dyn TilesRenderer> {
        Rc::new(NoTiles)
    }

    fn tab_group_renderer(&self) -> Rc<dyn TabGroupRenderer> {
        Rc::new(IjToolWindowHeader {
            title: self.title.clone(),
            on_hide: self.on_hide.clone(),
            headerless: self.headerless.clone(),
        })
    }
}

/// IdeaGit has no floating tiles; the renderer gpui-component's dock requires draws nothing.
struct NoTiles;

impl TilesRenderer for NoTiles {
    fn render_drag_bar(&self, _: &TileContext, _: &mut Window, _: &mut App) -> AnyElement {
        div().into_any_element()
    }
}

/// The tool-window header with content tabs (`ToolWindowHeader` + `TabContentLayout`).
struct IjToolWindowHeader {
    title: SharedString,
    on_hide: HideHandler,
    headerless: Vec<&'static str>,
}

fn tab_name(panel: &std::sync::Arc<dyn gpui_component::dock::BasePanelView>, cx: &App) -> String {
    PanelHandle::of(panel)
        .and_then(|p| p.tab_name(cx))
        .map(|s| s.to_string())
        .unwrap_or_else(|| panel.panel_name(cx).to_string())
}

impl TabGroupRenderer for IjToolWindowHeader {
    fn frame(&self, _: &TabGroupContext, _: &mut Window, cx: &mut App) -> Stateful<Div> {
        div()
            .id("ij-tool-window")
            .bg(color(cx.ij().tokens.tool_window_background))
    }

    fn render_tab_bar(&self, group: &TabGroupContext, _: &mut Window, cx: &mut App) -> AnyElement {
        if !group.panels().is_empty()
            && group
                .panels()
                .iter()
                .all(|p| self.headerless.contains(&p.panel_name(cx)))
        {
            return div().into_any_element();
        }
        let theme = cx.ij();
        let t = theme.tokens;
        let active = group.active_ix();
        let tabs: Vec<AnyElement> = group
            .panels()
            .iter()
            .enumerate()
            .filter(|(_, p)| p.visible(cx))
            .map(|(ix, panel)| {
                let selected = ix == active;
                let group = group.clone();
                div()
                    .id(("ij-tool-window-tab", ix))
                    .relative()
                    .h_full()
                    .flex()
                    .items_center()
                    .px(px(TOOL_WINDOW_HEADER_TAB_PADDING))
                    .rounded(px(TOOL_WINDOW_HEADER_TAB_ARC))
                    .when(!selected, |d| {
                        d.hover(|d| d.bg(color(t.tool_window_header_tab_hover)))
                    })
                    .child(tab_name(panel, cx))
                    .when(selected, |d| {
                        d.child(
                            div()
                                .absolute()
                                .bottom_0()
                                .left_0()
                                .right_0()
                                .h(px(TOOL_WINDOW_TAB_UNDERLINE_HEIGHT))
                                .bg(color(t.tool_window_tab_underline)),
                        )
                    })
                    .on_click(move |_, window, cx| group.select_tab(ix, window, cx))
                    .into_any_element()
            })
            .collect();
        let on_hide = self.on_hide.clone();
        div()
            .id("ij-tool-window-header")
            .flex()
            .flex_none()
            .items_center()
            .h(px(TOOL_WINDOW_HEADER_HEIGHT))
            .pl(px(TOOL_WINDOW_HEADER_LABEL_LEFT))
            .pr(px(TOOL_WINDOW_HEADER_TOOLBAR_RIGHT))
            .bg(color(t.tool_window_header_background))
            .text_size(px(UI_FONT_SIZE))
            .text_color(color(t.foreground))
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .pr(px(4.0))
                    .child(format!("{}:", self.title)),
            )
            .child(div().flex().h_full().children(tabs))
            .child(div().flex_1())
            .child(
                div()
                    .id("ij-tool-window-hide")
                    .size(px(24.0))
                    .rounded(px(4.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|d| d.bg(color(t.tool_window_header_tab_hover)))
                    .child(
                        svg()
                            .path(crate::icons::path("general/hide", theme.dark))
                            .size(px(16.0))
                            .text_color(color(t.secondary_foreground)),
                    )
                    .on_click(move |_, window, cx| on_hide(window, cx)),
            )
            .into_any_element()
    }
}
```

`crates/ig-ui-kit/src/lib.rs`:

```rust
//! IntelliJ New UI look for IdeaGit: generated theme tokens, `expui` icons, the Inter and
//! JetBrains Mono fonts, New UI metrics, the graph palette, and IntelliJ-styled widgets (tool
//! window stripes and headers, status bar, banners and balloons, table colors).
//!
//! This crate knows nothing about Git: the app crate maps Git concepts onto these pieces. Every
//! module names the upstream sources it ports; see also `reference/upstream-map.toml`.

pub mod fonts;
pub mod icons;
pub mod metrics;
pub mod notification;
pub mod palette;
pub mod status_bar;
pub mod stripe;
pub mod table;
pub mod theme;
pub mod tool_window;

pub use theme::{ActiveIjTheme, IjTheme, color};

/// Registers fonts and installs the theme for `appearance`. Call once at startup, after
/// `gpui_component::init`.
pub fn init(appearance: gpui::WindowAppearance, cx: &mut gpui::App) -> gpui::Result<()> {
    fonts::load(cx)?;
    IjTheme::for_appearance(appearance).apply(cx);
    Ok(())
}
```

- [ ] **Step 2: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code origin/wip/app-shell-prototype -- crates/ig-ui-kit/src/notification.rs crates/ig-ui-kit/src/status_bar.rs crates/ig-ui-kit/src/stripe.rs crates/ig-ui-kit/src/table.rs crates/ig-ui-kit/src/tool_window.rs crates/ig-ui-kit/src/lib.rs
```

Expected: no output. The intermediate files written above differ from the branch until their final task.

- [ ] **Step 3: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 420 passed, 1 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(ig-ui-kit): tool window stripes and header, status bar, notifications, table colors"
```

---

### Task 5: `ig-log` and `ig-git` additions for the UI

**Files:**
- Modify: `crates/ig-log/src/print/elements.rs`, `crates/ig-log/tests/print_element_generator_extra.rs`, `crates/ig-git/src/console.rs`, `crates/ig-git/src/runner.rs`, `crates/ig-git/tests/runner_integration.rs`

**Review focus:** `is_terminal` keeps upstream equality (ignored, as in `EdgePrintElementImpl.equals`); `silent_command` default is a no-op so existing sinks are unchanged.

Two small additions. The graph painter leaves a gap before a terminal arrow, as `SimpleGraphCellPainter` does, so it needs to know which edges are `TerminalEdgePrintElement`s. The console debug toggle needs silent commands reported.

- [ ] **Step 1: The changed files.** Each test addition is marked as an IdeaGit test.

```bash
git checkout origin/wip/app-shell-prototype -- crates/ig-log/src/print/elements.rs crates/ig-log/tests/print_element_generator_extra.rs crates/ig-git/src/console.rs crates/ig-git/src/runner.rs crates/ig-git/tests/runner_integration.rs
```

`crates/ig-log/src/print/elements.rs`:

```rust
//! Concrete print elements produced by [`PrintElementGeneratorImpl`](super::print_element_generator_impl::PrintElementGeneratorImpl).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/elements/`
//! (`PrintElementBase.kt`, `SimplePrintElementImpl.kt`, `EdgePrintElementImpl.kt`,
//! `TerminalEdgePrintElement.java`).
//!
//! Deviation: upstream elements ask the presentation manager for `colorId` / `isSelected` on every
//! access; here both are computed once, when the generator creates the element. Elements are
//! created afresh on every `get_print_elements` call, so they reflect the selection at that time.

use crate::api::elements::{GraphEdge, GraphEdgeType, GraphElement, GraphNode};
use crate::api::printer::PrintElementPresentationManager;
use crate::print_element::{
    EdgePrintElement, EdgePrintElementType, LineStyle, NodePrintElement, PrintElement,
};
use crate::visible_graph::VcsLogVisibleGraphIndex;

/// A node print element (`SimplePrintElementImpl`).
#[derive(Clone, Debug)]
pub struct SimplePrintElementImpl {
    row_index: VcsLogVisibleGraphIndex,
    position_in_current_row: i32,
    graph_node: GraphNode,
    color_id: i32,
    is_selected: bool,
}

impl SimplePrintElementImpl {
    /// Creates the element; color and selection come from `presentation_manager`.
    pub fn new(
        row_index: VcsLogVisibleGraphIndex,
        position_in_current_row: i32,
        graph_node: GraphNode,
        presentation_manager: &dyn PrintElementPresentationManager,
    ) -> SimplePrintElementImpl {
        let mut element = SimplePrintElementImpl {
            row_index,
            position_in_current_row,
            graph_node,
            color_id: presentation_manager.get_color_id(&GraphElement::Node(graph_node)),
            is_selected: false,
        };
        element.is_selected = presentation_manager.is_selected(
            &crate::api::printer::GraphPrintElement::Node(element.clone()),
        );
        element
    }

    pub fn graph_node(&self) -> GraphNode {
        self.graph_node
    }
}

impl PrintElement for SimplePrintElementImpl {
    fn row_index(&self) -> VcsLogVisibleGraphIndex {
        self.row_index
    }
    fn position_in_current_row(&self) -> i32 {
        self.position_in_current_row
    }
    fn color_id(&self) -> i32 {
        self.color_id
    }
    fn is_selected(&self) -> bool {
        self.is_selected
    }
}

impl NodePrintElement for SimplePrintElementImpl {}

/// Upstream equality: same row and position (graph element, color and selection are ignored).
impl PartialEq for SimplePrintElementImpl {
    fn eq(&self, other: &Self) -> bool {
        self.position_in_current_row == other.position_in_current_row
            && self.row_index == other.row_index
    }
}

/// An edge print element (`EdgePrintElementImpl`); a `TerminalEdgePrintElement` is one created by
/// [`EdgePrintElementImpl::terminal`].
#[derive(Clone, Debug)]
pub struct EdgePrintElementImpl {
    row_index: VcsLogVisibleGraphIndex,
    position_in_current_row: i32,
    position_in_other_row: i32,
    edge_print_type: EdgePrintElementType,
    graph_edge: GraphEdge,
    has_arrow: bool,
    line_style: LineStyle,
    color_id: i32,
    is_selected: bool,
    is_terminal: bool,
}

impl EdgePrintElementImpl {
    /// Creates the element; color and selection come from `presentation_manager`.
    pub fn new(
        row_index: VcsLogVisibleGraphIndex,
        position_in_current_row: i32,
        position_in_other_row: i32,
        edge_print_type: EdgePrintElementType,
        graph_edge: GraphEdge,
        has_arrow: bool,
        presentation_manager: &dyn PrintElementPresentationManager,
    ) -> EdgePrintElementImpl {
        let mut element = EdgePrintElementImpl {
            row_index,
            position_in_current_row,
            position_in_other_row,
            edge_print_type,
            graph_edge,
            has_arrow,
            line_style: Self::convert_to_line_style(graph_edge.edge_type),
            color_id: presentation_manager.get_color_id(&GraphElement::Edge(graph_edge)),
            is_selected: false,
            is_terminal: false,
        };
        element.is_selected = presentation_manager.is_selected(
            &crate::api::printer::GraphPrintElement::Edge(element.clone()),
        );
        element
    }

    /// `TerminalEdgePrintElement`: an arrow that ends in this row (other position = own position).
    pub fn terminal(
        row_index: VcsLogVisibleGraphIndex,
        position: i32,
        edge_print_type: EdgePrintElementType,
        graph_edge: GraphEdge,
        presentation_manager: &dyn PrintElementPresentationManager,
    ) -> EdgePrintElementImpl {
        EdgePrintElementImpl {
            is_terminal: true,
            ..EdgePrintElementImpl::new(
                row_index,
                position,
                position,
                edge_print_type,
                graph_edge,
                true,
                presentation_manager,
            )
        }
    }

    /// Whether this is a `TerminalEdgePrintElement` (an arrow ending in this row). The painter
    /// (`SimpleGraphCellPainter`) leaves a gap before a terminal arrow; equality ignores it, as
    /// upstream's `EdgePrintElementImpl.equals` does.
    pub fn is_terminal(&self) -> bool {
        self.is_terminal
    }

    pub fn graph_edge(&self) -> GraphEdge {
        self.graph_edge
    }

    pub fn convert_to_line_style(edge_type: GraphEdgeType) -> LineStyle {
        match edge_type {
            GraphEdgeType::Usual | GraphEdgeType::NotLoadCommit => LineStyle::Solid,
            GraphEdgeType::Dotted
            | GraphEdgeType::DottedArrowUp
            | GraphEdgeType::DottedArrowDown => LineStyle::Dashed,
        }
    }
}

impl PrintElement for EdgePrintElementImpl {
    fn row_index(&self) -> VcsLogVisibleGraphIndex {
        self.row_index
    }
    fn position_in_current_row(&self) -> i32 {
        self.position_in_current_row
    }
    fn color_id(&self) -> i32 {
        self.color_id
    }
    fn is_selected(&self) -> bool {
        self.is_selected
    }
}

impl EdgePrintElement for EdgePrintElementImpl {
    fn position_in_other_row(&self) -> i32 {
        self.position_in_other_row
    }
    fn edge_print_type(&self) -> EdgePrintElementType {
        self.edge_print_type
    }
    fn line_style(&self) -> LineStyle {
        self.line_style
    }
    fn has_arrow(&self) -> bool {
        self.has_arrow
    }
}

/// Upstream equality: positions, row, type and arrow (graph edge, style, color and selection are ignored).
impl PartialEq for EdgePrintElementImpl {
    fn eq(&self, other: &Self) -> bool {
        self.position_in_current_row == other.position_in_current_row
            && self.position_in_other_row == other.position_in_other_row
            && self.row_index == other.row_index
            && self.edge_print_type == other.edge_print_type
            && self.has_arrow == other.has_arrow
    }
}
```

`crates/ig-log/tests/print_element_generator_extra.rs`:

```rust
//! IdeaGit tests for `recommended_width` and for the layout tie-break of branch heads.
//! Upstream has no tests for these; expected values follow from the upstream algorithms.

use std::collections::HashSet;

use ig_log::api::GraphLayout;
use ig_log::facade::permanent_graph_impl::PermanentGraphImpl;
use ig_log::graph_commit::GraphCommitImpl;
use ig_log::permanent_graph::{Options, PermanentGraph};
use ig_log::print::graph_color_getter::GraphColorGetterByNodeFactory;
use ig_log::visible_graph::VisibleGraph;

fn build(
    commits: &[GraphCommitImpl<String>],
    branches: &HashSet<String>,
) -> PermanentGraphImpl<String> {
    let factory = GraphColorGetterByNodeFactory::new(|_: &String, layout_index| layout_index);
    PermanentGraphImpl::new_instance(
        commits,
        &factory,
        |_: &String, _: &String| std::cmp::Ordering::Equal,
        branches,
    )
}

/// `c{n-1} -> ... -> c0`, `c0` has no parents.
fn linear_chain(n: usize) -> Vec<GraphCommitImpl<String>> {
    (0..n)
        .rev()
        .map(|i| {
            let parents = if i == 0 {
                vec![]
            } else {
                vec![format!("c{}", i - 1)]
            };
            GraphCommitImpl::create_commit(format!("c{i}"), parents, i as i64)
        })
        .collect()
}

#[test]
fn recommended_width_of_linear_history_is_one() {
    for n in [10, 36] {
        let graph = build(&linear_chain(n), &HashSet::new());
        let visible = graph
            .create_visible_graph(Options::DEFAULT, None, None)
            .unwrap();
        assert_eq!(visible.recommended_width(), 1, "n = {n}");
    }
}

#[test]
fn layout_is_independent_of_branch_input_order_when_heads_tie() {
    // Five unrelated single-commit branches; the comparator never separates them.
    let commits: Vec<_> = (0..5)
        .map(|i| GraphCommitImpl::create_commit(format!("h{i}"), vec![], 10 - i as i64))
        .collect();
    let branches: Vec<String> = vec!["h4".into(), "h2".into()];
    // Branch heads first in ascending node id (2, 4), then the remaining graph heads (0, 1, 3).
    let expected = vec![3, 4, 1, 5, 2];
    // Every `HashSet` has its own random hasher, so each run iterates the branches in another order.
    for round in 0..50 {
        let mut shuffled = branches.clone();
        if round % 2 == 1 {
            shuffled.reverse();
        }
        let set: HashSet<String> = shuffled.into_iter().collect();
        let graph = build(&commits, &set);
        let layout = graph.permanent_graph_layout_impl();
        let actual: Vec<i32> = (0..5).map(|i| layout.get_layout_index(i)).collect();
        assert_eq!(actual, expected, "round {round}");
    }
}

/// IdeaGit addition (`EdgePrintElementImpl::is_terminal`, needed by the Log painter): the cut ends
/// of a hidden long edge are terminal arrows; ordinary edges are not terminal.
#[test]
fn long_edge_cut_points_are_terminal_arrows() {
    use ig_log::GraphPrintElement;
    use ig_log::print_element::{EdgePrintElement, PrintElement};

    // `side` (newest) -> c0, next to the 80-commit chain c79 -> ... -> c0
    let mut commits = vec![GraphCommitImpl::create_commit(
        "side".to_string(),
        vec!["c0".to_string()],
        1000,
    )];
    commits.extend(linear_chain(80));
    let graph = build(&commits, &HashSet::new());
    let visible = graph
        .create_visible_graph(Options::DEFAULT, None, None)
        .unwrap();
    let edges: Vec<_> = (0..visible.visible_commit_count())
        .flat_map(|row| visible.get_row_info(row).print_elements())
        .filter_map(|e| match e {
            GraphPrintElement::Edge(edge) => Some(edge),
            GraphPrintElement::Node(_) => None,
        })
        .collect();
    let terminal: Vec<_> = edges.iter().filter(|e| e.is_terminal()).collect();
    assert!(!terminal.is_empty(), "the long edge is cut");
    for edge in &terminal {
        assert!(edge.has_arrow());
        assert_eq!(edge.position_in_current_row(), edge.position_in_other_row());
    }
    assert!(edges.iter().any(|e| !e.is_terminal()));
}
```

`crates/ig-git/src/console.rs`:

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
///
/// Must not run write commands on the same [`crate::runner::Git`]: the per-project write lock is not re-entrant.
pub trait ConsoleSink: Send + Sync {
    fn event(&self, event: ConsoleEvent);

    /// A silent (read) command started. Upstream never shows these; IdeaGit's console has a debug
    /// toggle that does. Only the command line is reported, no output. Default: ignored.
    fn silent_command(&self, _working_dir: &std::path::Path, _command_line: &str) {}
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

`crates/ig-git/src/runner.rs`:

```rust
//! Port of the execution parts of `plugins/git4idea/backend/src/commands/GitImplBase.java`
//! (environment, write locking, console logging, output collection) and `GitCommandResult.java`.
//!
//! Commands run synchronously on the calling thread; callers (UI) run them on a background executor.
//!
//! Known gaps, scheduled with cancellation in the fetch/push plan: a grandchild process that keeps
//! a pipe open (ssh ControlPersist, backgrounded hooks) delays completion until it exits, and the
//! write lock cannot be interrupted while waiting.

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
    fn error_or_std_output(&self) -> Vec<String> {
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

    /// Upstream `getPresentableErrorOutput`: `error_or_std_output` without `@...@` banner
    /// lines and without `fatal:` / `error:` prefixes.
    pub fn presentable_error_output(&self) -> Vec<String> {
        self.error_or_std_output()
            .into_iter()
            .filter(|line| !is_git_error_header_line(line))
            .map(|line| cleanup_error_prefixes(&line))
            .collect()
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

/// Upstream `GitCommandResult.isGitErrorHeaderLine`: lines of ASCII-art banners (e.g. ssh warnings).
fn is_git_error_header_line(line: &str) -> bool {
    line.starts_with('@') && line.ends_with('@')
}

/// Upstream `GitUtil.cleanupErrorPrefixes`.
pub fn cleanup_error_prefixes(msg: &str) -> String {
    let mut msg = msg.trim();
    for prefix in ["fatal:", "error:"] {
        if let Some(rest) = msg.strip_prefix(prefix) {
            msg = rest.trim();
        }
    }
    msg.to_string()
}

/// Exit code as the JVM reports it: 128 + signal number for processes killed by a signal.
fn exit_code_of(status: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    status.code().unwrap_or(-1)
}

/// Maximum number of output lines buffered between the reader threads and the calling thread.
const OUTPUT_CHANNEL_BOUND: usize = 1024;

/// Receives each output line with its kind (upstream `GitLineHandlerListener.onLineAvailable`).
pub type LineListener<'a> = dyn FnMut(&str, OutputKind) + 'a;

/// Runs git commands for one project. Holds the per-project command lock (upstream
/// `GitVcs.getCommandLock`) that serializes write commands.
pub struct Git {
    executable: PathBuf,
    version: GitVersion,
    console: Arc<dyn ConsoleSink>,
    command_lock: Mutex<()>,
    next_id: AtomicU64,
    base_env: Vec<(String, String)>,
    base_env_removed: Vec<String>,
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
            base_env: Vec::new(),
            base_env_removed: Vec::new(),
        }
    }

    /// Environment applied to every handler created by [`Git::handler`] (on top of the inherited
    /// process environment). Used by tests to isolate git from the developer's configuration.
    pub fn with_base_environment(mut self, set: &[(&str, &str)], remove: &[&str]) -> Self {
        self.base_env = set
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self.base_env_removed = remove.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Runs `git version` with `executable` and returns a runner configured with the detected version.
    pub fn detect(executable: impl Into<PathBuf>, console: Arc<dyn ConsoleSink>) -> Result<Git> {
        let executable = executable.into();
        let probe = Git::new(executable.clone(), GitVersion::NULL, console.clone());
        let cwd = std::env::temp_dir();
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
        self.handler_with_config(working_dir, command, &[])
    }

    /// Creates a handler with extra `-c` config parameters (upstream
    /// `GitLineHandler(project, root, command, configParameters)`).
    pub fn handler_with_config(
        &self,
        working_dir: &Path,
        command: GitCommand,
        config_parameters: &[&str],
    ) -> GitHandler {
        let mut handler =
            GitHandler::with_config(working_dir, &self.executable, command, config_parameters);
        for (k, v) in &self.base_env {
            handler.add_custom_environment_variable(k, v);
        }
        for name in &self.base_env_removed {
            handler.remove_inherited_environment_variable(name);
        }
        handler
    }

    /// Upstream `GitImplBase.doRun`.
    pub fn run_command(&self, handler: GitHandler) -> GitCommandResult {
        self.run_with(handler, None, true)
    }

    /// Upstream `runCommandWithoutCollectingOutput`: every stdout and stderr line goes to
    /// `listener` as it arrives (upstream `GitLineHandlerListener.onLineAvailable`); only stderr is
    /// collected in the result.
    ///
    /// The listener runs on the calling thread, possibly while the per-project write lock is
    /// held. It must not run write commands on the same [`Git`]: the lock is not reentrant.
    pub fn run_command_without_collecting_output(
        &self,
        handler: GitHandler,
        listener: &mut dyn FnMut(&str, OutputKind),
    ) -> GitCommandResult {
        self.run_with(handler, Some(listener), false)
    }

    fn run_with(
        &self,
        mut handler: GitHandler,
        listener: Option<&mut LineListener<'_>>,
        collect_stdout: bool,
    ) -> GitCommandResult {
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
        self.execute(&handler, listener, collect_stdout)
    }

    fn execute(
        &self,
        handler: &GitHandler,
        mut listener: Option<&mut LineListener<'_>>,
        collect_stdout: bool,
    ) -> GitCommandResult {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let log_to_console = !handler.is_silent();

        let mut cmd = handler.build_command();
        let stdin = if handler.input().is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        };
        cmd.stdin(stdin)
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
        } else {
            self.console
                .silent_command(handler.working_dir(), &handler.printable_command_line());
        }

        // Upstream `GitHandlerInputProcessorUtil.writeLines`: the input is written, then stdin closed.
        let in_thread = match (child.stdin.take(), handler.input()) {
            (Some(mut stdin), Some(input)) => {
                let input = input.to_vec();
                let spawned = thread::Builder::new()
                    .name("ig-git-stdin-writer".into())
                    .spawn(move || {
                        use std::io::Write;
                        let _ = stdin.write_all(&input);
                    });
                match spawned {
                    Ok(handle) => Some(handle),
                    Err(e) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        if log_to_console {
                            self.console
                                .event(ConsoleEvent::Finished { id, exit_code: -1 });
                        }
                        return GitCommandResult {
                            start_failed: true,
                            exit_code: -1,
                            output: Vec::new(),
                            error_output: vec![format!("Failed to start Git process: {e}")],
                        };
                    }
                }
            }
            _ => None,
        };

        // Bounded for backpressure. The consumer is this (calling) thread and the readers only send,
        // so a full channel just pauses the readers; it cannot deadlock.
        let (tx, rx) = mpsc::sync_channel::<(OutputKind, String)>(OUTPUT_CHANNEL_BOUND);
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let out_thread = spawn_reader(stdout, OutputKind::Stdout, tx.clone());
        let err_thread = spawn_reader(stderr, OutputKind::Stderr, tx);

        let mut output = Vec::new();
        let mut error_output = Vec::new();
        let mut panic_payload: Option<Box<dyn std::any::Any + Send>> = None;
        for (kind, line) in rx {
            if log_to_console && should_log_line(handler, kind, &line) {
                self.console.event(ConsoleEvent::Output {
                    id,
                    kind,
                    line: line.clone(),
                });
            }
            // MSRV 1.85: no let-chains
            #[allow(clippy::collapsible_if)]
            if panic_payload.is_none() {
                if let Some(listener) = listener.as_mut() {
                    if let Err(payload) =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            listener(&line, kind)
                        }))
                    {
                        // Stop calling the listener but keep draining, so the readers finish and
                        // the child is reaped; the panic is resumed after cleanup.
                        panic_payload = Some(payload);
                    }
                }
            }
            match kind {
                OutputKind::Stdout => {
                    if collect_stdout {
                        output.push(line);
                    }
                }
                OutputKind::Stderr => {
                    if !looks_like_progress(&line) {
                        error_output.push(line);
                    }
                }
            }
        }
        let _ = out_thread.join();
        let _ = err_thread.join();
        if let Some(in_thread) = in_thread {
            let _ = in_thread.join();
        }
        let exit_code = match child.wait() {
            Ok(status) => exit_code_of(status),
            Err(_) => -1,
        };
        if log_to_console {
            self.console.event(ConsoleEvent::Finished { id, exit_code });
        }
        if let Some(payload) = panic_payload {
            std::panic::resume_unwind(payload);
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
    tx: mpsc::SyncSender<(OutputKind, String)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut splitter = LineSplitter::default();
        let mut buf = [0u8; 8192];
        let mut emit = |line: String| {
            let _ = tx.send((kind, line));
        };
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => splitter.feed(&buf[..n], &mut emit),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        splitter.finish(&mut emit);
    })
}

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
        assert_eq!(split(&[b"a\r", b"b"]), ["a", "b"]);
        assert_eq!(split(&[b"a\r"]), ["a"]);
        assert_eq!(split(&[b"a\r\rb"]), ["a", "", "b"]);
    }

    #[test]
    fn presentable_errors_are_cleaned_up() {
        let r = GitCommandResult {
            start_failed: false,
            exit_code: 128,
            output: vec![],
            error_output: vec![
                "@@@@@@@@@@@@".into(),
                "@    WARNING: UNPROTECTED PRIVATE KEY FILE!    @".into(),
                "fatal: not a git repository".into(),
                "  error: bad thing ".into(),
            ],
        };
        assert_eq!(
            r.presentable_error_output(),
            ["not a git repository", "bad thing"]
        );
        assert_eq!(cleanup_error_prefixes("fatal: error: x"), "x");
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
        assert!(looks_like_progress(
            "Receiving objects: 100% (57489/57489), 50.03 MiB | 2.83 MiB/s, done."
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

`crates/ig-git/tests/runner_integration.rs`:

```rust
//! Runner integration tests against the real `git` executable.

mod common;

use std::sync::Arc;

use common::{init_repo, isolate, runner, runner_with};
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
    isolate(&mut status);
    status.add_parameters(["--porcelain"]);
    assert!(g.run_command(status).success());
    assert_eq!(console.events().len(), before, "status is silent");

    let mut checkout = g.handler(tmp.path(), GitCommand::CHECKOUT);
    isolate(&mut checkout);
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
    isolate(&mut h);
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

/// IdeaGit addition: silent commands are reported to `ConsoleSink::silent_command` (command line
/// only), never as console events.
#[test]
fn silent_commands_reach_only_the_silent_hook() {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Sink {
        events: Mutex<Vec<ConsoleEvent>>,
        silent: Mutex<Vec<(PathBuf, String)>>,
    }
    impl ig_git::console::ConsoleSink for Sink {
        fn event(&self, event: ConsoleEvent) {
            self.events.lock().unwrap().push(event);
        }
        fn silent_command(&self, working_dir: &Path, command_line: &str) {
            self.silent
                .lock()
                .unwrap()
                .push((working_dir.to_path_buf(), command_line.to_string()));
        }
    }

    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    let sink = Arc::new(Sink::default());
    let g = runner_with(sink.clone());
    let before = sink.events.lock().unwrap().len();
    let mut status = g.handler(tmp.path(), GitCommand::STATUS);
    isolate(&mut status);
    status.add_parameters(["--porcelain"]);
    assert!(g.run_command(status).success());
    assert_eq!(sink.events.lock().unwrap().len(), before);
    let silent = sink.silent.lock().unwrap();
    let (dir, line) = silent.last().expect("status reported as silent");
    assert_eq!(dir, tmp.path());
    assert!(
        line.contains("status") && line.contains("--porcelain"),
        "{line}"
    );
}
```

- [ ] **Step 2: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code origin/wip/app-shell-prototype -- crates/ig-log/src/print/elements.rs crates/ig-log/tests/print_element_generator_extra.rs crates/ig-git/src/console.rs crates/ig-git/src/runner.rs crates/ig-git/tests/runner_integration.rs
```

Expected: no output. The intermediate files written above differ from the branch until their final task.

- [ ] **Step 3: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 422 passed, 1 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: EdgePrintElementImpl::is_terminal and ConsoleSink::silent_command"
```

---

### Task 6: `ig-app`: strings, paths, settings, logging, actions, console

**Files:**
- Create: `crates/ig-app/src/strings.rs`, `crates/ig-app/src/paths.rs`, `crates/ig-app/src/settings.rs`, `crates/ig-app/src/logging.rs`, `crates/ig-app/src/actions.rs`, `crates/ig-app/src/console.rs`, `crates/ig-app/src/lib.rs`

**Review focus:** `console.rs` format vs upstream `GitConsole`/`VcsConsoleLine`; action ids and keymap loading vs `KeymapImpl`; settings persistence of recent projects.

- [ ] **Step 1: The modules.**

```bash
git checkout origin/wip/app-shell-prototype -- crates/ig-app/src/strings.rs crates/ig-app/src/paths.rs crates/ig-app/src/settings.rs crates/ig-app/src/logging.rs crates/ig-app/src/actions.rs crates/ig-app/src/console.rs
```

`crates/ig-app/src/strings.rs`:

```rust
//! UI strings from the upstream bundles (`assets/strings/*.properties`, written by
//! `cargo xtask import-ui`), with the `java.text.MessageFormat` subset they use.
//!
//! Supported patterns: `{0}`, `{0,number}` and `{0,choice,limit#text|...}` (`ChoiceFormat`, whose
//! texts may contain `{0,number}`); `''` is a literal quote. Properties escapes `\uXXXX`, `\n`,
//! `\t`, `\\` and line continuations are decoded.

use std::collections::HashMap;
use std::sync::OnceLock;

static BUNDLES: &[(&str, &str)] = &[
    (
        "VcsLogBundle",
        include_str!("../../../assets/strings/VcsLogBundle.properties"),
    ),
    (
        "GitBundle",
        include_str!("../../../assets/strings/GitBundle.properties"),
    ),
    (
        "VcsBundle",
        include_str!("../../../assets/strings/VcsBundle.properties"),
    ),
    (
        "UtilBundle",
        include_str!("../../../assets/strings/UtilBundle.properties"),
    ),
];

fn bundles() -> &'static HashMap<(&'static str, String), String> {
    static MAP: OnceLock<HashMap<(&'static str, String), String>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = HashMap::new();
        for (bundle, text) in BUNDLES {
            for (key, value) in parse_properties(text) {
                map.insert((*bundle, key), value);
            }
        }
        map
    })
}

/// The raw text of `key` in `bundle`; the key itself if missing (as upstream's `!key!` fallback
/// would hide less). `import-ui` fails on missing keys, so this only happens for typos.
pub fn text(bundle: &str, key: &str) -> String {
    bundles()
        .iter()
        .find(|((b, k), _)| *b == bundle && k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| key.to_string())
}

/// `Bundle.message(key, args...)`.
pub fn message(bundle: &str, key: &str, args: &[&str]) -> String {
    format_message(&text(bundle, key), args)
}

/// Parses `.properties` text: `key=value` / `key: value` lines, `#`/`!` comments.
pub fn parse_properties(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut logical = String::new();
    for line in text.lines() {
        let line = line.trim_start();
        if logical.is_empty() && (line.is_empty() || line.starts_with('#') || line.starts_with('!'))
        {
            continue;
        }
        let continued = line.ends_with('\\') && !line.ends_with("\\\\");
        logical.push_str(if continued {
            &line[..line.len() - 1]
        } else {
            line
        });
        if continued {
            continue;
        }
        let entry = std::mem::take(&mut logical);
        let split = entry
            .char_indices()
            .find(|&(i, c)| (c == '=' || c == ':') && !entry[..i].ends_with('\\'))
            .map(|(i, _)| i)
            .unwrap_or(entry.len());
        let key = unescape(entry[..split].trim_end());
        let value = unescape(entry.get(split + 1..).unwrap_or("").trim_start());
        out.push((key, value));
    }
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// `MessageFormat.format` for the supported subset.
pub fn format_message(pattern: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(i) = rest.find(['{', '\'']) {
        out.push_str(&rest[..i]);
        if rest[i..].starts_with("''") {
            out.push('\'');
            rest = &rest[i + 2..];
            continue;
        }
        if rest[i..].starts_with('\'') {
            out.push('\'');
            rest = &rest[i + 1..];
            continue;
        }
        // find the matching close brace (choice formats nest `{0,number}`)
        let mut depth = 0;
        let mut end = None;
        for (j, c) in rest[i..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i + j);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            out.push_str(&rest[i..]);
            return out;
        };
        out.push_str(&format_argument(&rest[i + 1..end], args));
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn format_argument(spec: &str, args: &[&str]) -> String {
    let mut parts = spec.splitn(3, ',');
    let index: usize = parts
        .next()
        .unwrap_or("")
        .trim()
        .parse()
        .unwrap_or(usize::MAX);
    let arg = args.get(index).copied().unwrap_or("");
    match (parts.next().map(str::trim), parts.next()) {
        (Some("choice"), Some(choices)) => {
            let n: f64 = arg.parse().unwrap_or(0.0);
            let mut chosen = None;
            for choice in choices.split('|') {
                let Some((limit, text)) = choice.split_once(['#', '<']) else {
                    continue;
                };
                let limit: f64 = limit.trim().parse().unwrap_or(f64::MAX);
                if chosen.is_none() || n >= limit {
                    chosen = Some(text);
                }
                if n < limit {
                    break;
                }
            }
            format_message(chosen.unwrap_or(""), args)
        }
        _ => arg.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_format_picks_the_last_reached_limit() {
        let pattern = "{0,choice, 0#Moments|1#A minute|2#{0,number} minutes|60#1 hour} ago";
        assert_eq!(format_message(pattern, &["0"]), "Moments ago");
        assert_eq!(format_message(pattern, &["1"]), "A minute ago");
        assert_eq!(format_message(pattern, &["42"]), "42 minutes ago");
        assert_eq!(format_message(pattern, &["61"]), "1 hour ago");
    }

    #[test]
    fn placeholders_and_quotes() {
        assert_eq!(
            format_message("Git {0} isn''t supported", &["1.0"]),
            "Git 1.0 isn't supported"
        );
    }

    #[test]
    fn properties_escapes_and_continuations() {
        let parsed = parse_properties("# c\na=b\\u00e9\nlong=x\\\n   y\nk\\=x : v\n");
        assert_eq!(
            parsed,
            vec![
                ("a".to_string(), "bé".to_string()),
                ("long".to_string(), "xy".to_string()),
                ("k=x".to_string(), "v".to_string()),
            ]
        );
    }
}
```

`crates/ig-app/src/paths.rs`:

```rust
//! Where IdeaGit keeps its files: settings in the OS config directory, `ideagit.log` in the OS log
//! directory (macOS `~/Library/Logs/IdeaGit`, Linux `$XDG_STATE_HOME/IdeaGit` or
//! `~/.local/state/IdeaGit`, Windows `%LOCALAPPDATA%\IdeaGit\logs`).
//!
//! `IDEAGIT_CONFIG_DIR` overrides the config directory (tests, screenshots).

use std::path::PathBuf;

const APP_DIR: &str = "IdeaGit";

pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("IDEAGIT_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR)
}

pub fn settings_file() -> PathBuf {
    config_dir().join("settings.toml")
}

pub fn log_dir() -> PathBuf {
    if cfg!(target_os = "macos")
        && let Some(home) = dirs::home_dir()
    {
        return home.join("Library/Logs").join(APP_DIR);
    }
    if cfg!(windows)
        && let Some(dir) = dirs::data_local_dir()
    {
        return dir.join(APP_DIR).join("logs");
    }
    dirs::state_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/state")))
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR)
}

pub fn log_file() -> PathBuf {
    log_dir().join("ideagit.log")
}
```

`crates/ig-app/src/settings.rs`:

```rust
//! Application settings persisted in `settings.toml`: recent projects and the git executable.
//!
//! Upstream keeps these in `recentProjects.xml` (`RecentProjectsManagerBase`) and `git.xml`
//! (`GitVcsApplicationSettings.myPathToGit`). IdeaGit uses one TOML file in the OS config
//! directory ([`crate::paths::config_dir`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Upstream `RecentProjectsManagerBase` keeps 50 entries (`ide.max.recent.projects`).
pub const MAX_RECENT_PROJECTS: usize = 50;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Git executable; `None` means `git` from `PATH` (upstream's auto-detected path).
    pub git_path: Option<PathBuf>,
    /// Most recent first.
    pub recent_projects: Vec<PathBuf>,
}

impl Settings {
    /// Reads `path`; a missing or unreadable file gives the defaults (logged).
    pub fn load(path: &Path) -> Settings {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                log::warn!("ignoring malformed {}: {e}", path.display());
                Settings::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                log::warn!("can't read {}: {e}", path.display());
                Settings::default()
            }
        }
    }

    /// Writes `path` atomically (temp file + rename).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }

    /// Moves `project` to the top of the recent list (upstream `addRecentPath`).
    pub fn add_recent(&mut self, project: &Path) {
        self.recent_projects.retain(|p| p != project);
        self.recent_projects.insert(0, project.to_path_buf());
        self.recent_projects.truncate(MAX_RECENT_PROJECTS);
    }

    pub fn remove_recent(&mut self, project: &Path) {
        self.recent_projects.retain(|p| p != project);
    }

    /// The git executable to run.
    pub fn git_executable(&self) -> PathBuf {
        self.git_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("git"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_projects_round_trip_most_recent_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let mut settings = Settings::load(&path);
        assert_eq!(settings, Settings::default());
        settings.add_recent(Path::new("/a"));
        settings.add_recent(Path::new("/b"));
        settings.add_recent(Path::new("/a"));
        settings.git_path = Some(PathBuf::from("/opt/git/bin/git"));
        settings.save(&path).unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(
            loaded.recent_projects,
            [PathBuf::from("/a"), PathBuf::from("/b")]
        );
        assert_eq!(loaded.git_executable(), PathBuf::from("/opt/git/bin/git"));
    }

    #[test]
    fn recent_list_is_bounded_and_malformed_files_are_ignored() {
        let mut settings = Settings::default();
        for i in 0..60 {
            settings.add_recent(Path::new(&format!("/p{i}")));
        }
        assert_eq!(settings.recent_projects.len(), MAX_RECENT_PROJECTS);
        assert_eq!(settings.recent_projects[0], PathBuf::from("/p59"));
        settings.remove_recent(Path::new("/p59"));
        assert_eq!(settings.recent_projects[0], PathBuf::from("/p58"));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "recent_projects = 3").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }
}
```

`crates/ig-app/src/logging.rs`:

```rust
//! `ideagit.log` in the OS log directory, and panic logging.
//!
//! A minimal `log` backend (no new dependency): info and above to the file, warnings and above
//! also to stderr. Panics anywhere are logged with their location before the default hook runs.

use std::io::Write;
use std::sync::Mutex;

struct FileLogger {
    file: Option<Mutex<std::fs::File>>,
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:5} {}: {}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
            record.level(),
            record.target(),
            record.args()
        );
        if let Some(file) = &self.file {
            let mut file = file.lock().unwrap_or_else(|e| e.into_inner());
            let _ = file.write_all(line.as_bytes());
        }
        if record.level() <= log::Level::Warn {
            eprint!("{line}");
        }
    }

    fn flush(&self) {}
}

/// Installs the logger and the panic hook. Safe to call once.
pub fn init() {
    let path = crate::paths::log_file();
    let file = path
        .parent()
        .and_then(|dir| std::fs::create_dir_all(dir).ok())
        .and_then(|_| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .ok()
        })
        .map(Mutex::new);
    let logger = Box::leak(Box::new(FileLogger { file }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        default_hook(info);
    }));
    log::info!(
        "IdeaGit {} starting; log at {}",
        env!("CARGO_PKG_VERSION"),
        path.display()
    );
}
```

`crates/ig-app/src/actions.rs`:

```rust
//! IdeaGit's actions, keyed by IntelliJ action ids, and their key bindings from the imported
//! IntelliJ keymaps (`assets/keymaps/{default,macos}.tsv`, written by `cargo xtask import-ui` from
//! `platform/platform-resources/src/keymaps/$default.xml` and `Mac OS X 10.5+.xml`).
//!
//! GPUI action names can't contain dots, so each action has a GPUI name and the IntelliJ id it
//! stands for ([`ACTION_IDS`]); bindings are looked up by IntelliJ id.
//!
//! Deviation: only the actions IdeaGit has are bound; keystrokes IntelliJ writes that GPUI can't
//! express are skipped (logged).

use gpui::{Action, App, KeyBinding};

gpui::actions!(
    ideagit,
    [
        /// `OpenFile`
        OpenFile,
        /// `CloseProject`
        CloseProject,
        /// `Exit`
        Exit,
        /// `About`
        About,
        /// `ActivateVersionControlToolWindow`
        ActivateVersionControlToolWindow,
        /// `HideActiveWindow`
        HideActiveWindow,
        /// `Vcs.Log.Refresh`
        RefreshLog,
        /// IdeaGit addition: show silent (read) commands in the Git console.
        ToggleShowReadCommands,
    ]
);

/// Table navigation in the Log (`EditorUp` & co. in the keymaps; Swing's `JTable` bindings).
pub mod table {
    gpui::actions!(
        git_log_table,
        [
            SelectUp,
            SelectDown,
            ExtendUp,
            ExtendDown,
            PageUp,
            PageDown,
            SelectFirst,
            SelectLast,
            SelectAll
        ]
    );
}

/// The key context of the Log table.
pub const LOG_TABLE_CONTEXT: &str = "GitLogTable";

/// IntelliJ action id -> (GPUI action, key context).
pub fn bindable(id: &str) -> Option<(Box<dyn Action>, Option<&'static str>)> {
    let global = |a: Box<dyn Action>| Some((a, None));
    let table = |a: Box<dyn Action>| Some((a, Some(LOG_TABLE_CONTEXT)));
    match id {
        "OpenFile" => global(Box::new(OpenFile)),
        "CloseProject" => global(Box::new(CloseProject)),
        "Exit" => global(Box::new(Exit)),
        "About" => global(Box::new(About)),
        "ActivateVersionControlToolWindow" => global(Box::new(ActivateVersionControlToolWindow)),
        "HideActiveWindow" => global(Box::new(HideActiveWindow)),
        "Vcs.Log.Refresh" => global(Box::new(RefreshLog)),
        "EditorUp" => table(Box::new(table::SelectUp)),
        "EditorDown" => table(Box::new(table::SelectDown)),
        "EditorUpWithSelection" => table(Box::new(table::ExtendUp)),
        "EditorDownWithSelection" => table(Box::new(table::ExtendDown)),
        "EditorPageUp" => table(Box::new(table::PageUp)),
        "EditorPageDown" => table(Box::new(table::PageDown)),
        "EditorTextStart" => table(Box::new(table::SelectFirst)),
        "EditorTextEnd" => table(Box::new(table::SelectLast)),
        "$SelectAll" => table(Box::new(table::SelectAll)),
        _ => None,
    }
}

/// The IntelliJ ids IdeaGit implements (for menus showing shortcuts).
pub const ACTION_IDS: &[&str] = &[
    "OpenFile",
    "CloseProject",
    "Exit",
    "About",
    "ActivateVersionControlToolWindow",
    "HideActiveWindow",
    "Vcs.Log.Refresh",
];

/// Converts an IntelliJ keystroke (`KeyStroke.getKeyStroke` syntax, e.g. `meta R`,
/// `control F5`, `shift PAGE_UP`) to GPUI's (`cmd-r`, `ctrl-f5`, `shift-pageup`).
pub fn convert_keystroke(stroke: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for token in stroke.split_whitespace() {
        let mapped = match token {
            "meta" => "cmd".to_string(),
            "control" | "ctrl" => "ctrl".to_string(),
            "alt" => "alt".to_string(),
            "shift" => "shift".to_string(),
            "pressed" | "released" | "typed" => continue,
            key => convert_key(key)?,
        };
        parts.push(mapped);
    }
    (!parts.is_empty()).then(|| parts.join("-"))
}

fn convert_key(key: &str) -> Option<String> {
    let named = match key {
        "UP" => "up",
        "DOWN" => "down",
        "LEFT" => "left",
        "RIGHT" => "right",
        "PAGE_UP" => "pageup",
        "PAGE_DOWN" => "pagedown",
        "HOME" => "home",
        "END" => "end",
        "ESCAPE" => "escape",
        "ENTER" => "enter",
        "TAB" => "tab",
        "SPACE" => "space",
        "BACK_SPACE" => "backspace",
        "DELETE" => "delete",
        "INSERT" => "insert",
        "COMMA" => ",",
        "PERIOD" => ".",
        "SLASH" => "/",
        "SEMICOLON" => ";",
        "EQUALS" => "=",
        "MINUS" => "-",
        "OPEN_BRACKET" => "[",
        "CLOSE_BRACKET" => "]",
        "BACK_SLASH" => "\\",
        "BACK_QUOTE" => "`",
        "QUOTE" => "'",
        _ => "",
    };
    if !named.is_empty() {
        return Some(named.to_string());
    }
    let is_function_key = key.len() > 1
        && key.starts_with('F')
        && key[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n));
    if is_function_key {
        return Some(key.to_ascii_lowercase());
    }
    if key.chars().count() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Some(key.to_ascii_lowercase());
    }
    None
}

/// A parsed keymap line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub action_id: String,
    /// GPUI keystroke sequence (`cmd-k cmd-s` for two-stroke shortcuts).
    pub keystrokes: String,
}

/// Parses an imported `.tsv` keymap.
pub fn parse_keymap(text: &str) -> Vec<Shortcut> {
    let mut out = Vec::new();
    for line in text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
    {
        let mut fields = line.split('\t');
        let Some(id) = fields.next() else { continue };
        let strokes: Option<Vec<String>> = fields.map(convert_keystroke).collect();
        match strokes {
            Some(strokes) if !strokes.is_empty() => out.push(Shortcut {
                action_id: id.to_string(),
                keystrokes: strokes.join(" "),
            }),
            _ => log::debug!("skipping keymap line {line:?}"),
        }
    }
    out
}

static DEFAULT_KEYMAP: &str = include_str!("../../../assets/keymaps/default.tsv");
static MACOS_KEYMAP: &str = include_str!("../../../assets/keymaps/macos.tsv");

/// The keymap of this OS (IntelliJ's default per OS: `Mac OS X 10.5+` on macOS, `$default`
/// elsewhere).
pub fn os_keymap() -> Vec<Shortcut> {
    parse_keymap(if cfg!(target_os = "macos") {
        MACOS_KEYMAP
    } else {
        DEFAULT_KEYMAP
    })
}

/// The first shortcut of `action_id` in this OS keymap, for menus and tooltips.
pub fn shortcut_for(action_id: &str) -> Option<String> {
    os_keymap()
        .into_iter()
        .find(|s| s.action_id == action_id)
        .map(|s| s.keystrokes)
}

/// Binds every implemented action's shortcuts.
pub fn bind_keys(cx: &mut App) {
    let bindings: Vec<KeyBinding> = os_keymap()
        .into_iter()
        .filter_map(|s| {
            let (action, context) = bindable(&s.action_id)?;
            let context = context.and_then(|c| {
                gpui::KeyBindingContextPredicate::parse(c)
                    .ok()
                    .map(std::rc::Rc::new)
            });
            KeyBinding::load(
                &s.keystrokes,
                action,
                context,
                false,
                None,
                cx.keyboard_mapper().as_ref(),
            )
            .map_err(|e| log::warn!("binding {} to {}: {e}", s.action_id, s.keystrokes))
            .ok()
        })
        .collect();
    cx.bind_keys(bindings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keystrokes_convert_to_gpui_syntax() {
        assert_eq!(convert_keystroke("meta R").as_deref(), Some("cmd-r"));
        assert_eq!(convert_keystroke("control F5").as_deref(), Some("ctrl-f5"));
        assert_eq!(
            convert_keystroke("shift PAGE_UP").as_deref(),
            Some("shift-pageup")
        );
        assert_eq!(convert_keystroke("alt 9").as_deref(), Some("alt-9"));
        assert_eq!(
            convert_keystroke("shift ESCAPE").as_deref(),
            Some("shift-escape")
        );
        assert_eq!(convert_keystroke("control NUMPAD5"), None);
    }

    #[test]
    fn imported_keymaps_bind_the_log_refresh() {
        let mac = parse_keymap(MACOS_KEYMAP);
        let default = parse_keymap(DEFAULT_KEYMAP);
        let find = |map: &[Shortcut], id: &str| {
            map.iter()
                .find(|s| s.action_id == id)
                .map(|s| s.keystrokes.clone())
        };
        assert_eq!(find(&mac, "Vcs.Log.Refresh").as_deref(), Some("cmd-r"));
        assert_eq!(
            find(&default, "Vcs.Log.Refresh").as_deref(),
            Some("ctrl-f5")
        );
        assert_eq!(
            find(&mac, "ActivateVersionControlToolWindow").as_deref(),
            Some("cmd-9")
        );
        // every implemented id is known to `bindable`
        for id in ACTION_IDS {
            assert!(bindable(id).is_some(), "{id}");
        }
    }

    #[test]
    fn two_stroke_lines_become_sequences() {
        let parsed = parse_keymap("# c\nX\tmeta K\tmeta S\nY\tcontrol NUMPAD1\n");
        assert_eq!(
            parsed,
            [Shortcut {
                action_id: "X".into(),
                keystrokes: "cmd-k cmd-s".into()
            }]
        );
    }
}
```

`crates/ig-app/src/console.rs`:

```rust
//! The Git tool window's Console tab.
//!
//! Port of `plugins/git4idea/backend/src/commands/GitCommandOutputConsolePrinter.kt` and
//! `plugins/git4idea/backend/src/util/GitVcsConsoleWriter.java`: a command start prints
//! `HH:mm:ss.SSS: [<working dir>] <command line>` in the console's system-output color, stdout lines
//! in the normal color, stderr lines in the error color; lines are cut to 10,000 chars
//! (`MAX_CONSOLE_OUTPUT_SIZE`: first 30% … last 70%). Only non-silent commands are logged, as
//! upstream; the "Show read commands in Console" toggle (an IdeaGit addition) also shows the
//! command lines of silent (read) commands.
//!
//! Deviations: no ANSI escape decoding (`AnsiEscapeDecoder`); the console keeps the last
//! [`MAX_LINES`] lines (upstream's console buffer is bounded by `idea.cycle.buffer.size`).

use std::collections::VecDeque;
use std::path::Path;

use chrono::{DateTime, Local};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, Styled, UniformListScrollHandle, Window, div, px,
    uniform_list,
};
use ig_git::console::{ConsoleEvent, ConsoleSink, OutputKind};
use ig_ui_kit::{ActiveIjTheme, color};

/// `GitVcsConsoleWriter.MAX_CONSOLE_OUTPUT_SIZE`.
const MAX_CONSOLE_OUTPUT_SIZE: usize = 10_000;
/// Lines kept in the console.
pub const MAX_LINES: usize = 20_000;

/// `ConsoleViewContentType` of a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    /// `SYSTEM_OUTPUT`: the command line.
    System,
    /// `NORMAL_OUTPUT`: stdout.
    Normal,
    /// `ERROR_OUTPUT`: stderr.
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleLine {
    pub kind: LineKind,
    pub text: String,
    /// A silent command's line (shown only with the debug toggle).
    pub silent: bool,
}

/// `GitVcsConsoleWriter.showCommandLine` with `GitCommandOutputConsolePrinter.showCommandStart`.
pub fn format_command_start(
    time: DateTime<Local>,
    working_dir: &Path,
    command_line: &str,
) -> String {
    shorten(&format!(
        "{}: [{}] {command_line}",
        time.format("%H:%M:%S%.3f"),
        working_dir.display()
    ))
}

/// `StringUtil.shortenPathWithEllipsis(message, MAX_CONSOLE_OUTPUT_SIZE)` (`...` symbol).
pub fn shorten(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= MAX_CONSOLE_OUTPUT_SIZE {
        return text.to_string();
    }
    let suffix = (MAX_CONSOLE_OUTPUT_SIZE as f64 * 0.7) as usize;
    let prefix = MAX_CONSOLE_OUTPUT_SIZE - suffix - 3;
    let mut out: String = chars[..prefix].iter().collect();
    out.push_str("...");
    out.extend(&chars[chars.len() - suffix..]);
    out
}

/// What a console sink sends to the UI.
pub enum ConsoleMessage {
    Event(DateTime<Local>, ConsoleEvent),
    Silent(DateTime<Local>, std::path::PathBuf, String),
}

/// The `ConsoleSink` given to a project's `Git`: forwards events over a channel (it runs on git
/// threads and must not block).
pub struct ChannelConsole(pub flume::Sender<ConsoleMessage>);

impl ConsoleSink for ChannelConsole {
    fn event(&self, event: ConsoleEvent) {
        let _ = self.0.send(ConsoleMessage::Event(Local::now(), event));
    }

    fn silent_command(&self, working_dir: &Path, command_line: &str) {
        let _ = self.0.send(ConsoleMessage::Silent(
            Local::now(),
            working_dir.to_path_buf(),
            command_line.to_string(),
        ));
    }
}

/// The console's lines.
#[derive(Default)]
pub struct ConsoleBuffer {
    lines: VecDeque<ConsoleLine>,
}

impl ConsoleBuffer {
    pub fn push(&mut self, message: ConsoleMessage) {
        let line = match message {
            ConsoleMessage::Event(
                time,
                ConsoleEvent::Started {
                    working_dir,
                    command_line,
                    ..
                },
            ) => ConsoleLine {
                kind: LineKind::System,
                text: format_command_start(time, &working_dir, &command_line),
                silent: false,
            },
            ConsoleMessage::Event(_, ConsoleEvent::Output { kind, line, .. }) => ConsoleLine {
                kind: if kind == OutputKind::Stderr {
                    LineKind::Error
                } else {
                    LineKind::Normal
                },
                text: shorten(&line),
                silent: false,
            },
            // `showCommandFinished` prints nothing
            ConsoleMessage::Event(_, ConsoleEvent::Finished { .. }) => return,
            ConsoleMessage::Silent(time, dir, command_line) => ConsoleLine {
                kind: LineKind::System,
                text: format_command_start(time, &dir, &command_line),
                silent: true,
            },
        };
        self.lines.push_back(line);
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
    }

    /// The lines shown, with or without silent commands.
    pub fn visible(&self, show_silent: bool) -> Vec<&ConsoleLine> {
        self.lines
            .iter()
            .filter(|l| show_silent || !l.silent)
            .collect()
    }
}

/// The Console tab.
pub struct ConsoleView {
    buffer: ConsoleBuffer,
    show_silent: bool,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    follow: bool,
}

impl ConsoleView {
    /// Creates the view and starts reading `rx`.
    pub fn new(rx: flume::Receiver<ConsoleMessage>, cx: &mut Context<ConsoleView>) -> ConsoleView {
        cx.spawn(async move |this, cx| {
            while let Ok(message) = rx.recv_async().await {
                let mut batch = vec![message];
                batch.extend(rx.try_iter());
                if this
                    .update(cx, |view, cx| {
                        for m in batch {
                            view.buffer.push(m);
                        }
                        view.follow = true;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        ConsoleView {
            buffer: ConsoleBuffer::default(),
            show_silent: false,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            follow: false,
        }
    }

    pub fn show_silent(&self) -> bool {
        self.show_silent
    }

    pub fn set_show_silent(&mut self, show: bool, cx: &mut Context<ConsoleView>) {
        self.show_silent = show;
        cx.notify();
    }

    pub fn line_count(&self) -> usize {
        self.buffer.visible(self.show_silent).len()
    }
}

impl Focusable for ConsoleView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ConsoleView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<ConsoleView>) -> impl IntoElement {
        let t = cx.ij().tokens;
        let lines: Vec<(LineKind, SharedString)> = self
            .buffer
            .visible(self.show_silent)
            .into_iter()
            .map(|l| (l.kind, SharedString::from(l.text.clone())))
            .collect();
        let count = lines.len();
        if self.follow && count > 0 {
            self.scroll
                .scroll_to_item(count - 1, gpui::ScrollStrategy::Bottom);
            self.follow = false;
        }
        let colors = [
            (LineKind::System, color(t.console_system)),
            (LineKind::Normal, color(t.console_normal)),
            (LineKind::Error, color(t.console_error)),
        ];
        div()
            .id("git-console")
            .track_focus(&self.focus)
            .size_full()
            .bg(color(t.console_background))
            .font_family(ig_ui_kit::fonts::MONO_FAMILY)
            .text_size(px(ig_ui_kit::metrics::MONO_FONT_SIZE))
            .child(
                uniform_list("git-console-lines", count, move |range, _, _| {
                    range
                        .map(|i| {
                            let (kind, text) = &lines[i];
                            let fg = colors.iter().find(|(k, _)| k == kind).map(|(_, c)| *c);
                            div()
                                .px(px(8.0))
                                .whitespace_nowrap()
                                .when_some(fg, |d, c| d.text_color(c))
                                .child(text.clone())
                        })
                        .collect()
                })
                .size_full()
                .track_scroll(&self.scroll),
            )
    }
}

/// Creates the console view and the sink to give to the project's `Git`.
pub fn create(cx: &mut App) -> (Entity<ConsoleView>, ChannelConsole) {
    let (tx, rx) = flume::unbounded();
    (cx.new(|cx| ConsoleView::new(rx, cx)), ChannelConsole(tx))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn command_start_has_time_dir_and_command() {
        let time = Local.with_ymd_and_hms(2026, 11, 8, 9, 5, 7).unwrap();
        assert_eq!(
            format_command_start(time, Path::new("/repo"), "git fetch origin"),
            "09:05:07.000: [/repo] git fetch origin"
        );
    }

    #[test]
    fn events_become_colored_lines_and_silent_ones_are_hidden_by_default() {
        let mut buffer = ConsoleBuffer::default();
        let now = Local::now();
        buffer.push(ConsoleMessage::Event(
            now,
            ConsoleEvent::Started {
                id: 1,
                working_dir: "/r".into(),
                command_line: "git commit".into(),
            },
        ));
        buffer.push(ConsoleMessage::Event(
            now,
            ConsoleEvent::Output {
                id: 1,
                kind: OutputKind::Stdout,
                line: "ok".into(),
            },
        ));
        buffer.push(ConsoleMessage::Event(
            now,
            ConsoleEvent::Output {
                id: 1,
                kind: OutputKind::Stderr,
                line: "warn".into(),
            },
        ));
        buffer.push(ConsoleMessage::Event(
            now,
            ConsoleEvent::Finished {
                id: 1,
                exit_code: 0,
            },
        ));
        buffer.push(ConsoleMessage::Silent(now, "/r".into(), "git log".into()));
        let kinds: Vec<LineKind> = buffer.visible(false).iter().map(|l| l.kind).collect();
        assert_eq!(kinds, [LineKind::System, LineKind::Normal, LineKind::Error]);
        assert_eq!(buffer.visible(true).len(), 4);
        assert!(buffer.visible(true)[3].text.ends_with("[/r] git log"));
    }

    #[test]
    fn long_lines_keep_head_and_tail() {
        let long = "a".repeat(5000) + &"b".repeat(10_000);
        let short = shorten(&long);
        assert_eq!(short.chars().count(), MAX_CONSOLE_OUTPUT_SIZE);
        assert!(short.starts_with("aaa") && short.ends_with("bbb") && short.contains("..."));
    }
}
```

- [ ] **Step 2: `crates/ig-app/src/lib.rs`**, with these modules only. Write exactly:

```rust
//! IdeaGit's application: windows, actions, the Git tool window (Log and Console) and the log
//! worker. Built on `ig-git` / `ig-log` for Git and `ig-ui-kit` for the IntelliJ New UI look.
//!
//! Every module names the upstream sources it ports or imitates; see also
//! `reference/upstream-map.toml`.

pub mod actions;
pub mod console;
pub mod logging;
pub mod paths;
pub mod settings;
pub mod strings;
```

- [ ] **Step 3: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code origin/wip/app-shell-prototype -- crates/ig-app/src/strings.rs crates/ig-app/src/paths.rs crates/ig-app/src/settings.rs crates/ig-app/src/logging.rs crates/ig-app/src/actions.rs crates/ig-app/src/console.rs
```

Expected: no output. The intermediate files written above differ from the branch until their final task.

- [ ] **Step 4: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 433 passed, 1 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(ig-app): strings, settings, logging, action registry and keymap, Git console"
```

---

### Task 7: `ig-app`: Log painters and helpers

**Files:**
- Create: `crates/ig-app/src/log/date.rs`, `crates/ig-app/src/log/ref_groups.rs`, `crates/ig-app/src/log/selection.rs`, `crates/ig-app/src/log/metadata.rs`, `crates/ig-app/src/log/graph_painter.rs`, `crates/ig-app/src/log/labels.rs`, `crates/ig-app/src/log/mod.rs`, `crates/ig-app/src/lib.rs`

**Review focus:** `graph_painter.rs` vs `SimpleGraphCellPainter`; `labels.rs` vs `LabelPainter` (grouping and compaction); `ref_groups.rs` vs `GitRefManager` groups; `date.rs` vs `DateFormatUtil.formatPrettyDateTime`.

- [ ] **Step 1: The modules.**

```bash
git checkout origin/wip/app-shell-prototype -- crates/ig-app/src/log/date.rs crates/ig-app/src/log/ref_groups.rs crates/ig-app/src/log/selection.rs crates/ig-app/src/log/metadata.rs crates/ig-app/src/log/graph_painter.rs crates/ig-app/src/log/labels.rs
```

`crates/ig-app/src/log/date.rs`:

```rust
//! The Log's Date column: IntelliJ's "pretty" relative date.
//!
//! Port of `DateFormatUtil.formatPrettyDateTime` (`platform/platform-api/src/com/intellij/util/text/DateFormatUtil.java`):
//! up to 61 minutes ago as "N minutes ago" (`UtilBundle` `date.format.minutes.ago`), then
//! "Today h:mm AM" / "Yesterday h:mm AM", otherwise the date and time.
//!
//! Deviation: the absolute format is the JDK's `en_US` `FormatStyle.SHORT` ("M/d/yy, h:mm AM").
//! Upstream asks `DateTimeFormatManager`, which follows the OS locale settings by default.

use chrono::{DateTime, Datelike, TimeZone};

use crate::strings;

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;

/// The texts the format needs, from `UtilBundle`.
pub struct DateTexts {
    pub minutes_ago: String,
    pub today: String,
    pub yesterday: String,
}

impl DateTexts {
    pub fn load() -> DateTexts {
        DateTexts {
            minutes_ago: strings::text("UtilBundle", "date.format.minutes.ago"),
            today: strings::text("UtilBundle", "date.format.today"),
            yesterday: strings::text("UtilBundle", "date.format.yesterday"),
        }
    }
}

/// `formatTime` (`FormatStyle.SHORT`, en_US).
fn format_time<Tz: TimeZone>(t: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    t.format("%-I:%M\u{202f}%p").to_string()
}

/// `formatPrettyDateTime(time)`: `time` and `now` are epoch milliseconds, shown in `tz`.
pub fn format_pretty<Tz: TimeZone>(time_ms: i64, now_ms: i64, tz: &Tz, texts: &DateTexts) -> String
where
    Tz::Offset: std::fmt::Display,
{
    if time_ms < 0 {
        return String::new();
    }
    let delta = now_ms - time_ms;
    if (0..=HOUR + MINUTE).contains(&delta) {
        let minutes = (delta as f64 / MINUTE as f64).round_ties_even() as i64;
        return strings::format_message(&texts.minutes_ago, &[&minutes.to_string()]);
    }
    let (Some(now), Some(time)) = (
        tz.timestamp_millis_opt(now_ms).single(),
        tz.timestamp_millis_opt(time_ms).single(),
    ) else {
        return String::new();
    };
    if now.date_naive() == time.date_naive() {
        return format!("{} {}", texts.today, format_time(&time));
    }
    let is_yesterday = now
        .date_naive()
        .pred_opt()
        .is_some_and(|d| d == time.date_naive());
    if is_yesterday {
        return format!("{} {}", texts.yesterday, format_time(&time));
    }
    format!(
        "{}/{}/{:02}, {}",
        time.month(),
        time.day(),
        time.year().rem_euclid(100),
        format_time(&time)
    )
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::*;

    fn texts() -> DateTexts {
        DateTexts {
            minutes_ago: "{0,choice, 0#Moments|1#A minute|2#{0,number} minutes|60#1 hour} ago"
                .into(),
            today: "Today".into(),
            yesterday: "Yesterday".into(),
        }
    }

    fn ms(tz: &FixedOffset, y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        tz.with_ymd_and_hms(y, mo, d, h, mi, 0)
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn recent_commits_show_minutes() {
        let tz = FixedOffset::east_opt(3600).unwrap();
        let now = ms(&tz, 2026, 11, 8, 15, 0);
        assert_eq!(
            format_pretty(now - 10_000, now, &tz, &texts()),
            "Moments ago"
        );
        assert_eq!(
            format_pretty(now - 42 * MINUTE, now, &tz, &texts()),
            "42 minutes ago"
        );
        assert_eq!(
            format_pretty(now - 61 * MINUTE, now, &tz, &texts()),
            "1 hour ago"
        );
    }

    #[test]
    fn today_and_yesterday_use_local_days() {
        let tz = FixedOffset::east_opt(3600).unwrap();
        let now = ms(&tz, 2026, 11, 8, 15, 0);
        let nbsp = '\u{202f}';
        assert_eq!(
            format_pretty(ms(&tz, 2026, 11, 8, 9, 5), now, &tz, &texts()),
            format!("Today 9:05{nbsp}AM")
        );
        assert_eq!(
            format_pretty(ms(&tz, 2026, 11, 7, 23, 59), now, &tz, &texts()),
            format!("Yesterday 11:59{nbsp}PM")
        );
        assert_eq!(
            format_pretty(
                ms(&tz, 2026, 1, 1, 0, 0),
                ms(&tz, 2026, 1, 1, 10, 0),
                &tz,
                &texts()
            ),
            format!("Today 12:00{nbsp}AM")
        );
        // yesterday across a year boundary
        assert_eq!(
            format_pretty(
                ms(&tz, 2025, 12, 31, 13, 0),
                ms(&tz, 2026, 1, 1, 10, 0),
                &tz,
                &texts()
            ),
            format!("Yesterday 1:00{nbsp}PM")
        );
    }

    #[test]
    fn older_commits_show_the_short_date() {
        let tz = FixedOffset::east_opt(0).unwrap();
        let now = ms(&tz, 2026, 11, 8, 15, 0);
        assert_eq!(
            format_pretty(ms(&tz, 2017, 7, 14, 2, 40), now, &tz, &texts()),
            "7/14/17, 2:40\u{202f}AM"
        );
        assert_eq!(format_pretty(-1, now, &tz, &texts()), "");
    }
}
```

`crates/ig-app/src/log/ref_groups.rs`:

```rust
//! Groups the references of a commit for its Log row label.
//!
//! Port of `GitRefManager.groupForTable` (`plugins/git4idea/backend/src/log/GitRefManager.kt`),
//! `SimpleRefGroup.buildGroups` / `getColors` and `DetachedHeadRefGroup`
//! (`platform/vcs-log/impl/src/com/intellij/vcs/log/impl/`).
//!
//! Deviations: tracked groups (`origin & main`) are found by name only (upstream's
//! `createTrackedGroup` fallback: a remote branch named `<remote>/<local>`), because branch tracking
//! info isn't read yet; ref type colors are [`RefColor`] keys the UI maps to theme colors
//! (`VersionControl.GitLog.*IconColor`).

use ig_git::log::ref_manager::{HEAD, LOCAL_BRANCH, REMOTE_BRANCH, TAG};
use ig_log::data::{VcsRef, VcsRefType};

/// `GitRefManager.REMOTE_TABLE_SEPARATOR`.
const REMOTE_TABLE_SEPARATOR: &str = " & ";

/// A ref type's label color (upstream `VcsRefType.getBackgroundColor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RefColor {
    Head,
    LocalBranch,
    RemoteBranch,
    Tag,
    Other,
}

impl RefColor {
    pub fn of(ref_type: VcsRefType) -> RefColor {
        match ref_type {
            t if t == HEAD => RefColor::Head,
            t if t == LOCAL_BRANCH => RefColor::LocalBranch,
            t if t == REMOTE_BRANCH => RefColor::RemoteBranch,
            t if t == TAG => RefColor::Tag,
            _ => RefColor::Other,
        }
    }
}

/// A label group (`RefGroup`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefGroup {
    pub name: String,
    pub refs: Vec<VcsRef>,
    /// `DetachedHeadRefGroup`: a warning label instead of ref colors.
    pub detached_head: bool,
}

impl RefGroup {
    fn simple(name: impl Into<String>, refs: Vec<VcsRef>) -> RefGroup {
        RefGroup {
            name: name.into(),
            refs,
            detached_head: false,
        }
    }

    /// `SimpleRefGroup.getColors`: one color per ref type in order of appearance, twice when the
    /// type has several refs. Detached-head groups have none.
    pub fn colors(&self) -> Vec<RefColor> {
        if self.detached_head {
            return Vec::new();
        }
        let mut types: Vec<(VcsRefType, usize)> = Vec::new();
        for r in &self.refs {
            match types.iter_mut().find(|(t, _)| *t == r.ref_type()) {
                Some((_, n)) => *n += 1,
                None => types.push((r.ref_type(), 1)),
            }
        }
        types
            .into_iter()
            .flat_map(|(t, n)| std::iter::repeat_n(RefColor::of(t), if n > 1 { 2 } else { 1 }))
            .collect()
    }
}

/// The repository state grouping needs.
#[derive(Clone, Debug, Default)]
pub struct RepoRefsContext<'a> {
    /// `None` when HEAD is detached (or there are no commits).
    pub current_branch: Option<&'a str>,
    pub remotes: &'a [String],
}

/// `groupForTable(references, compact, showTagNames)`. `sorted` must already be in label order
/// (`VcsLogRefManager::sort_labels`); `detached_head_name` is `VcsLogBundle`'s
/// `vcs.log.references.detached.head`.
pub fn group_for_table(
    sorted: &[VcsRef],
    repo: &RepoRefsContext<'_>,
    compact: bool,
    show_tag_names: bool,
    detached_head_name: &str,
) -> Vec<RefGroup> {
    if sorted.is_empty() {
        return Vec::new();
    }
    // `ContainerUtil.groupBy(sortedReferences) { it.type }`: types in order of appearance
    let mut grouped: Vec<(VcsRefType, Vec<VcsRef>)> = Vec::new();
    for r in sorted {
        match grouped.iter_mut().find(|(t, _)| *t == r.ref_type()) {
            Some((_, refs)) => refs.push(r.clone()),
            None => grouped.push((r.ref_type(), vec![r.clone()])),
        }
    }
    let take = |grouped: &mut Vec<(VcsRefType, Vec<VcsRef>)>, t: VcsRefType| -> Vec<VcsRef> {
        match grouped.iter().position(|(gt, _)| *gt == t) {
            Some(i) => grouped.remove(i).1,
            None => Vec::new(),
        }
    };
    let head_refs = take(&mut grouped, HEAD);

    // tracked groups (name-based `createTrackedGroup`)
    let mut groups: Vec<RefGroup> = Vec::new();
    let locals: Vec<VcsRef> = grouped
        .iter()
        .find(|(t, _)| *t == LOCAL_BRANCH)
        .map(|(_, r)| r.clone())
        .unwrap_or_default();
    let mut tracked = Vec::new();
    for local in &locals {
        let candidate = repo.remotes.iter().find_map(|remote| {
            let name = format!("{remote}/{}", local.name());
            grouped
                .iter()
                .find(|(t, _)| *t == REMOTE_BRANCH)
                .and_then(|(_, rs)| rs.iter().find(|r| r.name() == name))
                .map(|r| (remote.clone(), r.clone()))
        });
        if let Some((remote, remote_ref)) = candidate {
            tracked.push(RefGroup::simple(
                format!("{remote}{REMOTE_TABLE_SEPARATOR}{}", local.name()),
                vec![local.clone(), remote_ref],
            ));
        }
    }
    for group in &tracked {
        for (_, refs) in grouped.iter_mut() {
            refs.retain(|r| !group.refs.contains(r));
        }
    }
    grouped.retain(|(_, refs)| !refs.is_empty());

    // current branch first
    let current = repo.current_branch.and_then(|branch| {
        let (_, locals) = grouped.iter_mut().find(|(t, _)| *t == LOCAL_BRANCH)?;
        let i = locals.iter().position(|r| r.name() == branch)?;
        Some(RefGroup::simple(branch, vec![locals.remove(i)]))
    });
    grouped.retain(|(_, refs)| !refs.is_empty());
    groups.extend(current);
    groups.extend(tracked);

    let mut result = build_groups(groups, grouped, compact, show_tag_names);
    if head_refs.is_empty() {
        return result;
    }
    if repo.current_branch.is_none() {
        result.insert(
            0,
            RefGroup {
                name: detached_head_name.to_string(),
                refs: head_refs,
                detached_head: true,
            },
        );
    } else if let Some(first) = result.first_mut() {
        first.refs.splice(0..0, head_refs);
    } else {
        result.push(RefGroup::simple("", head_refs));
    }
    result
}

/// `SimpleRefGroup.buildGroups`.
fn build_groups(
    ref_groups: Vec<RefGroup>,
    refs: Vec<(VcsRefType, Vec<VcsRef>)>,
    compact: bool,
    show_tag_names: bool,
) -> Vec<RefGroup> {
    if refs.is_empty() {
        return ref_groups;
    }
    if compact {
        let first_ref = &refs[0].1[0];
        if ref_groups.is_empty() {
            let name = if first_ref.ref_type().is_branch() || show_tag_names {
                first_ref.name().to_string()
            } else {
                String::new()
            };
            return vec![RefGroup::simple(
                name,
                refs.into_iter().flat_map(|(_, r)| r).collect(),
            )];
        }
        let name = ref_groups[0].name.clone();
        let all = ref_groups
            .into_iter()
            .flat_map(|g| g.refs)
            .chain(refs.into_iter().flat_map(|(_, r)| r))
            .collect();
        return vec![RefGroup::simple(name, all)];
    }
    let mut result = ref_groups;
    for (ref_type, refs_of_type) in refs {
        if ref_type.is_branch() {
            result.extend(refs_of_type.into_iter().map(|r| {
                let name = r.name().to_string();
                RefGroup::simple(name, vec![r])
            }));
        } else {
            let name = if show_tag_names {
                refs_of_type[0].name().to_string()
            } else {
                String::new()
            };
            result.push(RefGroup::simple(name, refs_of_type));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use ig_log::data::{Hash, vcs_root};

    use super::*;

    fn r(name: &str, t: VcsRefType) -> VcsRef {
        VcsRef::new(Hash::build("abcd"), name, t, vcs_root("/r"))
    }

    fn names(groups: &[RefGroup]) -> Vec<(&str, usize)> {
        groups
            .iter()
            .map(|g| (g.name.as_str(), g.refs.len()))
            .collect()
    }

    #[test]
    fn non_compact_groups_current_tracked_then_each_branch_and_tags_together() {
        let refs = [
            r("HEAD", HEAD),
            r("main", LOCAL_BRANCH),
            r("feature", LOCAL_BRANCH),
            r("origin/feature", REMOTE_BRANCH),
            r("origin/other", REMOTE_BRANCH),
            r("v1", TAG),
            r("v2", TAG),
        ];
        let remotes = ["origin".to_string()];
        let repo = RepoRefsContext {
            current_branch: Some("main"),
            remotes: &remotes,
        };
        let groups = group_for_table(&refs, &repo, false, false, "Detached HEAD");
        assert_eq!(
            names(&groups),
            [
                ("main", 2),
                ("origin & feature", 2),
                ("origin/other", 1),
                ("", 2)
            ]
        );
        // HEAD joined the first group
        assert_eq!(groups[0].colors(), [RefColor::Head, RefColor::LocalBranch]);
        assert_eq!(groups[3].colors(), [RefColor::Tag, RefColor::Tag]);
    }

    #[test]
    fn compact_view_makes_one_group_named_after_the_first() {
        let refs = [
            r("main", LOCAL_BRANCH),
            r("origin/main", REMOTE_BRANCH),
            r("v1", TAG),
        ];
        let remotes = ["origin".to_string()];
        let repo = RepoRefsContext {
            current_branch: Some("dev"),
            remotes: &remotes,
        };
        let groups = group_for_table(&refs, &repo, true, false, "Detached HEAD");
        assert_eq!(names(&groups), [("origin & main", 3)]);
        assert_eq!(
            groups[0].colors(),
            [RefColor::LocalBranch, RefColor::RemoteBranch, RefColor::Tag]
        );
        // only tags: no name unless tag names are shown
        let tags = [r("v1", TAG), r("v2", TAG)];
        assert_eq!(
            names(&group_for_table(&tags, &repo, true, false, "")),
            [("", 2)]
        );
        assert_eq!(
            names(&group_for_table(&tags, &repo, true, true, "")),
            [("v1", 2)]
        );
    }

    #[test]
    fn detached_head_gets_a_warning_group_first() {
        let refs = [r("HEAD", HEAD), r("v1", TAG)];
        let repo = RepoRefsContext {
            current_branch: None,
            remotes: &[],
        };
        let groups = group_for_table(&refs, &repo, true, false, "Detached HEAD");
        assert_eq!(names(&groups), [("Detached HEAD", 1), ("", 1)]);
        assert!(groups[0].detached_head && groups[0].colors().is_empty());
    }
}
```

`crates/ig-app/src/log/selection.rs`:

```rust
//! Row selection of the Log table, kept by commit so it survives refreshes.
//!
//! Behaves like the Swing `JTable` the Log uses (`VcsLogGraphTable`, `ListSelectionModel`
//! `MULTIPLE_INTERVAL_SELECTION`): click selects one row, Cmd/Ctrl+click toggles a row,
//! Shift+click and Shift+arrows extend from the anchor, arrows move the lead and select it.
//! Upstream restores the selection by commit after a refresh (`VcsLogGraphTable.Selection`);
//! here the selection *is* the set of commits, so rows are recomputed from the new graph.

use std::collections::BTreeSet;

use ig_log::data::VcsLogCommitStorageIndex as Commit;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    commits: BTreeSet<Commit>,
    anchor: Option<Commit>,
    lead: Option<Commit>,
}

/// Maps rows to commits and back for the current graph.
pub trait Rows {
    fn count(&self) -> usize;
    fn commit(&self, row: usize) -> Commit;
    fn row(&self, commit: Commit) -> Option<usize>;
}

impl Selection {
    pub fn is_selected(&self, commit: Commit) -> bool {
        self.commits.contains(&commit)
    }

    pub fn commits(&self) -> &BTreeSet<Commit> {
        &self.commits
    }

    pub fn len(&self) -> usize {
        self.commits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commits.is_empty()
    }

    /// The lead row (where keyboard navigation continues), if its commit is still shown.
    pub fn lead_row(&self, rows: &dyn Rows) -> Option<usize> {
        self.lead.and_then(|c| rows.row(c))
    }

    /// A click on `row`: `toggle` with Cmd/Ctrl, `extend` with Shift.
    pub fn click(&mut self, row: usize, toggle: bool, extend: bool, rows: &dyn Rows) {
        let commit = rows.commit(row);
        if extend {
            let anchor_row = self.anchor.and_then(|a| rows.row(a)).unwrap_or(row);
            if !toggle {
                self.commits.clear();
            }
            let (from, to) = (anchor_row.min(row), anchor_row.max(row));
            self.commits.extend((from..=to).map(|r| rows.commit(r)));
            self.anchor.get_or_insert(commit);
        } else if toggle {
            if !self.commits.remove(&commit) {
                self.commits.insert(commit);
            }
            self.anchor = Some(commit);
        } else {
            self.commits.clear();
            self.commits.insert(commit);
            self.anchor = Some(commit);
        }
        self.lead = Some(commit);
    }

    /// Moves the lead by `delta` rows (clamped); `extend` keeps the anchor (Shift+arrows).
    /// Returns the new lead row, to scroll to.
    pub fn move_by(&mut self, delta: isize, extend: bool, rows: &dyn Rows) -> Option<usize> {
        let count = rows.count();
        if count == 0 {
            return None;
        }
        let target = match self.lead_row(rows) {
            Some(lead) => (lead as isize + delta).clamp(0, count as isize - 1) as usize,
            None => {
                if delta >= 0 {
                    0
                } else {
                    count - 1
                }
            }
        };
        self.click(target, false, extend, rows);
        Some(target)
    }

    /// Selects every row (`$SelectAll`).
    pub fn select_all(&mut self, rows: &dyn Rows) {
        self.commits = (0..rows.count()).map(|r| rows.commit(r)).collect();
    }

    /// Selects only `row` (a jump).
    pub fn select_only(&mut self, row: usize, rows: &dyn Rows) {
        self.click(row, false, false, rows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Row r shows commit 100 + r.
    struct Linear(usize);

    impl Rows for Linear {
        fn count(&self) -> usize {
            self.0
        }
        fn commit(&self, row: usize) -> Commit {
            100 + row as Commit
        }
        fn row(&self, commit: Commit) -> Option<usize> {
            let r = (commit - 100) as usize;
            (r < self.0).then_some(r)
        }
    }

    fn rows_of(s: &Selection) -> Vec<Commit> {
        s.commits().iter().map(|c| c - 100).collect()
    }

    #[test]
    fn click_toggle_and_shift_extend() {
        let rows = Linear(10);
        let mut s = Selection::default();
        s.click(2, false, false, &rows);
        s.click(5, false, true, &rows);
        assert_eq!(rows_of(&s), [2, 3, 4, 5]);
        s.click(8, true, false, &rows);
        assert_eq!(rows_of(&s), [2, 3, 4, 5, 8]);
        s.click(3, true, false, &rows);
        assert_eq!(rows_of(&s), [2, 4, 5, 8]);
        s.click(1, false, false, &rows);
        assert_eq!(rows_of(&s), [1]);
    }

    #[test]
    fn keyboard_moves_and_extends_within_bounds() {
        let rows = Linear(4);
        let mut s = Selection::default();
        assert_eq!(s.move_by(1, false, &rows), Some(0));
        assert_eq!(s.move_by(1, true, &rows), Some(1));
        assert_eq!(s.move_by(10, true, &rows), Some(3));
        assert_eq!(rows_of(&s), [0, 1, 2, 3]);
        assert_eq!(s.move_by(-1, false, &rows), Some(2));
        assert_eq!(rows_of(&s), [2]);
        s.select_all(&rows);
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn selection_follows_commits_to_new_rows() {
        let mut s = Selection::default();
        s.click(1, false, false, &Linear(3));
        // after a refresh, commit 101 is gone: no lead row, the next move starts at the top
        assert_eq!(s.lead_row(&Linear(1)), None);
        assert_eq!(s.move_by(1, false, &Linear(1)), Some(0));
    }
}
```

`crates/ig-app/src/log/metadata.rs`:

```rust
//! Row metadata (subject, author, date) for the Log table, loaded lazily for the visible rows.
//!
//! Port of the UI-thread side of `platform/vcs-log/impl/src/com/intellij/vcs/log/data/MiniDetailsGetter.kt`
//! with `TopCommitsCache.java`:
//! - The cache is first filled with the recent commits read with each first block (upstream
//!   `TopCommitsCache` fed by `storeRecentDetails`).
//! - Rows not cached get a "loading" placeholder tagged with the number of the load task that
//!   asked for them, so a scroll doesn't re-request rows already being loaded.
//! - Placeholders of tasks [`MAX_LOADING_TASKS`] or more behind the current one are dropped (the
//!   background loader is a LIFO queue keeping the newest tasks; older ones are discarded).
//! - The cache is bounded ([`CACHE_SIZE`], upstream `maximumSize(10000)`); entries are evicted
//!   least recently used first.
//!
//! Deviation: one cache for both upstream maps (top commits are not kept separately).

use std::collections::HashMap;
use std::sync::Arc;

use ig_log::GraphCommit;
use ig_log::data::{Hash, VcsCommitMetadata, VcsLogCommitStorageIndex};

/// `MiniDetailsGetter.MAX_LOADING_TASKS`.
pub const MAX_LOADING_TASKS: u64 = 10;
/// `MiniDetailsGetter`'s Caffeine `maximumSize`.
pub const CACHE_SIZE: usize = 10_000;
/// Rows loaded per `read_metadata` call (`VcsLogData`/`GitLogProvider` batch size).
pub const BATCH_SIZE: usize = 100;

/// What a Log row shows besides the graph and labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowMeta {
    pub hash: Hash,
    pub subject: String,
    pub author: String,
    /// Epoch milliseconds.
    pub author_time: i64,
}

impl RowMeta {
    pub fn from_metadata(metadata: &VcsCommitMetadata) -> RowMeta {
        RowMeta {
            hash: metadata.id().clone(),
            // `GraphCommitCellRenderer.appendText`: tabs become spaces, trimmed
            subject: metadata.subject().replace('\t', " ").trim().to_string(),
            // `CommitPresentationUtil.getAuthorPresentation`: the author name
            author: metadata.author().name().to_string(),
            author_time: metadata.author_time(),
        }
    }
}

#[derive(Clone, Debug)]
enum Entry {
    Loaded(Arc<RowMeta>),
    Loading { task: u64 },
}

/// The cache with loading placeholders.
#[derive(Default)]
pub struct MetadataCache {
    entries: HashMap<VcsLogCommitStorageIndex, (Entry, u64)>,
    current_task: u64,
    clock: u64,
}

/// A cache lookup.
#[derive(Clone, Debug, PartialEq)]
pub enum Lookup {
    Loaded(Arc<RowMeta>),
    /// Requested, not loaded yet ("Loading…").
    Loading,
    Missing,
}

impl MetadataCache {
    pub fn new() -> MetadataCache {
        MetadataCache::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `getFromCacheAndCleanOldPlaceholder` (without loading).
    pub fn get(&mut self, commit: VcsLogCommitStorageIndex) -> Lookup {
        self.clock += 1;
        let clock = self.clock;
        let current = self.current_task;
        match self.entries.get_mut(&commit) {
            Some((Entry::Loaded(meta), used)) => {
                *used = clock;
                Lookup::Loaded(meta.clone())
            }
            Some((Entry::Loading { task }, _)) => {
                if *task + MAX_LOADING_TASKS <= current {
                    self.entries.remove(&commit);
                    Lookup::Missing
                } else {
                    Lookup::Loading
                }
            }
            None => Lookup::Missing,
        }
    }

    /// `getCommitData(commit, commitsToLoad)`: marks the missing ones of `commits` as loading under
    /// a new task and returns them (empty: nothing to request).
    pub fn request(
        &mut self,
        commits: impl IntoIterator<Item = VcsLogCommitStorageIndex>,
    ) -> Vec<VcsLogCommitStorageIndex> {
        let missing: Vec<_> = commits
            .into_iter()
            .filter(|&c| self.get(c) == Lookup::Missing)
            .collect();
        if missing.is_empty() {
            return missing;
        }
        let task = self.current_task;
        self.current_task += 1;
        for &commit in &missing {
            self.entries
                .insert(commit, (Entry::Loading { task }, self.clock));
        }
        missing
    }

    /// Stores loaded metadata (`saveInCache`), also for the recent details of a first block.
    pub fn put(&mut self, loaded: impl IntoIterator<Item = (VcsLogCommitStorageIndex, RowMeta)>) {
        for (commit, meta) in loaded {
            self.clock += 1;
            self.entries
                .insert(commit, (Entry::Loaded(Arc::new(meta)), self.clock));
        }
        self.evict();
    }

    /// Drops the placeholders of a failed load, so the rows are requested again.
    pub fn forget(&mut self, commits: &[VcsLogCommitStorageIndex]) {
        for commit in commits {
            if let Some((Entry::Loading { .. }, _)) = self.entries.get(commit) {
                self.entries.remove(commit);
            }
        }
    }

    fn evict(&mut self) {
        if self.entries.len() <= CACHE_SIZE {
            return;
        }
        let mut used: Vec<u64> = self.entries.values().map(|(_, u)| *u).collect();
        let excess = self.entries.len() - CACHE_SIZE;
        let (_, cutoff, _) = used.select_nth_unstable(excess - 1);
        let cutoff = *cutoff;
        self.entries.retain(|_, (_, u)| *u > cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(subject: &str) -> RowMeta {
        RowMeta {
            hash: Hash::build("0123456789abcdef0123456789abcdef01234567"),
            subject: subject.to_string(),
            author: "A".to_string(),
            author_time: 0,
        }
    }

    #[test]
    fn requested_rows_are_loading_until_put() {
        let mut cache = MetadataCache::new();
        assert_eq!(cache.request([1, 2, 3]), [1, 2, 3]);
        assert_eq!(cache.get(2), Lookup::Loading);
        // already loading: no new request
        assert!(cache.request([1, 2, 3]).is_empty());
        cache.put([(2, meta("two"))]);
        assert_eq!(cache.get(2), Lookup::Loaded(Arc::new(meta("two"))));
        cache.forget(&[1, 2]);
        assert_eq!(cache.get(1), Lookup::Missing);
        assert!(matches!(cache.get(2), Lookup::Loaded(_)));
    }

    #[test]
    fn stale_placeholders_expire_after_max_loading_tasks() {
        let mut cache = MetadataCache::new();
        cache.request([100]); // task 0
        // upstream drops a placeholder once `task <= current - MAX_LOADING_TASKS`
        for i in 0..MAX_LOADING_TASKS as i32 - 2 {
            cache.request([i]);
        }
        assert_eq!(cache.get(100), Lookup::Loading);
        cache.request([1000]);
        assert_eq!(cache.get(100), Lookup::Missing);
        assert_eq!(cache.request([100]), [100]);
    }

    #[test]
    fn cache_is_bounded_and_keeps_recently_used_rows() {
        let mut cache = MetadataCache::new();
        cache.put([(0, meta("first"))]);
        cache.put((1..CACHE_SIZE as i32).map(|i| (i, meta("x"))));
        assert!(matches!(cache.get(0), Lookup::Loaded(_))); // touch the oldest
        cache.put([(-1, meta("new"))]);
        assert_eq!(cache.len(), CACHE_SIZE);
        assert!(matches!(cache.get(0), Lookup::Loaded(_)));
        assert_eq!(cache.get(1), Lookup::Missing);
    }

    #[test]
    fn subject_is_trimmed_with_tabs_replaced() {
        let metadata = VcsCommitMetadata::new(
            Hash::build("ab"),
            vec![],
            5,
            ig_log::data::vcs_root("/r"),
            "\tfix\tit \n".to_string(),
            ig_log::data::VcsUser::create_user("Ann", "a@x"),
            "fix\tit".to_string(),
            ig_log::data::VcsUser::create_user("Bob", "b@x"),
            7,
        );
        let row = RowMeta::from_metadata(&metadata);
        assert_eq!(
            (row.subject.as_str(), row.author.as_str(), row.author_time),
            ("fix it", "Ann", 7)
        );
    }
}
```

`crates/ig-app/src/log/graph_painter.rs`:

```rust
//! Paints a row of the commit graph from its print elements, and finds the element under the mouse.
//!
//! Port of `platform/vcs-log/impl/src/com/intellij/vcs/log/paint/SimpleGraphCellPainter.kt`,
//! `HeadNodePainter.kt`, `PaintParameters.java`, `GraphCommitCellUtil.kt` (`getGraphWidth`) and
//! the `PaintUtil.alignToInt` device-pixel alignment they use.
//!
//! The painter is split in two: [`GraphGeometry::shapes`] turns print elements into lines and
//! circles (pure, tested), and [`paint_shapes`] draws them with GPUI.
//!
//! Deviations:
//! - Dashed lines start with a full dash: GPUI's dash arrays have no phase (upstream offsets the
//!   pattern by half a dash).
//! - The `OUTLINE` node stroke uses the ordinary line thickness (upstream draws it with whatever
//!   stroke the previous element left on the `Graphics2D`).

use gpui::{
    App, Bounds, ContentMask, Hsla, PathBuilder, PathStyle, Pixels, StrokeOptions, Window, point,
    px, quad, transparent_black,
};
use ig_log::GraphPrintElement;
use ig_log::print_element::{
    EdgePrintElement, EdgePrintElementType, LineStyle, NodePrintElementType, PrintElement,
};
use ig_ui_kit::metrics::{
    GRAPH_CIRCLE_RADIUS, GRAPH_MAX_WIDTH_ELEMENTS, GRAPH_SELECT_THICK_LINE, GRAPH_TEXT_GAP,
    GRAPH_THICK_LINE, GRAPH_WIDTH_NODE, scale_with_row_height,
};
use lyon::tessellation::{LineCap, LineJoin};

/// `SimpleGraphCellPainter.ARROW_ANGLE_COS2`.
const ARROW_ANGLE_COS2: f32 = 0.7;
/// `SimpleGraphCellPainter.ARROW_LENGTH`.
const ARROW_LENGTH: f32 = 0.3;
/// `HeadNodePainter.RADIUS_DELTA`.
const RADIUS_DELTA: f32 = 2.0;
/// `JBColor.GRAY` (`Gray._128`), the `OUTLINE` node color.
pub const NEUTRAL_COLOR: u32 = 0x808080;

/// `PaintUtil.RoundingMode` subset.
#[derive(Clone, Copy)]
enum Rounding {
    Floor,
}

/// `PaintUtil.ParityMode`.
#[derive(Clone, Copy, PartialEq)]
enum Parity {
    Odd,
}

/// `PaintUtil.alignToInt(usrValue, ctx, FLOOR, pm)`.
fn align_to_int(value: f32, scale: f32, _rm: Rounding, parity: Option<Parity>) -> f32 {
    if scale == 0.0 {
        return 0.0;
    }
    let mut dev = (value * scale).floor() as i32;
    if parity == Some(Parity::Odd) && dev % 2 == 0 {
        dev -= 1;
    }
    dev as f32 / scale
}

/// A colored shape of a row, in the row's coordinates (logical pixels).
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Line {
        from: (f32, f32),
        to: (f32, f32),
        width: f32,
        /// `(dash, space)` for dashed lines.
        dash: Option<(f32, f32)>,
        color: u32,
    },
    /// A filled circle given by its bounding box (`Ellipse2D(x, y, d, d)`).
    Circle {
        x: f32,
        y: f32,
        diameter: f32,
        color: Color,
    },
    /// An outlined circle.
    Ring {
        x: f32,
        y: f32,
        diameter: f32,
        width: f32,
        color: u32,
    },
}

/// A shape color: a branch color, or the row background (the head node's middle ring).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    Rgb(u32),
    RowBackground,
}

/// The painter's metrics for a row height and display scale (`SimpleGraphCellPainter.MyPainter`).
#[derive(Clone, Debug, PartialEq)]
pub struct GraphGeometry {
    pub row_height: f32,
    row_center: f32,
    element_width: f32,
    element_center: f32,
    line_thickness: f32,
    selected_line_thickness: f32,
    circle_diameter: f32,
    selected_circle_diameter: f32,
    circle_radius: f32,
    selected_circle_radius: f32,
    // HeadNodePainter
    outer_circle_diameter: f32,
    outer_circle_radius: f32,
    selected_outer_circle_diameter: f32,
    selected_outer_circle_radius: f32,
    head_delta: f32,
}

impl GraphGeometry {
    /// `scale` is the device pixel ratio (`ScaleContext`'s system scale).
    pub fn new(row_height: f32, scale: f32) -> GraphGeometry {
        let s = |v: f32| scale_with_row_height(v, row_height);
        let floor_odd = |v: f32| align_to_int(v, scale, Rounding::Floor, Some(Parity::Odd));
        let floor = |v: f32| align_to_int(v, scale, Rounding::Floor, None);
        let pixel = 1.0 / scale;

        let line_thickness = floor_odd(s(GRAPH_THICK_LINE)).max(pixel);
        let selected_line_thickness =
            floor_odd(s(GRAPH_SELECT_THICK_LINE)).max(line_thickness + 2.0 * pixel);
        let circle_diameter = floor_odd(2.0 * s(GRAPH_CIRCLE_RADIUS));
        let selected_circle_diameter = circle_diameter + selected_line_thickness - line_thickness;
        let outer_circle_diameter = floor_odd(2.0 * s(GRAPH_CIRCLE_RADIUS + RADIUS_DELTA));
        let selected_outer_circle_diameter =
            outer_circle_diameter + selected_line_thickness - line_thickness;
        GraphGeometry {
            row_height,
            row_center: floor_odd(row_height / 2.0),
            element_width: floor_odd(s(GRAPH_WIDTH_NODE)),
            element_center: floor(s(GRAPH_WIDTH_NODE) / 2.0),
            line_thickness,
            selected_line_thickness,
            circle_diameter,
            selected_circle_diameter,
            circle_radius: floor(circle_diameter / 2.0),
            selected_circle_radius: floor(selected_circle_diameter / 2.0),
            outer_circle_diameter,
            outer_circle_radius: floor(outer_circle_diameter / 2.0),
            selected_outer_circle_diameter,
            selected_outer_circle_radius: floor(selected_outer_circle_diameter / 2.0),
            head_delta: floor(s(RADIUS_DELTA)),
        }
    }

    fn x(&self, position: i32) -> f32 {
        self.element_width * position as f32 + self.element_center
    }

    /// `GraphCommitCellUtil.getGraphWidth`: the width taken by the graph before the text.
    pub fn graph_width(&self, elements: &[GraphPrintElement], recommended_width: i32) -> f32 {
        if elements.is_empty() {
            return 0.0;
        }
        let mut max_index: f32 = 0.0;
        for element in elements {
            let position = element.position_in_current_row() as f32;
            max_index = max_index.max(position);
            if let GraphPrintElement::Edge(edge) = element {
                let other = edge.position_in_other_row() as f32;
                max_index = max_index.max((position + other) / 2.0);
            }
        }
        max_index += 1.0;
        max_index = max_index.max(GRAPH_MAX_WIDTH_ELEMENTS.min(recommended_width as f32));
        let element_width = scale_with_row_height(GRAPH_WIDTH_NODE, self.row_height);
        let gap = scale_with_row_height(GRAPH_TEXT_GAP, self.row_height);
        (max_index * element_width).trunc() + gap.trunc()
    }

    /// `SimpleGraphCellPainter.paint`: non-selected elements, then the selected ones twice (a
    /// thick `mark` outline, then the normal color on top). `color_of` maps a color id.
    pub fn shapes(
        &self,
        elements: &[GraphPrintElement],
        color_of: impl Fn(i32) -> u32,
        mark_color: u32,
    ) -> Vec<Shape> {
        let mut shapes = Vec::new();
        let (selected, normal): (Vec<_>, Vec<_>) = elements.iter().partition(|e| e.is_selected());
        for element in normal {
            self.element(&mut shapes, element, color_of(element.color_id()), false);
        }
        for element in &selected {
            self.element(&mut shapes, element, mark_color, true);
        }
        for element in &selected {
            self.element(&mut shapes, element, color_of(element.color_id()), false);
        }
        shapes
    }

    fn element(
        &self,
        out: &mut Vec<Shape>,
        element: &GraphPrintElement,
        color: u32,
        selected: bool,
    ) {
        match element {
            GraphPrintElement::Edge(edge) => self.edge(out, edge, color, selected),
            GraphPrintElement::Node(node) => {
                let x0 = self.x(node.position_in_current_row());
                let y0 = self.row_center;
                let (radius, diameter) = if selected {
                    (self.selected_circle_radius, self.selected_circle_diameter)
                } else {
                    (self.circle_radius, self.circle_diameter)
                };
                match ig_log::print_element::NodePrintElement::node_type(node) {
                    NodePrintElementType::Fill => out.push(Shape::Circle {
                        x: x0 - radius,
                        y: y0 - radius,
                        diameter,
                        color: Color::Rgb(color),
                    }),
                    NodePrintElementType::OutlineAndFill => {
                        self.head_node(out, x0, y0, color, selected)
                    }
                    NodePrintElementType::Outline => out.push(Shape::Ring {
                        x: x0 - radius,
                        y: y0 - radius,
                        diameter,
                        width: self.line_thickness,
                        color: NEUTRAL_COLOR,
                    }),
                }
            }
        }
    }

    /// `HeadNodePainter.paint`.
    fn head_node(&self, out: &mut Vec<Shape>, x0: f32, y0: f32, color: u32, selected: bool) {
        if selected {
            let r = self.selected_outer_circle_radius;
            out.push(Shape::Circle {
                x: x0 - r,
                y: y0 - r,
                diameter: self.selected_outer_circle_diameter,
                color: Color::Rgb(color),
            });
            return;
        }
        let mut x = x0 - self.outer_circle_radius;
        let mut y = y0 - self.outer_circle_radius;
        let mut diameter = self.outer_circle_diameter;
        out.push(Shape::Circle {
            x,
            y,
            diameter,
            color: Color::Rgb(color),
        });
        for fill in [Color::RowBackground, Color::Rgb(color)] {
            diameter -= self.head_delta * 2.0;
            x += self.head_delta;
            y += self.head_delta;
            out.push(Shape::Circle {
                x,
                y,
                diameter,
                color: fill,
            });
        }
    }

    /// `MyPainter.paintEdge`.
    fn edge(
        &self,
        out: &mut Vec<Shape>,
        edge: &ig_log::print::elements::EdgePrintElementImpl,
        color: u32,
        selected: bool,
    ) {
        let down = edge.edge_print_type() == EdgePrintElementType::Down;
        let usual = edge.line_style() == LineStyle::Solid;
        let from = edge.position_in_current_row();
        let to = edge.position_in_other_row();
        let x1 = self.x(from);
        let y1 = self.row_center;
        if from == to {
            let arrow_gap = if edge.is_terminal() {
                self.circle_radius / 2.0 + 1.0
            } else {
                0.0
            };
            let y2 = if down {
                self.row_height - arrow_gap
            } else {
                arrow_gap
            };
            self.line(
                out,
                (x1, y1),
                (x1, y2),
                (x1, y2),
                edge.has_arrow(),
                usual,
                selected,
                color,
            );
        } else {
            // paint non-vertical lines twice the size to make them dock with each other well
            let x2 = self.x(to);
            let y2 = if down {
                self.row_height + self.row_center
            } else {
                self.row_center - self.row_height
            };
            let mid = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
            self.line(
                out,
                (x1, y1),
                (x2, y2),
                mid,
                edge.has_arrow(),
                usual,
                selected,
                color,
            );
        }
    }

    /// `MyPainter.paintLine`.
    #[allow(clippy::too_many_arguments)]
    fn line(
        &self,
        out: &mut Vec<Shape>,
        from: (f32, f32),
        to: (f32, f32),
        arrow_start: (f32, f32),
        has_arrow: bool,
        usual: bool,
        selected: bool,
        color: u32,
    ) {
        let width = if selected {
            self.selected_line_thickness
        } else {
            self.line_thickness
        };
        let dash = if usual || has_arrow {
            None
        } else {
            let length = if from.0 == to.0 {
                self.row_height
            } else {
                (from.0 - to.0).hypot(from.1 - to.1)
            };
            Some(dash_length(length, self.row_height))
        };
        out.push(Shape::Line {
            from,
            to,
            width,
            dash,
            color,
        });
        if has_arrow {
            let cos = ARROW_ANGLE_COS2.sqrt();
            let sin = (1.0 - ARROW_ANGLE_COS2).sqrt();
            let length = ARROW_LENGTH * self.row_height;
            for sin in [sin, -sin] {
                let end = rotate(from, arrow_start, cos, sin, length);
                out.push(Shape::Line {
                    from: arrow_start,
                    to: end,
                    width,
                    dash: None,
                    color,
                });
            }
        }
    }

    /// `getElementUnderCursor`: nodes first, then edges; `(x, y)` relative to the row.
    pub fn element_under(
        &self,
        elements: &[GraphPrintElement],
        x: f32,
        y: f32,
    ) -> Option<GraphPrintElement> {
        let over_node =
            |position: i32| (self.x(position) - x).hypot(self.row_center - y) <= self.circle_radius;
        if let Some(node) = elements.iter().find(
            |e| matches!(e, GraphPrintElement::Node(n) if over_node(n.position_in_current_row())),
        ) {
            return Some(node.clone());
        }
        elements
            .iter()
            .find(|e| match e {
                GraphPrintElement::Edge(edge) => {
                    let x1 = self.x(edge.position_in_current_row());
                    let y1 = self.row_center;
                    let x2 = self.x(edge.position_in_other_row());
                    let y2 = if edge.edge_print_type() == EdgePrintElementType::Down {
                        self.row_height + self.row_center
                    } else {
                        self.row_center - self.row_height
                    };
                    (x1 - x).hypot(y1 - y) + (x2 - x).hypot(y2 - y)
                        < (x1 - x2).hypot(y1 - y2) + self.line_thickness
                }
                GraphPrintElement::Node(_) => false,
            })
            .cloned()
    }
}

/// `SimpleGraphCellPainter.rotate`.
fn rotate(p: (f32, f32), center: (f32, f32), cos: f32, sin: f32, length: f32) -> (f32, f32) {
    let (tx, ty) = (p.0 - center.0, p.1 - center.1);
    let d = tx.hypot(ty);
    let (sx, sy) = (length * tx / d, length * ty / d);
    (
        sx * cos - sy * sin + center.0,
        sx * sin + sy * cos + center.1,
    )
}

/// `SimpleGraphCellPainter.getDashLength`: `(dash, space)`.
fn dash_length(edge_length: f32, row_height: f32) -> (f32, f32) {
    let dash_count = ((edge_length / row_height).floor() as i32).max(1);
    let space = row_height / 2.0 - 2.0;
    (edge_length / dash_count as f32 - space, space)
}

/// Draws `shapes` in `bounds` (one graph cell), clipped to it.
pub fn paint_shapes(
    shapes: &[Shape],
    bounds: Bounds<Pixels>,
    row_background: Hsla,
    window: &mut Window,
    _: &mut App,
) {
    let origin = bounds.origin;
    let at = |(x, y): (f32, f32)| point(origin.x + px(x), origin.y + px(y));
    let rgb = |c: u32| -> Hsla { gpui::rgb(c).into() };
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        for shape in shapes {
            match shape {
                Shape::Line {
                    from,
                    to,
                    width,
                    dash,
                    color,
                } => {
                    let options = StrokeOptions::default()
                        .with_line_width(*width)
                        .with_line_cap(LineCap::Round)
                        .with_line_join(LineJoin::Round);
                    let mut builder =
                        PathBuilder::stroke(px(*width)).with_style(PathStyle::Stroke(options));
                    if let Some((dash, space)) = dash {
                        builder = builder.dash_array(&[px(*dash), px(*space)]);
                    }
                    builder.move_to(at(*from));
                    builder.line_to(at(*to));
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, rgb(*color));
                    }
                }
                Shape::Circle {
                    x,
                    y,
                    diameter,
                    color,
                } => {
                    let fill = match color {
                        Color::Rgb(c) => rgb(*c),
                        Color::RowBackground => row_background,
                    };
                    let b = Bounds::new(at((*x, *y)), gpui::size(px(*diameter), px(*diameter)));
                    window.paint_quad(quad(
                        b,
                        px(diameter / 2.0),
                        fill,
                        px(0.0),
                        transparent_black(),
                        Default::default(),
                    ));
                }
                Shape::Ring {
                    x,
                    y,
                    diameter,
                    width,
                    color,
                } => {
                    let b = Bounds::new(at((*x, *y)), gpui::size(px(*diameter), px(*diameter)));
                    window.paint_quad(quad(
                        b,
                        px(diameter / 2.0),
                        transparent_black(),
                        px(*width),
                        rgb(*color),
                        Default::default(),
                    ));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_to_int_matches_paint_util() {
        // PaintUtil javadoc: 2.1 floor'd to odd -> 1
        assert_eq!(
            align_to_int(2.1, 1.0, Rounding::Floor, Some(Parity::Odd)),
            1.0
        );
        assert_eq!(
            align_to_int(1.9, 1.0, Rounding::Floor, Some(Parity::Odd)),
            1.0
        );
        // at 2x: 1.5 -> 3 device px (odd) -> 1.5
        assert_eq!(
            align_to_int(1.5, 2.0, Rounding::Floor, Some(Parity::Odd)),
            1.5
        );
        assert_eq!(align_to_int(13.0, 2.0, Rounding::Floor, None), 13.0);
    }

    #[test]
    fn metrics_at_the_new_ui_row_height() {
        // row height 26 at 2x: everything scales by 26/22
        let g = GraphGeometry::new(26.0, 2.0);
        assert_eq!(g.row_center, 12.5); // 26 dev -> odd 25 -> 12.5
        assert_eq!(g.element_width, 18.5); // 18.909 * 2 = 37.8 -> 37
        assert_eq!(g.element_center, 9.0); // 9.4545 * 2 = 18.9 -> 18
        assert_eq!(g.line_thickness, 1.5); // 1.77 * 2 = 3.5 -> 3
        assert_eq!(g.selected_line_thickness, 2.5); // 2.95 * 2 = 5.9 -> 5
        assert_eq!(g.circle_diameter, 8.5); // 9.45 * 2 = 18.9 -> 17 (odd)
        assert_eq!(g.circle_radius, 4.0); // 4.25 * 2 = 8.5 -> 8
        assert_eq!(g.selected_circle_radius, 4.5); // (8.5 + 2.5 - 1.5) / 2 = 4.75 -> 9.5 dev -> 9
    }

    #[test]
    fn dashes_fit_the_edge() {
        let (dash, space) = dash_length(26.0, 26.0);
        assert_eq!(space, 11.0);
        assert_eq!(dash + space, 26.0);
        let (dash, space) = dash_length(60.0, 26.0);
        assert_eq!(2.0 * (dash + space), 60.0);
    }

    #[test]
    fn arrow_head_is_rotated_around_the_arrow_start() {
        // a vertical edge pointing down: the arrow wings go up-left and up-right
        let end = rotate((0.0, 0.0), (0.0, 10.0), 0.7f32.sqrt(), 0.3f32.sqrt(), 5.0);
        assert!((end.0 - 0.0).abs() > 1.0 && end.1 < 10.0);
        assert!(((end.0).hypot(end.1 - 10.0) - 5.0).abs() < 1e-4);
    }
}
```

`crates/ig-app/src/log/labels.rs`:

```rust
//! Reference labels at the right of the Subject cell.
//!
//! Port of `platform/vcs-log/impl/src/com/intellij/vcs/log/ui/render/LabelPainter.java` (New UI
//! path: no grey background, icon-text padding), `LabelIcon.java` (`paintTagsForNewUi`, widths)
//! and `TagPainter.kt` (the tag outline from `expui/vcs/currentBranch.svg`), plus the
//! `GraphCommitCellRenderer.getAvailableWidth` rule.
//!
//! [`layout`] is pure (text widths come from a measuring function); [`paint`] draws with GPUI.
//!
//! Deviations: no bookmarks; string lengths count chars, not UTF-16 units; the detached-HEAD
//! warning icon is drawn as a single-color mask (GPUI `paint_svg`) at the label height.

use gpui::{
    App, Bounds, Font, Hsla, PathBuilder, PathStyle, Pixels, Point, SharedString, StrokeOptions,
    TextAlign, TextRun, Window, point, px,
};
use ig_ui_kit::metrics::label::{
    COMPACT_MIDDLE_PADDING, DISPLAYED_MESSAGE_PART, ICON_ADDITIONAL_PADDING, ICON_SIZE_UNITS,
    ICON_TEXT_PADDING, LEFT_PADDING, MAX_LENGTH, MIDDLE_PADDING, RIGHT_PADDING,
};

use super::ref_groups::{RefColor, RefGroup};

const TWO_DOTS: &str = "..";
const SEPARATOR: &str = "/";
const ELLIPSIS: &str = "\u{2026}";

/// One painted label: an icon (tag shapes or the warning icon) and text.
#[derive(Clone, Debug, PartialEq)]
pub struct Presentation {
    pub text: String,
    pub icon: LabelIcon,
    pub width: f32,
    pub icon_padding: f32,
    /// The detached-HEAD warning text color instead of the label foreground.
    pub warning: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LabelIcon {
    /// Overlapping tag shapes, one per color.
    Tags(Vec<RefColor>),
    Warning,
}

impl LabelIcon {
    fn width(&self, height: f32) -> f32 {
        match self {
            LabelIcon::Tags(colors) => tags_width(height, colors.len()),
            LabelIcon::Warning => height,
        }
    }
}

/// `LabelIcon.getWidth(height, labelsCount)`.
pub fn tags_width(height: f32, count: usize) -> f32 {
    let scale = height / ICON_SIZE_UNITS;
    ((ICON_SIZE_UNITS + 2.0 * (count as f32 - 1.0)) * scale).round()
}

/// The labels of a row and their total width (0 when none).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Labels {
    pub list: Vec<Presentation>,
    pub width: f32,
    pub compact: bool,
}

/// `GraphCommitCellRenderer.getAvailableWidth`: the space labels may take in a Subject cell of
/// `column_width` with `graph_width` of graph and a subject `text_width` wide.
pub fn available_width(column_width: f32, graph_width: f32, text_width: f32, compact: bool) -> f32 {
    let text_and_labels = column_width - graph_width;
    let free = text_and_labels - text_width;
    let allowed = if compact {
        free.min(text_and_labels / 3.0)
    } else {
        free.max((text_and_labels / 2.0).max(text_and_labels - DISPLAYED_MESSAGE_PART))
    };
    allowed.max(0.0).trunc()
}

/// `calculatePresentation` (right-aligned labels, no bookmarks). `height` is the reference font's
/// height (`fontMetrics.getHeight()`), `measure` its `stringWidth`.
pub fn layout(
    groups: &[RefGroup],
    available: f32,
    height: f32,
    compact: bool,
    measure: &dyn Fn(&str) -> f32,
) -> Labels {
    if groups.is_empty() {
        return Labels {
            compact,
            ..Labels::default()
        };
    }
    let side = LEFT_PADDING + RIGHT_PADDING;
    let middle = if compact {
        COMPACT_MIDDLE_PADDING
    } else {
        MIDDLE_PADDING
    };
    let detached = groups.first().filter(|g| g.detached_head).map(|g| {
        presentation(
            &g.name,
            LabelIcon::Warning,
            height,
            ICON_ADDITIONAL_PADDING,
            true,
            measure,
        )
    });
    let detached_width = detached.as_ref().map_or(0.0, |d| d.width + middle);
    let remaining = available - detached_width - side;
    let mut list: Vec<Presentation> = detached.into_iter().collect();
    list.extend(reference_labels(
        groups, remaining, height, compact, measure,
    ));
    let width = if list.is_empty() {
        0.0
    } else {
        list.iter().map(|p| p.width).sum::<f32>() + middle * (list.len() - 1) as f32 + side
    };
    Labels {
        list,
        width,
        compact,
    }
}

fn text_width(text: &str, measure: &dyn Fn(&str) -> f32) -> f32 {
    let w = measure(text);
    if w > 0.0 { ICON_TEXT_PADDING + w } else { 0.0 }
}

fn presentation(
    text: &str,
    icon: LabelIcon,
    height: f32,
    icon_padding: f32,
    warning: bool,
    measure: &dyn Fn(&str) -> f32,
) -> Presentation {
    let width = icon.width(height) + icon_padding + text_width(text, measure);
    Presentation {
        text: text.to_string(),
        icon,
        width,
        icon_padding,
        warning,
    }
}

/// `LabelPainter.getColors(groups)`: colors in order of first use, twice when used more than once.
fn merged_colors(groups: &[RefGroup]) -> Vec<RefColor> {
    let mut used: Vec<(RefColor, usize)> = Vec::new();
    for color in groups.iter().flat_map(|g| g.colors()) {
        match used.iter_mut().find(|(c, _)| *c == color) {
            Some((_, n)) => *n += 1,
            None => used.push((color, 1)),
        }
    }
    used.into_iter()
        .flat_map(|(c, n)| std::iter::repeat_n(c, if n > 1 { 2 } else { 1 }))
        .collect()
}

/// `processReferenceLabels`.
fn reference_labels(
    groups: &[RefGroup],
    available: f32,
    height: f32,
    compact: bool,
    measure: &dyn Fn(&str) -> f32,
) -> Vec<Presentation> {
    let mut labels = Vec::new();
    let mut references_width = 0.0;
    let middle = if compact {
        COMPACT_MIDDLE_PADDING
    } else {
        MIDDLE_PADDING
    };
    for (i, group) in groups.iter().enumerate() {
        if group.detached_head {
            continue;
        }
        let do_not_fit = if !compact && i < groups.len() - 1 {
            tags_width(height, merged_colors(&groups[i + 1..]).len())
        } else {
            0.0
        };
        let icon = LabelIcon::Tags(group.colors());
        let is_last = i == groups.len() - 1;
        let mut new_width =
            references_width + icon.width(height) + if is_last { 0.0 } else { middle };
        let text = shorten_ref_name(&group.name, available - new_width - do_not_fit, measure);
        new_width += text_width(&text, measure);
        if !compact && available - new_width - do_not_fit < 0.0 {
            let last_icon = LabelIcon::Tags(merged_colors(&groups[i..]));
            let name = if labels.is_empty() {
                text
            } else {
                String::new()
            };
            labels.push(presentation(&name, last_icon, height, 0.0, false, measure));
            break;
        }
        labels.push(presentation(&text, icon, height, 0.0, false, measure));
        references_width = new_width;
    }
    labels
}

/// `StringUtil.shortenTextWithEllipsis(text, maxLength, 0, symbol)`.
fn shorten_with_ellipsis(text: &str, max_length: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_length {
        return text.to_string();
    }
    let prefix = max_length.saturating_sub(ELLIPSIS.chars().count());
    chars[..prefix].iter().collect::<String>() + ELLIPSIS
}

/// `LabelPainter.shortenRefName` with `VcsLogUiUtil.shortenTextToFit`.
fn shorten_ref_name(name: &str, available: f32, measure: &dyn Fn(&str) -> f32) -> String {
    if measure(name) > available && name.chars().count() > MAX_LENGTH {
        let mut name = name.to_string();
        if let Some(separator) = name.find(SEPARATOR)
            && separator > TWO_DOTS.len()
        {
            name = format!("{TWO_DOTS}{}", &name[separator..]);
        }
        if available > 0.0 {
            if measure(&name) <= available {
                return name;
            }
            for length in (MAX_LENGTH + 1..=name.chars().count()).rev() {
                let shortened = shorten_with_ellipsis(&name, length);
                if measure(&shortened) <= available {
                    return shortened;
                }
            }
        }
        return shorten_with_ellipsis(&name, MAX_LENGTH);
    }
    name.to_string()
}

/// How labels are painted.
pub struct LabelStyle {
    pub font: Font,
    pub font_size: Pixels,
    /// The font's line height (`fontMetrics.getHeight()`).
    pub height: f32,
    pub foreground: Hsla,
    pub warning_foreground: Hsla,
    pub background: Hsla,
    pub color_of: fn(RefColor, &App) -> Hsla,
    /// Asset path of the warning icon and the color it is drawn with.
    pub warning_icon: SharedString,
    pub warning_icon_color: Hsla,
}

/// `LabelPainter.paint(g2, x, y, height)`: `bounds` is the label area of the row.
pub fn paint(
    labels: &Labels,
    style: &LabelStyle,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    if labels.list.is_empty() {
        return;
    }
    // opaque: hides subject text running under the labels
    window.paint_quad(gpui::fill(bounds, style.background));
    let row_height = bounds.size.height;
    let mut x = bounds.origin.x + px(LEFT_PADDING);
    let middle = if labels.compact {
        COMPACT_MIDDLE_PADDING
    } else {
        MIDDLE_PADDING
    };
    for label in &labels.list {
        let icon_width = label.icon.width(style.height);
        let icon_y = bounds.origin.y + (row_height - px(style.height)) / 2.0;
        match &label.icon {
            LabelIcon::Tags(colors) => {
                let colors: Vec<Hsla> = colors.iter().map(|c| (style.color_of)(*c, cx)).collect();
                paint_tags(
                    point(x, icon_y),
                    style.height,
                    style.background,
                    &colors,
                    window,
                );
            }
            LabelIcon::Warning => {
                let icon = Bounds::new(
                    point(x, icon_y),
                    gpui::size(px(style.height), px(style.height)),
                );
                let _ = window.paint_svg(
                    icon,
                    style.warning_icon.clone(),
                    None,
                    gpui::TransformationMatrix::unit(),
                    style.warning_icon_color,
                    cx,
                );
            }
        }
        x += px(icon_width + ICON_TEXT_PADDING + label.icon_padding);
        if !label.text.is_empty() {
            let color = if label.warning {
                style.warning_foreground
            } else {
                style.foreground
            };
            let run = TextRun {
                len: label.text.len(),
                font: style.font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window.text_system().shape_line(
                label.text.clone().into(),
                style.font_size,
                &[run],
                None,
            );
            let width = line.width();
            let _ = line.paint(
                point(x, bounds.origin.y),
                row_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
            x += width;
        }
        x += px(middle);
    }
}

/// `LabelIcon.paintTagsForNewUi`: tags left to right, the last color drawn first.
fn paint_tags(
    origin: Point<Pixels>,
    size: f32,
    background: Hsla,
    colors: &[Hsla],
    window: &mut Window,
) {
    let scale = size / ICON_SIZE_UNITS;
    let count = colors.len();
    for i in 0..count {
        let color = colors[count - 1 - i];
        paint_tag(
            window,
            origin,
            scale * 2.0 * i as f32,
            i == count - 1,
            background,
            color,
            size,
        );
        if i != count - 1 {
            let x0 = scale * 2.0 * i as f32 + 2.0 * scale - 2f32.sqrt();
            paint_tag(window, origin, x0, false, background, background, size);
        }
    }
}

/// `TagPainter.paintTag`: the tag outline of `expui/vcs/currentBranch.svg` (a 16x16 design).
fn paint_tag(
    window: &mut Window,
    origin: Point<Pixels>,
    offset: f32,
    with_dot: bool,
    background: Hsla,
    color: Hsla,
    size: f32,
) {
    let s = size / 16.0;
    let p = |x: f32, y: f32| point(origin.x + px(x * s + offset), origin.y + px(y * s));
    let outline = |mut b: PathBuilder| {
        b.move_to(p(13.4495, 7.5001));
        b.line_to(p(7.01028, 13.9394));
        b.cubic_bezier_to(
            p(6.30317, 13.9394),
            p(6.81502, 14.1347),
            p(6.49843, 14.1347),
        );
        b.line_to(p(2.06053, 9.69679));
        b.cubic_bezier_to(
            p(2.06053, 8.98968),
            p(1.86527, 9.50152),
            p(1.86527, 9.18494),
        );
        b.line_to(p(8.49978, 2.55035));
        b.cubic_bezier_to(
            p(8.92404, 2.40893),
            p(8.61106, 2.43907),
            p(8.76824, 2.38667),
        );
        b.line_to(p(12.6363, 2.93926));
        b.cubic_bezier_to(
            p(13.0606, 3.36352),
            p(12.8563, 2.97069),
            p(13.0292, 3.14354),
        );
        b.line_to(p(13.5909, 7.07584));
        b.cubic_bezier_to(p(13.4495, 7.5001), p(13.6132, 7.23163), p(13.5608, 7.38882));
        b.close();
        b
    };
    if let Ok(path) = outline(PathBuilder::fill()).build() {
        window.paint_path(path, background);
    }
    let stroke = StrokeOptions::default()
        .with_line_width(s)
        .with_line_cap(lyon::tessellation::LineCap::Round)
        .with_line_join(lyon::tessellation::LineJoin::Round);
    if let Ok(path) =
        outline(PathBuilder::stroke(px(s)).with_style(PathStyle::Stroke(stroke))).build()
    {
        window.paint_path(path, color);
    }
    if with_dot {
        // oval(offset, scale, r = 1, center (10.5, 5.5))
        let center = p(10.5, 5.5);
        let r = px(s);
        window.paint_quad(gpui::quad(
            Bounds::new(
                point(center.x - r, center.y - r),
                gpui::size(r * 2.0, r * 2.0),
            ),
            r,
            color,
            px(0.0),
            gpui::transparent_black(),
            Default::default(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use ig_git::log::ref_manager::{LOCAL_BRANCH, TAG};
    use ig_log::data::{Hash, VcsRef, vcs_root};

    use super::*;

    fn group(name: &str, n: usize) -> RefGroup {
        let refs = (0..n)
            .map(|i| {
                VcsRef::new(
                    Hash::build("ab"),
                    &format!("{name}{i}"),
                    if i == 0 { LOCAL_BRANCH } else { TAG },
                    vcs_root("/r"),
                )
            })
            .collect();
        RefGroup {
            name: name.to_string(),
            refs,
            detached_head: false,
        }
    }

    /// Every char is 7px wide.
    fn measure(s: &str) -> f32 {
        7.0 * s.chars().count() as f32
    }

    #[test]
    fn icon_width_grows_by_two_units_per_extra_tag() {
        assert_eq!(tags_width(15.0, 1), 15.0);
        assert_eq!(tags_width(15.0, 2), 20.0); // (6.25 + 2) * 2.4 = 19.8
        assert_eq!(tags_width(15.0, 3), 25.0); // 10.25 * 2.4 = 24.6
    }

    #[test]
    fn single_label_width_includes_paddings() {
        let labels = layout(&[group("main", 1)], 500.0, 15.0, true, &measure);
        assert_eq!(labels.list.len(), 1);
        // icon 15 + text padding 1 + 28, plus left/right paddings 4 + 4
        assert_eq!(labels.list[0].width, 44.0);
        assert_eq!(labels.width, 52.0);
    }

    #[test]
    fn labels_that_do_not_fit_collapse_into_one_icon() {
        let groups = [group("first", 1), group("second", 1), group("third", 1)];
        let labels = layout(&groups, 90.0, 15.0, false, &measure);
        assert_eq!(labels.list.len(), 1);
        assert_eq!(labels.list[0].text, "first");
        assert_eq!(
            labels.list[0].icon,
            LabelIcon::Tags(vec![RefColor::LocalBranch, RefColor::LocalBranch])
        );
    }

    #[test]
    fn long_names_are_shortened_from_the_separator() {
        let name = "feature/a-very-long-branch-name-indeed";
        assert_eq!(shorten_ref_name(name, 1000.0, &measure), name);
        let short = shorten_ref_name(name, 120.0, &measure);
        assert!(
            short.starts_with("../a-very") && short.ends_with(ELLIPSIS),
            "{short}"
        );
        assert_eq!(short.chars().count(), MAX_LENGTH);
        assert_eq!(
            shorten_ref_name(name, 0.0, &measure).chars().count(),
            MAX_LENGTH
        );
    }

    #[test]
    fn available_width_follows_the_renderer() {
        // compact: at most a third of the text area
        assert_eq!(available_width(600.0, 60.0, 100.0, true), 180.0);
        // non-compact: at least half, or all but 80px of the text
        assert_eq!(available_width(600.0, 60.0, 500.0, false), 460.0);
    }
}
```

- [ ] **Step 2: `crates/ig-app/src/log/mod.rs`**, helpers only. Write exactly:

```rust
//! The read-only Git Log: the ported painters ([`graph_painter`], [`labels`]) and their helpers.
//! Loading and the table land in the next tasks.

pub mod date;
pub mod graph_painter;
pub mod labels;
pub mod metadata;
pub mod ref_groups;
pub mod selection;
```

- [ ] **Step 3: `crates/ig-app/src/lib.rs`** adds `pub mod log;`. Write exactly:

```rust
//! IdeaGit's application: windows, actions, the Git tool window (Log and Console) and the log
//! worker. Built on `ig-git` / `ig-log` for Git and `ig-ui-kit` for the IntelliJ New UI look.
//!
//! Every module names the upstream sources it ports or imitates; see also
//! `reference/upstream-map.toml`.

pub mod actions;
pub mod console;
pub mod log;
pub mod logging;
pub mod paths;
pub mod settings;
pub mod strings;
```

- [ ] **Step 4: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code origin/wip/app-shell-prototype -- crates/ig-app/src/log/date.rs crates/ig-app/src/log/ref_groups.rs crates/ig-app/src/log/selection.rs crates/ig-app/src/log/metadata.rs crates/ig-app/src/log/graph_painter.rs crates/ig-app/src/log/labels.rs
```

Expected: no output. The intermediate files written above differ from the branch until their final task.

- [ ] **Step 5: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 455 passed, 1 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(ig-app): graph painter, label painter, ref groups, relative dates, selection, metadata cache"
```

---

### Task 8: `ig-app`: log worker, model and headless UI tests

**Files:**
- Create: `crates/ig-app/src/log/worker.rs`, `crates/ig-app/src/log/model.rs`, `crates/ig-app/src/log/mod.rs`, `crates/ig-app/tests/ui.rs`

**Review focus:** threading in `worker.rs` (one thread, coalesced refreshes, panic catch and restart, data-pack handler never re-enters the refresher); metadata loading per visible range; `model.rs` re-fetches print elements after `is_repaint_required`.

**Never glob-import `gpui` in test modules** (`use gpui::*;`): `#[gpui::test]` then expands recursively and crashes rustc. Import the names you need.

- [ ] **Step 1: The modules.**

```bash
git checkout origin/wip/app-shell-prototype -- crates/ig-app/src/log/worker.rs crates/ig-app/src/log/model.rs
```

`crates/ig-app/src/log/worker.rs`:

```rust
//! The log worker of a project window: loads the log in the background and sends immutable
//! snapshots and row metadata to the UI.
//!
//! Follows the threading notes of Plan 2c (`docs/superpowers/plans/2026-11-08-plan-2c-git-log-data.md`)
//! and upstream's `VcsLogRefresherImpl` / `MiniDetailsGetter` split:
//! - The **refresher thread** owns the `VcsLogRefresher`. It runs `initialize()`, then serves
//!   refresh requests, coalescing those queued meanwhile into one `repository_manager.update()` +
//!   `refresh(roots)` (upstream batches requests in its refresh queue).
//! - The data-pack handler and the recent-details consumer only forward to the **snapshot
//!   thread** over a channel; they never call back into the refresher (re-entrancy rule).
//! - The snapshot thread builds the visible graph of the newest pack (`Options::DEFAULT`), loads
//!   the metadata of its first rows, and sends a [`LogSnapshot`]. It also serves the UI's metadata
//!   requests in batches of [`BATCH_SIZE`], newest request first, keeping the newest
//!   [`MAX_LOADING_TASKS`] (upstream `SequentialLimitedLifoExecutor`).
//! - A panic on either thread is caught at the thread boundary, logged and reported as
//!   [`LogEvent::WorkerFailed`]; [`LogWorker::refresh`] restarts a dead worker.
//!
//! Deviation from the app-shell spec ("one dedicated thread"): two threads, because the refresher
//! is synchronous and the first block must be shown while `initialize()` still loads the full
//! history, and metadata must load while a refresh runs.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use ig_git::log::provider::GitLogProvider;
use ig_git::log::repository_manager::GitRepositoryManager;
use ig_git::runner::Git;
use ig_log::data::{
    InMemoryStorage, LogProviders, VcsCommitMetadata, VcsLogCommitStorageIndex, VcsLogGraphData,
    VcsLogProvider, VcsLogRefresher, VcsLogStorage, VcsRoot, vcs_root,
};
use ig_log::{GraphCommit, Options, PermanentGraph, VisibleGraph, VisibleGraphImpl};

use super::metadata::{BATCH_SIZE, MAX_LOADING_TASKS, RowMeta};

/// `vcs.log.recent.commits.count`.
pub const RECENT_COMMITS_COUNT: usize = 1000;
/// Rows whose metadata comes with a snapshot (about two screens).
pub const FIRST_ROWS: i32 = 100;

/// An immutable view of a data pack, built on the snapshot thread and moved to the UI.
pub struct LogSnapshot {
    /// Increases with every snapshot of a worker.
    pub generation: u64,
    pub data: Arc<VcsLogGraphData>,
    /// `None` for an empty or failed pack.
    pub visible: Option<VisibleGraphImpl<VcsLogCommitStorageIndex>>,
    pub recommended_width: i32,
    /// Each root's current branch and remotes, as read when the snapshot was built.
    pub repos: Vec<RepoSummary>,
}

/// The repository state the Log UI needs (status bar branch, label grouping).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoSummary {
    pub root: VcsRoot,
    /// `None` when HEAD is detached or there are no commits.
    pub current_branch: Option<String>,
    pub remotes: Vec<String>,
}

impl LogSnapshot {
    pub fn repo(&self, root: &VcsRoot) -> Option<&RepoSummary> {
        self.repos.iter().find(|r| &r.root == root)
    }
}

impl LogSnapshot {
    pub fn row_count(&self) -> i32 {
        self.visible
            .as_ref()
            .map_or(0, |v| v.visible_commit_count())
    }

    pub fn error(&self) -> Option<&str> {
        self.data.load_error().map(|e| e.message())
    }
}

/// What the worker tells the UI.
pub enum LogEvent {
    Snapshot(Box<LogSnapshot>),
    Metadata {
        loaded: Vec<(VcsLogCommitStorageIndex, RowMeta)>,
        failed: Vec<VcsLogCommitStorageIndex>,
    },
    /// A refresh (or the initial load) started / finished: shown as status bar progress.
    Busy(bool),
    /// A worker thread panicked; the message is logged.
    WorkerFailed(String),
}

enum RefreshCommand {
    Refresh,
}

enum SnapshotCommand {
    Pack(Arc<VcsLogGraphData>),
    Recent(VcsRoot, Vec<VcsCommitMetadata>),
    Load(Vec<VcsLogCommitStorageIndex>),
}

/// Sends events to the UI; returns `false` once the UI is gone.
pub type EventSink = Arc<dyn Fn(LogEvent) -> bool + Send + Sync>;

/// The UI's handle on the worker threads.
pub struct LogWorker {
    git: Arc<Git>,
    roots: Vec<PathBuf>,
    sink: EventSink,
    refresh_tx: Sender<RefreshCommand>,
    snapshot_tx: Sender<SnapshotCommand>,
    threads: Vec<JoinHandle<()>>,
}

impl LogWorker {
    /// Starts loading the log of `roots` (`initialize()`).
    pub fn start(git: Arc<Git>, roots: Vec<PathBuf>, sink: EventSink) -> LogWorker {
        let (refresh_tx, refresh_rx) = channel();
        let (snapshot_tx, snapshot_rx) = channel();
        let threads = spawn_threads(
            git.clone(),
            &roots,
            sink.clone(),
            refresh_rx,
            snapshot_tx.clone(),
            snapshot_rx,
        );
        LogWorker {
            git,
            roots,
            sink,
            refresh_tx,
            snapshot_tx,
            threads,
        }
    }

    fn is_alive(&self) -> bool {
        self.threads.iter().all(|t| !t.is_finished())
    }

    /// Requests a refresh of all roots; restarts the worker if a thread died.
    pub fn refresh(&mut self) {
        if !self.is_alive() {
            log::info!("restarting the log worker");
            *self = LogWorker::start(self.git.clone(), self.roots.clone(), self.sink.clone());
            return;
        }
        let _ = self.refresh_tx.send(RefreshCommand::Refresh);
    }

    /// Requests the metadata of `commits` (already marked loading in the UI cache).
    pub fn load_metadata(&self, commits: Vec<VcsLogCommitStorageIndex>) {
        if !commits.is_empty() {
            let _ = self.snapshot_tx.send(SnapshotCommand::Load(commits));
        }
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

fn guarded(
    name: &'static str,
    sink: EventSink,
    body: impl FnOnce() + Send + 'static,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            if let Err(payload) = catch_unwind(AssertUnwindSafe(body)) {
                let message = format!("{name} panicked: {}", panic_message(payload.as_ref()));
                log::error!("{message}");
                sink(LogEvent::Busy(false));
                sink(LogEvent::WorkerFailed(message));
            }
        })
        .expect("spawning a log worker thread")
}

fn spawn_threads(
    git: Arc<Git>,
    roots: &[PathBuf],
    sink: EventSink,
    refresh_rx: Receiver<RefreshCommand>,
    snapshot_tx: Sender<SnapshotCommand>,
    snapshot_rx: Receiver<SnapshotCommand>,
) -> Vec<JoinHandle<()>> {
    let repository_manager = Arc::new(GitRepositoryManager::new(git));
    let provider = Arc::new(GitLogProvider::with_repository_manager(
        repository_manager.clone(),
    ));
    let roots: Vec<VcsRoot> = roots.iter().map(vcs_root).collect();
    let storage = Arc::new(InMemoryStorage::new());

    let snapshot_thread = {
        let sink = sink.clone();
        let storage: Arc<dyn VcsLogStorage> = storage.clone();
        let repository_manager = repository_manager.clone();
        guarded("ig-log-snapshot", sink.clone(), move || {
            SnapshotThread::new(sink, storage, repository_manager).run(snapshot_rx)
        })
    };
    let refresh_thread = guarded("ig-log-refresh", sink.clone(), move || {
        let mut providers = LogProviders::new();
        for root in &roots {
            providers.insert(root.clone(), provider.clone() as Arc<dyn VcsLogProvider>);
        }
        let mut refresher = VcsLogRefresher::new(storage, providers, RECENT_COMMITS_COUNT);
        let tx = snapshot_tx.clone();
        refresher.set_data_pack_handler(move |pack| {
            let _ = tx.send(SnapshotCommand::Pack(pack.clone()));
        });
        let tx = snapshot_tx;
        refresher.set_recent_details_consumer(move |root, details| {
            let _ = tx.send(SnapshotCommand::Recent(root.clone(), details.to_vec()));
        });
        sink(LogEvent::Busy(true));
        refresher.initialize();
        sink(LogEvent::Busy(false));
        while let Ok(RefreshCommand::Refresh) = refresh_rx.recv() {
            // coalesce the requests queued meanwhile
            while refresh_rx.try_recv().is_ok() {}
            sink(LogEvent::Busy(true));
            for root in &roots {
                repository_manager.update(root);
            }
            refresher.refresh(&roots);
            if !sink(LogEvent::Busy(false)) {
                break;
            }
        }
    });
    vec![refresh_thread, snapshot_thread]
}

struct SnapshotThread {
    sink: EventSink,
    storage: Arc<dyn VcsLogStorage>,
    repository_manager: Arc<GitRepositoryManager>,
    generation: u64,
    current: Option<Arc<VcsLogGraphData>>,
    /// Commits whose metadata the UI has (or was sent), so first rows aren't loaded twice.
    known: HashSet<VcsLogCommitStorageIndex>,
}

impl SnapshotThread {
    fn new(
        sink: EventSink,
        storage: Arc<dyn VcsLogStorage>,
        repository_manager: Arc<GitRepositoryManager>,
    ) -> SnapshotThread {
        SnapshotThread {
            sink,
            storage,
            repository_manager,
            generation: 0,
            current: None,
            known: HashSet::new(),
        }
    }

    fn run(mut self, rx: Receiver<SnapshotCommand>) {
        let mut loads: VecDeque<Vec<VcsLogCommitStorageIndex>> = VecDeque::new();
        loop {
            // block for one command, then take everything queued
            let first = if loads.is_empty() {
                match rx.recv() {
                    Ok(command) => Some(command),
                    Err(_) => return,
                }
            } else {
                None
            };
            let mut newest_pack = None;
            for command in first.into_iter().chain(rx.try_iter()) {
                match command {
                    SnapshotCommand::Pack(pack) => newest_pack = Some(pack),
                    SnapshotCommand::Recent(root, details) => self.recent(&root, &details),
                    SnapshotCommand::Load(commits) => {
                        loads.push_back(commits);
                        // LIFO, limited: drop the oldest requests
                        while loads.len() > MAX_LOADING_TASKS as usize {
                            loads.pop_front();
                        }
                    }
                }
            }
            if let Some(pack) = newest_pack
                && !self.snapshot(pack)
            {
                return;
            }
            if let Some(commits) = loads.pop_back()
                && !self.load(&commits)
            {
                return;
            }
        }
    }

    fn recent(&mut self, root: &VcsRoot, details: &[VcsCommitMetadata]) {
        let loaded: Vec<_> = details
            .iter()
            .map(|d| {
                (
                    self.storage.get_commit_index(d.id(), root),
                    RowMeta::from_metadata(d),
                )
            })
            .collect();
        self.known.extend(loaded.iter().map(|(i, _)| *i));
        (self.sink)(LogEvent::Metadata {
            loaded,
            failed: Vec::new(),
        });
    }

    fn snapshot(&mut self, data: Arc<VcsLogGraphData>) -> bool {
        self.generation += 1;
        let visible = data
            .permanent_graph()
            .and_then(|g| g.create_visible_graph(Options::DEFAULT, None, None).ok());
        let recommended_width = visible.as_ref().map_or(0, |v| v.recommended_width());
        if let Some(visible) = &visible {
            let first: Vec<_> = (0..visible.visible_commit_count().min(FIRST_ROWS))
                .map(|row| visible.get_row_info(row).commit())
                .filter(|c| !self.known.contains(c))
                .collect();
            self.current = Some(data.clone());
            if !self.load(&first) {
                return false;
            }
        }
        let repos = data
            .log_providers()
            .keys()
            .map(|root| {
                let repo = self.repository_manager.get_repository(root);
                RepoSummary {
                    root: root.clone(),
                    current_branch: repo.current_branch_name().map(str::to_string),
                    remotes: repo.remotes.iter().map(|r| r.name.clone()).collect(),
                }
            })
            .collect();
        self.current = Some(data.clone());
        (self.sink)(LogEvent::Snapshot(Box::new(LogSnapshot {
            generation: self.generation,
            data,
            visible,
            recommended_width,
            repos,
        })))
    }

    /// Reads the metadata of `commits` by root, in batches; reports loaded and failed commits.
    fn load(&mut self, commits: &[VcsLogCommitStorageIndex]) -> bool {
        let Some(data) = self.current.clone() else {
            return (self.sink)(LogEvent::Metadata {
                loaded: Vec::new(),
                failed: commits.to_vec(),
            });
        };
        let mut by_root: BTreeMap<VcsRoot, Vec<(String, VcsLogCommitStorageIndex)>> =
            BTreeMap::new();
        let mut failed = Vec::new();
        for &commit in commits {
            match self.storage.get_commit_id(commit) {
                Some(id) => by_root
                    .entry(id.root)
                    .or_default()
                    .push((id.hash.as_string(), commit)),
                None => failed.push(commit),
            }
        }
        let mut loaded = Vec::with_capacity(commits.len());
        for (root, hashes) in by_root {
            let Some(provider) = data.log_providers().get(&root) else {
                failed.extend(hashes.iter().map(|(_, c)| *c));
                continue;
            };
            for batch in hashes.chunks(BATCH_SIZE) {
                let index: HashMap<&str, VcsLogCommitStorageIndex> =
                    batch.iter().map(|(h, c)| (h.as_str(), *c)).collect();
                let names: Vec<String> = batch.iter().map(|(h, _)| h.clone()).collect();
                let mut found = Vec::with_capacity(batch.len());
                let result = provider.read_metadata(&root, &names, &mut |m| {
                    if let Some(&commit) = index.get(m.id().as_string().as_str()) {
                        found.push((commit, RowMeta::from_metadata(&m)));
                    }
                });
                match result {
                    Ok(()) => {
                        let got: HashSet<_> = found.iter().map(|(c, _)| *c).collect();
                        failed.extend(batch.iter().map(|(_, c)| *c).filter(|c| !got.contains(c)));
                        loaded.extend(found);
                    }
                    Err(e) => {
                        log::warn!("reading metadata in {}: {}", root.display(), e.message());
                        failed.extend(batch.iter().map(|(_, c)| *c));
                    }
                }
            }
        }
        self.known.extend(loaded.iter().map(|(c, _)| *c));
        (self.sink)(LogEvent::Metadata { loaded, failed })
    }
}
```

`crates/ig-app/src/log/model.rs`:

```rust
//! The Log tab's state on the UI thread: the newest snapshot, the metadata cache, the selection
//! and the graph's mouse actions.
//!
//! Graph actions follow `platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/GraphCommitCellController.java`
//! and `VcsLogGraphTable.handleAnswer`: hovering an element asks the `ActionController` (it
//! highlights the edge and sets the hand cursor); clicking an arrow jumps to the commit it points
//! to (select + scroll). Print elements are snapshots, so rows are re-read on every repaint.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{Context, EventEmitter, ScrollStrategy, UniformListScrollHandle};
use ig_git::runner::Git;
use ig_log::actions::{Cursor, GraphAction, GraphActionType};
use ig_log::data::VcsLogCommitStorageIndex;
use ig_log::{GraphPrintElement, VisibleGraph};

use super::graph_painter::GraphGeometry;
use super::metadata::{Lookup, MetadataCache};
use super::selection::{Rows, Selection};
use super::worker::{EventSink, LogEvent, LogSnapshot, LogWorker};

/// `GraphTableModel.UP_PRELOAD_COUNT` / `DOWN_PRELOAD_COUNT`.
const UP_PRELOAD_COUNT: usize = 20;
const DOWN_PRELOAD_COUNT: usize = 40;

/// What the Log tab shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogState {
    NoRoots,
    Loading,
    Ready,
    Error(String),
}

/// Events for the project window.
pub enum LogModelEvent {
    WorkerFailed(String),
    BusyChanged,
}

pub struct LogModel {
    roots: Vec<PathBuf>,
    worker: Option<LogWorker>,
    snapshot: Option<LogSnapshot>,
    pub cache: MetadataCache,
    pub selection: Selection,
    busy: bool,
    pub scroll: UniformListScrollHandle,
    hovered_element: Option<GraphPrintElement>,
    pub hand_cursor: bool,
    /// Bumped on every snapshot; rows built for an older one are stale.
    pub generation: u64,
    /// When the worker started, for the "first rows painted" log line.
    pub started: std::time::Instant,
    pub first_rows_logged: bool,
}

impl EventEmitter<LogModelEvent> for LogModel {}

struct SnapshotRows<'a>(&'a dyn VisibleGraph<VcsLogCommitStorageIndex>);

impl Rows for SnapshotRows<'_> {
    fn count(&self) -> usize {
        self.0.visible_commit_count().max(0) as usize
    }
    fn commit(&self, row: usize) -> VcsLogCommitStorageIndex {
        self.0.get_row_info(row as i32).commit()
    }
    fn row(&self, commit: VcsLogCommitStorageIndex) -> Option<usize> {
        self.0.get_visible_row_index(&commit).map(|r| r as usize)
    }
}

impl LogModel {
    /// Starts the worker for `roots` (none: the "no repositories" state).
    pub fn new(git: Arc<Git>, roots: Vec<PathBuf>, cx: &mut Context<LogModel>) -> LogModel {
        let worker = (!roots.is_empty()).then(|| {
            let (tx, rx) = flume::unbounded::<LogEvent>();
            cx.spawn(async move |this, cx| {
                while let Ok(event) = rx.recv_async().await {
                    let mut events = vec![event];
                    events.extend(rx.try_iter());
                    if this
                        .update(cx, |model, cx| {
                            for event in events {
                                model.on_event(event, cx);
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
            let sink: EventSink = Arc::new(move |event| tx.send(event).is_ok());
            LogWorker::start(git, roots.clone(), sink)
        });
        LogModel {
            roots,
            worker,
            snapshot: None,
            cache: MetadataCache::new(),
            selection: Selection::default(),
            busy: false,
            scroll: UniformListScrollHandle::new(),
            hovered_element: None,
            hand_cursor: false,
            generation: 0,
            started: std::time::Instant::now(),
            first_rows_logged: false,
        }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn snapshot(&self) -> Option<&LogSnapshot> {
        self.snapshot.as_ref()
    }

    pub fn state(&self) -> LogState {
        if self.roots.is_empty() {
            return LogState::NoRoots;
        }
        match &self.snapshot {
            None => LogState::Loading,
            Some(s) => match s.error() {
                Some(e) => LogState::Error(e.to_string()),
                None => LogState::Ready,
            },
        }
    }

    pub fn row_count(&self) -> usize {
        self.snapshot
            .as_ref()
            .map_or(0, |s| s.row_count().max(0) as usize)
    }

    /// `Vcs.Log.Refresh`: also restarts a crashed worker.
    pub fn refresh(&mut self) {
        if let Some(worker) = &mut self.worker {
            worker.refresh();
        }
    }

    fn on_event(&mut self, event: LogEvent, cx: &mut Context<LogModel>) {
        match event {
            LogEvent::Snapshot(snapshot) => {
                log::info!(
                    "log snapshot {} ({} rows, full: {}) after {:?}",
                    snapshot.generation,
                    snapshot.row_count(),
                    snapshot.data.is_full(),
                    self.started.elapsed()
                );
                self.generation = snapshot.generation;
                self.hovered_element = None;
                self.hand_cursor = false;
                self.snapshot = Some(*snapshot);
            }
            LogEvent::Metadata { loaded, failed } => {
                self.cache.forget(&failed);
                self.cache.put(loaded);
            }
            LogEvent::Busy(busy) => {
                self.busy = busy;
                cx.emit(LogModelEvent::BusyChanged);
            }
            LogEvent::WorkerFailed(message) => {
                self.busy = false;
                cx.emit(LogModelEvent::WorkerFailed(message));
            }
        }
        cx.notify();
    }

    /// The commit and print elements of `row`.
    pub fn row(&self, row: usize) -> Option<(VcsLogCommitStorageIndex, Vec<GraphPrintElement>)> {
        let visible = self.snapshot.as_ref()?.visible.as_ref()?;
        let info = visible.get_row_info(row as i32);
        Some((info.commit(), info.print_elements()))
    }

    /// Whether the first `rows` rows have their metadata (for timing logs).
    pub fn metadata_loaded_for(&mut self, rows: usize) -> bool {
        rows > 0
            && (0..rows).all(|r| {
                self.row(r)
                    .is_some_and(|(c, _)| matches!(self.cache.get(c), Lookup::Loaded(_)))
            })
    }

    pub fn metadata(&mut self, commit: VcsLogCommitStorageIndex) -> Lookup {
        self.cache.get(commit)
    }

    /// Requests the metadata of the rows around `range` (`GraphTableModel.getCommitMetadata`
    /// preloading).
    pub fn request_metadata(&mut self, range: std::ops::Range<usize>) {
        let count = self.row_count();
        let Some(visible) = self.snapshot.as_ref().and_then(|s| s.visible.as_ref()) else {
            return;
        };
        let from = range.start.saturating_sub(UP_PRELOAD_COUNT);
        let to = (range.end + DOWN_PRELOAD_COUNT).min(count);
        let commits: Vec<_> = (from..to)
            .map(|r| visible.get_row_info(r as i32).commit())
            .collect();
        let missing = self.cache.request(commits);
        if let Some(worker) = &self.worker {
            worker.load_metadata(missing);
        }
    }

    fn with_rows<R>(&mut self, f: impl FnOnce(&mut Selection, &dyn Rows) -> R) -> Option<R> {
        let visible = self.snapshot.as_ref()?.visible.as_ref()?;
        Some(f(&mut self.selection, &SnapshotRows(visible)))
    }

    pub fn click_row(&mut self, row: usize, toggle: bool, extend: bool) {
        self.with_rows(|s, rows| s.click(row, toggle, extend, rows));
    }

    pub fn move_selection(&mut self, delta: isize, extend: bool) {
        if let Some(Some(row)) = self.with_rows(|s, rows| s.move_by(delta, extend, rows)) {
            self.scroll.scroll_to_item(row, ScrollStrategy::Nearest);
        }
    }

    pub fn select_all(&mut self) {
        self.with_rows(|s, rows| s.select_all(rows));
    }

    pub fn is_selected(&self, commit: VcsLogCommitStorageIndex) -> bool {
        self.selection.is_selected(commit)
    }

    fn visible_mut(&mut self) -> Option<&mut ig_log::VisibleGraphImpl<VcsLogCommitStorageIndex>> {
        self.snapshot.as_mut()?.visible.as_mut()
    }

    /// Mouse over the graph of `row` at `(x, y)` (row coordinates). Returns whether to repaint.
    pub fn hover_graph(&mut self, row: usize, x: f32, y: f32, geometry: &GraphGeometry) -> bool {
        let element = self
            .row(row)
            .and_then(|(_, elements)| geometry.element_under(&elements, x, y));
        if element == self.hovered_element {
            return false;
        }
        self.hovered_element = element.clone();
        self.perform(GraphAction::new(element, GraphActionType::MouseOver))
    }

    /// The mouse left the graph.
    pub fn unhover_graph(&mut self) -> bool {
        if self.hovered_element.is_none() {
            return false;
        }
        self.hovered_element = None;
        self.perform(GraphAction::new(None, GraphActionType::MouseOver))
    }

    /// A click on the graph of `row`. Returns `true` if it was handled (an element was hit), so the
    /// click doesn't change the selection (`VcsLogGraphTable.shouldChangeSelect`).
    pub fn click_graph(&mut self, row: usize, x: f32, y: f32, geometry: &GraphGeometry) -> bool {
        let Some(element) = self
            .row(row)
            .and_then(|(_, elements)| geometry.element_under(&elements, x, y))
        else {
            return false;
        };
        self.click_element(element);
        true
    }

    /// Clicks a print element of the current graph (an arrow jumps to its target).
    pub fn click_element(&mut self, element: GraphPrintElement) {
        self.perform(GraphAction::new(Some(element), GraphActionType::MouseClick));
    }

    /// Runs an action and handles the answer (`GraphCommitCellController.handleGraphAnswer`).
    fn perform(&mut self, action: GraphAction) -> bool {
        let Some(visible) = self.visible_mut() else {
            return false;
        };
        let answer = visible.action_controller().perform_action(action);
        if let Some(updater) = answer.graph_updater {
            visible.run_graph_updater(updater);
        }
        if let Some(cursor) = answer.cursor_to_set {
            self.hand_cursor = cursor == Cursor::Hand;
        }
        if let (Some(commit), true) = (answer.commit_to_jump, answer.do_jump) {
            self.jump_to_commit(commit);
        }
        true
    }

    /// Selects and scrolls to `commit` (`VcsLogGraphTable.jumpToRow`).
    pub fn jump_to_commit(&mut self, commit: VcsLogCommitStorageIndex) -> Option<usize> {
        let row = self
            .snapshot
            .as_ref()?
            .visible
            .as_ref()?
            .get_visible_row_index(&commit)? as usize;
        self.with_rows(|s, rows| s.select_only(row, rows));
        self.scroll.scroll_to_item(row, ScrollStrategy::Center);
        Some(row)
    }
}
```

- [ ] **Step 2: `crates/ig-app/src/log/mod.rs`** adds `model` and `worker`. Write exactly:

```rust
//! The read-only Git Log: background loading ([`worker`]), the UI-side state ([`model`]) and the
//! ported painters ([`graph_painter`], [`labels`]) with their helpers. The table lands in the
//! next task.

pub mod date;
pub mod graph_painter;
pub mod labels;
pub mod metadata;
pub mod model;
pub mod ref_groups;
pub mod selection;
pub mod worker;
```

- [ ] **Step 3: `crates/ig-app/tests/ui.rs`**, the headless UI tests on `TestAppContext`. The tool-window test is added in the next task. Write exactly:

```rust
//! UI tests with GPUI's `TestAppContext` (headless, no GPU): the Log loads, selects and scrolls,
//! an arrow click jumps, and Refresh reaches the worker.
//!
//! The log worker runs on real threads, so the tests allow parking and poll until it answers.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{AppContext as _, Entity, TestAppContext};
use ig_app::log::model::{LogModel, LogState};
use ig_git::console::NullConsole;
use ig_git::runner::Git;
use ig_log::GraphPrintElement;
use ig_log::print_element::EdgePrintElement;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "T")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "T")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// `main` with `n` commits; `side` forks from the first commit and gets one recent commit, so its
/// edge to the root spans the whole log (drawn with arrows).
fn repo(n: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    git(d, &["init", "-q"]);
    git(d, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    for i in 0..n {
        git(
            d,
            &["commit", "-q", "--allow-empty", "-m", &format!("c{i}")],
        );
        if i == 0 {
            git(d, &["branch", "side"]);
        }
    }
    git(d, &["checkout", "-q", "side"]);
    git(d, &["commit", "-q", "--allow-empty", "-m", "side"]);
    git(d, &["checkout", "-q", "main"]);
    dir
}

fn wait(cx: &mut TestAppContext, model: &Entity<LogModel>, done: impl Fn(&LogModel) -> bool) {
    let started = Instant::now();
    loop {
        cx.run_until_parked();
        if model.read_with(cx, |m, _| done(m)) {
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn log_model(cx: &mut TestAppContext, root: &Path) -> Entity<LogModel> {
    cx.executor().allow_parking();
    let git = Arc::new(Git::detect("git", Arc::new(NullConsole)).unwrap());
    let roots = vec![root.canonicalize().unwrap()];
    let model = cx.new(|cx| LogModel::new(git, roots, cx));
    wait(cx, &model, |m| {
        m.state() == LogState::Ready
            && m.snapshot().is_some_and(|s| s.data.is_full())
            && !m.is_busy()
    });
    model
}

#[gpui::test]
fn log_loads_selects_and_scrolls(cx: &mut TestAppContext) {
    let dir = repo(60);
    let model = log_model(cx, dir.path());
    model.update(cx, |m, _| {
        assert_eq!(m.row_count(), 61);
        m.click_row(3, false, false);
        m.move_selection(1, true);
        m.move_selection(1, true);
        assert_eq!(m.selection.len(), 3);
        m.move_selection(isize::MAX / 2, false);
        assert_eq!(m.selection.len(), 1);
        m.request_metadata(0..10);
    });
    wait(cx, &model, |m| m.cache.len() >= 10);
}

#[gpui::test]
fn arrow_click_jumps_to_the_target_row(cx: &mut TestAppContext) {
    let dir = repo(80);
    let model = log_model(cx, dir.path());
    model.update(cx, |m, _| {
        let arrow = (0..m.row_count())
            .flat_map(|r| m.row(r).unwrap().1)
            .find(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.has_arrow()))
            .expect("the long `side` edge is drawn with arrows");
        let from = arrow_row(&arrow);
        m.click_element(arrow);
        assert_eq!(m.selection.len(), 1, "the jump selects its target");
        let commit = *m.selection.commits().iter().next().unwrap();
        let target = (0..m.row_count())
            .find(|&r| m.row(r).unwrap().0 == commit)
            .unwrap();
        assert!(
            target.abs_diff(from) > 10,
            "jumped from row {from} to {target}"
        );
    });
}

fn arrow_row(element: &GraphPrintElement) -> usize {
    use ig_log::print_element::PrintElement;
    element.row_index() as usize
}

#[gpui::test]
fn refresh_reaches_the_worker(cx: &mut TestAppContext) {
    let dir = repo(5);
    let model = log_model(cx, dir.path());
    git(dir.path(), &["commit", "-q", "--allow-empty", "-m", "new"]);
    model.update(cx, |m, _| m.refresh());
    wait(cx, &model, |m| m.row_count() == 7);
}

#[gpui::test]
fn no_repositories_state(cx: &mut TestAppContext) {
    let git = Arc::new(Git::detect("git", Arc::new(NullConsole)).unwrap());
    let model = cx.new(|cx| LogModel::new(git, Vec::new(), cx));
    model.read_with(cx, |m, _| assert_eq!(m.state(), LogState::NoRoots));
}
```

- [ ] **Step 4: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code origin/wip/app-shell-prototype -- crates/ig-app/src/log/worker.rs crates/ig-app/src/log/model.rs
```

Expected: no output. The intermediate files written above differ from the branch until their final task.

- [ ] **Step 5: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 459 passed, 1 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(ig-app): background log worker, Log model, headless UI tests"
```

---

### Task 9: `ig-app`: Log view, project window, welcome window, menus, `main`

**Files:**
- Create: `crates/ig-app/src/log/view.rs`, `crates/ig-app/src/log/mod.rs`, `crates/ig-app/src/app.rs`, `crates/ig-app/src/menus.rs`, `crates/ig-app/src/project.rs`, `crates/ig-app/src/welcome.rs`, `crates/ig-app/tests/ui.rs`, `crates/ig-app/src/lib.rs`, `crates/ig-app/src/main.rs`

**Review focus:** `view.rs` virtualized table, keyboard and multi-select, arrow hover/click; `project.rs` dock wiring, refresh on focus, worker-panic notification; `welcome.rs` git-missing banner and recent projects; window lifecycle in `app.rs`.

`app`, `menus`, `project` and `welcome` import each other, so they land together. After this task `cargo run --locked -p ig-app -- .` opens IdeaGit on this repository.

- [ ] **Step 1: The modules**, the final `log/mod.rs`, and the final `tests/ui.rs` (adds `git_tool_window_toggles`).

```bash
git checkout origin/wip/app-shell-prototype -- crates/ig-app/src/log/view.rs crates/ig-app/src/log/mod.rs crates/ig-app/src/app.rs crates/ig-app/src/menus.rs crates/ig-app/src/project.rs crates/ig-app/src/welcome.rs crates/ig-app/tests/ui.rs
```

`crates/ig-app/src/log/view.rs`:

```rust
//! The Log tab: a virtualized commit table (graph and subject with labels, author, date, hash).
//!
//! IdeaGit's own table on GPUI's `uniform_list` (spike decision: gpui-component's table keeps a
//! single selected row and draws shadcn-style borders and padding, while the Log needs
//! multi-selection, edge-to-edge graph cells and per-cell mouse hit testing). Rendering follows
//! `VcsLogGraphTable.java` and `GraphCommitCellRenderer.kt`: no header and no grid, the graph
//! painted from the row's print elements, the subject after the graph, labels right-aligned over
//! the subject's end.
//!
//! Deviations: fixed column widths (upstream sizes Author/Date/Hash from their content and lets the
//! user resize and hide columns); loading rows show "Loading…" (upstream shows nothing until the
//! details arrive); no Root column, no current-branch row highlighting, no tooltips.

use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, Font, Hsla,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, TextRun, Window, canvas, div, font,
    px, uniform_list,
};
use gpui_component::dock::PanelEvent;
use ig_log::data::VcsLogRefManager;
use ig_ui_kit::table::TableColors;
use ig_ui_kit::{ActiveIjTheme, color};

use super::date::{DateTexts, format_pretty};
use super::graph_painter::{GraphGeometry, paint_shapes};
use super::labels::{self, LabelStyle, Labels};
use super::metadata::Lookup;
use super::model::{LogModel, LogState};
use super::ref_groups::{RefColor, RepoRefsContext, group_for_table};
use crate::actions::{LOG_TABLE_CONTEXT, RefreshLog, table};
use crate::strings;

const AUTHOR_WIDTH: f32 = 160.0;
const DATE_WIDTH: f32 = 150.0;
const HASH_WIDTH: f32 = 80.0;
const CELL_PADDING: f32 = 8.0;
/// `CommonUiProperties.COMPACT_REFERENCES_VIEW` / `SHOW_TAG_NAMES` defaults
/// (`VcsLogApplicationSettings.State`).
const COMPACT_REFERENCES: bool = true;
const SHOW_TAG_NAMES: bool = false;
/// Short hash length (spec: 8).
const HASH_LENGTH: usize = 8;

pub struct LogView {
    pub model: Entity<LogModel>,
    focus: FocusHandle,
    date_texts: DateTexts,
    /// The table width of the last frame (the Subject column takes what the others leave).
    width: Rc<Cell<f32>>,
}

impl EventEmitter<PanelEvent> for LogView {}

impl Focusable for LogView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl gpui_component::dock::BasePanel for LogView {
    fn panel_name(&self) -> &'static str {
        "GitLog"
    }
}

impl gpui_component::dock::Panel for LogView {
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some(strings::text("VcsLogBundle", "vcs.log.tab.name").into())
    }
}

fn ref_color(color: RefColor, cx: &App) -> Hsla {
    let t = cx.ij().tokens;
    ig_ui_kit::color(match color {
        RefColor::Head => t.ref_head,
        RefColor::LocalBranch => t.ref_local_branch,
        RefColor::RemoteBranch => t.ref_remote_branch,
        RefColor::Tag => t.ref_tag,
        RefColor::Other => t.ref_other,
    })
}

/// Font metrics height (`FontMetrics.getHeight`): ascent + descent, rounded up.
fn font_height(font: &Font, size: f32, window: &Window) -> f32 {
    let ts = window.text_system();
    let id = ts.resolve_font(font);
    f32::from(ts.ascent(id, px(size)) + ts.descent(id, px(size))).ceil()
}

impl LogView {
    pub fn new(model: Entity<LogModel>, cx: &mut Context<LogView>) -> LogView {
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        LogView {
            model,
            focus: cx.focus_handle(),
            date_texts: DateTexts::load(),
            width: Rc::new(Cell::new(1000.0)),
        }
    }

    fn row_height(window: &Window) -> f32 {
        let ui = font(ig_ui_kit::fonts::UI_FAMILY);
        ig_ui_kit::metrics::log_row_height(font_height(
            &ui,
            ig_ui_kit::metrics::UI_FONT_SIZE,
            window,
        ))
    }

    fn render_rows(
        &mut self,
        range: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<LogView>,
    ) -> Vec<AnyElement> {
        let row_height = Self::row_height(window);
        let geometry = GraphGeometry::new(row_height, window.scale_factor());
        let colors = TableColors::get(cx);
        let focused = self.focus.contains_focused(window, cx);
        let theme = cx.ij();
        let t = theme.tokens;
        let palette = theme.graph_palette();
        let ui_font = font(ig_ui_kit::fonts::UI_FAMILY);
        let ui_size = ig_ui_kit::metrics::UI_FONT_SIZE;
        // `LabelPainter.getReferenceFont`: the label font, 1pt smaller
        let label_size = ui_size - 1.0;
        let label_height = font_height(&ui_font, label_size, window);
        let measure_with = |size: f32, s: &str, window: &Window| -> f32 {
            if s.is_empty() {
                return 0.0;
            }
            let run = TextRun {
                len: s.len(),
                font: ui_font.clone(),
                color: colors.foreground,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(SharedString::from(s.to_string()), px(size), &[run], None)
                .width()
                .into()
        };
        let subject_width = (self.width.get() - AUTHOR_WIDTH - DATE_WIDTH - HASH_WIDTH).max(100.0);
        let now = chrono::Local::now().timestamp_millis();
        let loading = SharedString::from("Loading\u{2026}");
        let detached = strings::text("VcsLogBundle", "vcs.log.references.detached.head");
        let view = cx.entity().downgrade();

        let model = self.model.clone();
        model.update(cx, |model, cx| {
            model.request_metadata(range.clone());
            let hand = model.hand_cursor;
            let snapshot = model.snapshot();
            let recommended_width = snapshot.map_or(0, |s| s.recommended_width);
            let mut rows = Vec::with_capacity(range.len());
            for row in range {
                let Some((commit, elements)) = model.row(row) else {
                    continue;
                };
                let meta = match model.metadata(commit) {
                    Lookup::Loaded(meta) => Some(meta),
                    _ => None,
                };
                let selected = model.is_selected(commit);
                let row_colors = colors.row(selected, focused, false);
                let graph_width = geometry.graph_width(&elements, recommended_width);
                let shapes = geometry.shapes(
                    &elements,
                    |id| palette.color(id),
                    if theme.dark { t.foreground >> 8 } else { 0 },
                );

                // labels: refs sorted by the root's ref manager, grouped like `groupForTable`
                let snapshot = model.snapshot().expect("rows come from a snapshot");
                let mut refs = snapshot.data.refs_model().refs_to_commit(commit);
                let labels = if refs.is_empty() {
                    Labels::default()
                } else {
                    let root = refs[0].root().clone();
                    if let Some(provider) = snapshot.data.log_providers().get(&root) {
                        let manager: std::sync::Arc<dyn VcsLogRefManager> =
                            provider.reference_manager();
                        manager.sort_labels(&mut refs);
                    }
                    let repo = snapshot.repo(&root);
                    let context = RepoRefsContext {
                        current_branch: repo.and_then(|r| r.current_branch.as_deref()),
                        remotes: repo.map(|r| r.remotes.as_slice()).unwrap_or(&[]),
                    };
                    let groups = group_for_table(
                        &refs,
                        &context,
                        COMPACT_REFERENCES,
                        SHOW_TAG_NAMES,
                        &detached,
                    );
                    let subject = meta.as_ref().map(|m| m.subject.as_str()).unwrap_or("");
                    let text_width = measure_with(ui_size, subject, window) + CELL_PADDING;
                    let available = labels::available_width(
                        subject_width,
                        graph_width,
                        text_width,
                        COMPACT_REFERENCES,
                    );
                    labels::layout(&groups, available, label_height, COMPACT_REFERENCES, &|s| {
                        measure_with(label_size, s, window)
                    })
                };
                let label_style = LabelStyle {
                    font: ui_font.clone(),
                    font_size: px(label_size),
                    height: label_height,
                    foreground: color(t.log_reference_foreground),
                    warning_foreground: color(t.banner_warning_border),
                    background: row_colors.background,
                    color_of: ref_color,
                    warning_icon: ig_ui_kit::icons::path("general/warning", theme.dark),
                    warning_icon_color: color(t.ref_head),
                };

                let (subject, author, date, hash): (
                    SharedString,
                    SharedString,
                    SharedString,
                    SharedString,
                ) = match &meta {
                    Some(m) => (
                        m.subject.clone().into(),
                        m.author.clone().into(),
                        format_pretty(m.author_time, now, &chrono::Local, &self.date_texts).into(),
                        m.hash
                            .as_string()
                            .chars()
                            .take(HASH_LENGTH)
                            .collect::<String>()
                            .into(),
                    ),
                    None => (loading.clone(), "".into(), "".into(), "".into()),
                };
                let fg = if meta.is_some() {
                    row_colors.foreground
                } else {
                    colors.secondary_foreground
                };
                let background = row_colors.background;
                let (geometry_move, geometry_down) = (geometry.clone(), geometry.clone());
                let (view_move, view_down) = (view.clone(), view.clone());
                let labels_width = labels.width;

                let graph = canvas(
                    |_, _, _| {},
                    move |bounds, _, window, cx| {
                        paint_shapes(&shapes, bounds, background, window, cx);
                        window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                            if !phase.bubble() || !bounds.contains(&e.position) {
                                return;
                            }
                            let (x, y): (f32, f32) = (
                                (e.position.x - bounds.origin.x).into(),
                                (e.position.y - bounds.origin.y).into(),
                            );
                            let g = geometry_move.clone();
                            let _ = view_move.update(cx, |view, cx| {
                                if view.model.update(cx, |m, _| m.hover_graph(row, x, y, &g)) {
                                    cx.notify();
                                }
                            });
                        });
                        window.on_mouse_event(move |e: &MouseDownEvent, phase, _, cx| {
                            if !phase.bubble()
                                || e.button != MouseButton::Left
                                || !bounds.contains(&e.position)
                            {
                                return;
                            }
                            let (x, y): (f32, f32) = (
                                (e.position.x - bounds.origin.x).into(),
                                (e.position.y - bounds.origin.y).into(),
                            );
                            let g = geometry_down.clone();
                            let handled = view_down
                                .update(cx, |view, cx| {
                                    let handled =
                                        view.model.update(cx, |m, _| m.click_graph(row, x, y, &g));
                                    cx.notify();
                                    handled
                                })
                                .unwrap_or(false);
                            if handled {
                                cx.stop_propagation();
                            }
                        });
                    },
                )
                .absolute()
                .left_0()
                .top_0()
                .h_full()
                .w(px(graph_width));

                let label_canvas = canvas(
                    |_, _, _| {},
                    move |bounds, _, window, cx| {
                        labels::paint(&labels, &label_style, bounds, window, cx)
                    },
                )
                .absolute()
                .right_0()
                .top_0()
                .h_full()
                .w(px(labels_width));

                let cell = |text: SharedString, width: f32| {
                    div()
                        .flex_none()
                        .w(px(width))
                        .px(px(CELL_PADDING))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(text)
                };
                let view_click = view.clone();
                rows.push(
                    div()
                        .id(("git-log-row", row))
                        .h(px(row_height))
                        .w_full()
                        .flex()
                        .items_center()
                        .bg(background)
                        .text_color(fg)
                        .when(!selected, |d| d.hover(|d| d.bg(colors.hover)))
                        .on_mouse_down(MouseButton::Left, move |e, window, cx| {
                            let (toggle, extend) = (e.modifiers.secondary(), e.modifiers.shift);
                            let _ = view_click.update(cx, |view, cx| {
                                window.focus(&view.focus, cx);
                                view.model.update(cx, |m, cx| {
                                    m.click_row(row, toggle, extend);
                                    cx.notify();
                                });
                            });
                        })
                        .child(
                            div()
                                .id(("git-log-subject", row))
                                .relative()
                                .flex_1()
                                .h_full()
                                .flex()
                                .items_center()
                                .overflow_hidden()
                                .when(hand, |d| d.cursor_pointer())
                                .on_hover({
                                    let view = view.clone();
                                    move |hovered, _, cx| {
                                        if !*hovered {
                                            let _ = view.update(cx, |view, cx| {
                                                if view.model.update(cx, |m, _| m.unhover_graph()) {
                                                    cx.notify();
                                                }
                                            });
                                        }
                                    }
                                })
                                .child(graph)
                                .child(
                                    div()
                                        .pl(px(graph_width))
                                        .pr(px(labels_width))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(subject),
                                )
                                .child(label_canvas),
                        )
                        .child(cell(author, AUTHOR_WIDTH))
                        .child(cell(date, DATE_WIDTH))
                        .child(cell(hash, HASH_WIDTH))
                        .into_any_element(),
                );
            }
            if !model.first_rows_logged && model.metadata_loaded_for(rows.len()) {
                model.first_rows_logged = true;
                log::info!(
                    "first {} rows with metadata painted after {:?}",
                    rows.len(),
                    model.started.elapsed()
                );
            }
            let _ = cx;
            rows
        })
    }

    fn render_message(&self, text: String, cx: &App) -> AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(color(cx.ij().tokens.secondary_foreground))
            .child(text)
            .into_any_element()
    }
}

impl Render for LogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<LogView>) -> impl IntoElement {
        let colors = TableColors::get(cx);
        let (state, count) = {
            let model = self.model.read(cx);
            (model.state(), model.row_count())
        };
        let width = self.width.clone();
        let body = match state {
            LogState::NoRoots => self.render_message(
                strings::text("VcsLogBundle", "vcs.log.is.not.available"),
                cx,
            ),
            LogState::Loading => {
                self.render_message(strings::text("VcsLogBundle", "vcs.log.is.loading"), cx)
            }
            LogState::Error(message) => div()
                .p(px(12.0))
                .child(
                    ig_ui_kit::notification::Banner::new(
                        "git-log-error",
                        ig_ui_kit::notification::Severity::Error,
                        format!(
                            "{}: {message}",
                            strings::text("VcsLogBundle", "vcs.log.error.loading.commits.status")
                        ),
                    )
                    .action(ig_ui_kit::notification::Action::new(
                        strings::text("VcsLogBundle", "vcs.log.refresh.status.action"),
                        |_, window, cx| window.dispatch_action(Box::new(RefreshLog), cx),
                    )),
                )
                .into_any_element(),
            LogState::Ready if count == 0 => self.render_message(
                strings::text("VcsLogBundle", "vcs.log.no.commits.status"),
                cx,
            ),
            LogState::Ready => {
                let scroll = self.model.read(cx).scroll.clone();
                uniform_list(
                    "git-log-rows",
                    count,
                    cx.processor(|view, range, window, cx| view.render_rows(range, window, cx)),
                )
                .size_full()
                .track_scroll(&scroll)
                .into_any_element()
            }
        };
        let _ = window;
        let model = self.model.clone();
        let on = move |f: fn(&mut LogModel)| {
            let model = model.clone();
            move |_: &dyn gpui::Action, _: &mut Window, cx: &mut App| {
                model.update(cx, |m, cx| {
                    f(m);
                    cx.notify();
                })
            }
        };
        let (up, down, ext_up, ext_down, pg_up, pg_down, first, last, all) = (
            on(|m| m.move_selection(-1, false)),
            on(|m| m.move_selection(1, false)),
            on(|m| m.move_selection(-1, true)),
            on(|m| m.move_selection(1, true)),
            on(|m| m.move_selection(-20, false)),
            on(|m| m.move_selection(20, false)),
            on(|m| m.move_selection(isize::MIN / 2, false)),
            on(|m| m.move_selection(isize::MAX / 2, false)),
            on(|m| m.select_all()),
        );
        div()
            .id("git-log")
            .key_context(LOG_TABLE_CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .bg(colors.background)
            .text_size(px(ig_ui_kit::metrics::UI_FONT_SIZE))
            .on_action(move |a: &table::SelectUp, w, cx| up(a, w, cx))
            .on_action(move |a: &table::SelectDown, w, cx| down(a, w, cx))
            .on_action(move |a: &table::ExtendUp, w, cx| ext_up(a, w, cx))
            .on_action(move |a: &table::ExtendDown, w, cx| ext_down(a, w, cx))
            .on_action(move |a: &table::PageUp, w, cx| pg_up(a, w, cx))
            .on_action(move |a: &table::PageDown, w, cx| pg_down(a, w, cx))
            .on_action(move |a: &table::SelectFirst, w, cx| first(a, w, cx))
            .on_action(move |a: &table::SelectLast, w, cx| last(a, w, cx))
            .on_action(move |a: &table::SelectAll, w, cx| all(a, w, cx))
            .child(
                canvas(
                    move |bounds, _, _| width.set(f32::from(bounds.size.width)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(body)
    }
}
```

`crates/ig-app/src/log/mod.rs`:

```rust
//! The read-only Git Log: background loading ([`worker`]), the UI-side state ([`model`]) and the
//! table ([`view`]) with its ported painters ([`graph_painter`], [`labels`]) and helpers.

pub mod date;
pub mod graph_painter;
pub mod labels;
pub mod metadata;
pub mod model;
pub mod ref_groups;
pub mod selection;
pub mod view;
pub mod worker;
```

`crates/ig-app/src/app.rs`:

```rust
//! Application state (settings, the detected git) and opening windows.
//!
//! Git detection follows `GitExecutableManager` / `GitVersion` checks: `git version` is run once at
//! startup (and again when the git path changes); a missing or too old git shows the welcome
//! window's banner instead of opening projects (`GitExecutableProblemsNotifier`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{App, Global};
use ig_git::console::NullConsole;
use ig_git::runner::Git;
use ig_git::version::GitVersion;

use crate::settings::Settings;
use crate::{menus, paths, project, welcome};

/// How deep `detect_roots` looks for nested repositories in an opened folder.
pub const ROOTS_SCAN_DEPTH: usize = 3;

/// The result of `Git::detect`.
#[derive(Clone, Debug)]
pub enum GitStatus {
    Ok {
        executable: PathBuf,
        version: GitVersion,
    },
    /// `git version` couldn't run (missing executable); the error text.
    Missing(String),
    Unsupported(GitVersion),
}

impl GitStatus {
    pub fn detect(executable: &Path) -> GitStatus {
        match Git::detect(executable, Arc::new(NullConsole)) {
            Ok(git) if git.version().is_supported() => GitStatus::Ok {
                executable: executable.to_path_buf(),
                version: git.version(),
            },
            Ok(git) => GitStatus::Unsupported(git.version()),
            Err(e) => GitStatus::Missing(e.to_string()),
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, GitStatus::Ok { .. })
    }
}

pub struct AppState {
    pub settings: Settings,
    pub settings_path: PathBuf,
    pub git: GitStatus,
}

impl Global for AppState {}

impl AppState {
    pub fn init(cx: &mut App) {
        let settings_path = paths::settings_file();
        let settings = Settings::load(&settings_path);
        let git = GitStatus::detect(&settings.git_executable());
        log::info!("git: {git:?}");
        cx.set_global(AppState {
            settings,
            settings_path,
            git,
        });
    }

    pub fn get(cx: &App) -> &AppState {
        cx.global::<AppState>()
    }

    pub fn save(cx: &mut App) {
        let state = cx.global::<AppState>();
        if let Err(e) = state.settings.save(&state.settings_path) {
            log::warn!("saving settings: {e}");
        }
    }

    /// Sets the git executable (from the welcome screen), re-detects and saves.
    pub fn set_git_path(path: Option<PathBuf>, cx: &mut App) {
        let state = cx.global_mut::<AppState>();
        state.settings.git_path = path;
        state.git = GitStatus::detect(&state.settings.git_executable());
        AppState::save(cx);
    }
}

/// Opens `path` in a project window (or the welcome window if git isn't usable).
pub fn open_project(path: &Path, cx: &mut App) {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let GitStatus::Ok {
        executable,
        version,
    } = AppState::get(cx).git.clone()
    else {
        welcome::open(cx);
        return;
    };
    cx.global_mut::<AppState>().settings.add_recent(&path);
    AppState::save(cx);
    menus::install(cx);
    let roots = ig_git::roots::detect_roots(&path, ROOTS_SCAN_DEPTH);
    log::info!("opening {} with roots {roots:?}", path.display());
    project::open(path, roots, executable, version, cx);
}

/// When the last window closes: closing the last project shows the welcome window (as IntelliJ
/// does); closing the welcome window quits.
pub fn on_window_closed(cx: &mut App) {
    if !cx.windows().is_empty() {
        return;
    }
    if welcome::take_open(cx) {
        cx.quit();
    } else {
        welcome::open(cx);
    }
}
```

`crates/ig-app/src/menus.rs`:

```rust
//! The menu bar: native on macOS, `gpui-component`'s in-window `AppMenuBar` on Windows and Linux.
//!
//! Items follow IntelliJ's main menu (`platform/platform-resources/src/idea/PlatformActions.xml`
//! groups), reduced to what IdeaGit has: File (Open…, Open Recent, Close Project, Exit), View (Tool
//! Windows → Git, plus the IdeaGit "Show read commands in Console" toggle), Git (Refresh Log),
//! Help (About).

use gpui::{App, Menu, MenuItem};
use gpui_component::GlobalState;

use crate::actions::{
    About, ActivateVersionControlToolWindow, CloseProject, Exit, OpenFile, RefreshLog,
    ToggleShowReadCommands,
};
use crate::app::AppState;
use crate::strings;

/// Opens the `index`-th recent project.
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = ideagit, no_json)]
pub struct OpenRecent {
    pub index: usize,
}

/// Recent projects listed in the menu.
const MENU_RECENTS: usize = 10;

/// Whether the console shows read commands (mirrors the toggle for the menu check mark).
#[derive(Default)]
pub struct ShowReadCommands(pub bool);

impl gpui::Global for ShowReadCommands {}

pub fn build(cx: &App) -> Vec<Menu> {
    let recents: Vec<MenuItem> = cx
        .try_global::<AppState>()
        .map(|s| s.settings.recent_projects.clone())
        .unwrap_or_default()
        .into_iter()
        .take(MENU_RECENTS)
        .enumerate()
        .map(|(index, path)| MenuItem::action(path.display().to_string(), OpenRecent { index }))
        .collect();
    let show_read = cx.try_global::<ShowReadCommands>().is_some_and(|s| s.0);
    let git = strings::text("GitBundle", "git4idea.vcs.name");
    let mut menus = Vec::new();
    if cfg!(target_os = "macos") {
        menus.push(Menu::new("IdeaGit").items([
            MenuItem::action("About IdeaGit", About),
            MenuItem::separator(),
            MenuItem::action("Quit IdeaGit", Exit),
        ]));
    }
    let mut file = vec![
        MenuItem::action("Open\u{2026}", OpenFile),
        MenuItem::submenu(Menu::new("Open Recent").items(recents)),
        MenuItem::separator(),
        MenuItem::action("Close Project", CloseProject),
    ];
    if !cfg!(target_os = "macos") {
        file.push(MenuItem::separator());
        file.push(MenuItem::action("Exit", Exit));
    }
    menus.push(Menu::new("File").items(file));
    menus.push(
        Menu::new("View").items([
            MenuItem::submenu(Menu::new("Tool Windows").items([MenuItem::action(
                git.clone(),
                ActivateVersionControlToolWindow,
            )])),
            MenuItem::separator(),
            MenuItem::action("Show Read Commands in Console", ToggleShowReadCommands)
                .checked(show_read),
        ]),
    );
    menus.push(Menu::new(git).items([MenuItem::action(
        strings::text("VcsLogBundle", "action.Vcs.Log.Refresh.text") + " Log",
        RefreshLog,
    )]));
    menus.push(Menu::new("Help").items([MenuItem::action("About", About)]));
    menus
}

/// (Re)installs the menus: natively, and for the in-window menu bar.
pub fn install(cx: &mut App) {
    let owned = build(cx).into_iter().map(Menu::owned).collect();
    GlobalState::global_mut(cx).set_app_menus(owned);
    let menus = build(cx);
    cx.set_menus(menus);
}
```

`crates/ig-app/src/project.rs`:

```rust
//! A project window: main toolbar, tool-window stripes, the dock with the Git tool window (Log and
//! Console tabs) at the bottom, the status bar, and error balloons.
//!
//! Layout ported from IntelliJ's New UI frame (`IdeRootPane`, `ToolWindowPane`, `ToolWindowToolbar`,
//! `MainToolbar`, `IdeStatusBarImpl`): the Git tool window lives at the bottom and its stripe button
//! at the bottom of the left stripe. Refreshes the log on window activation (upstream refreshes on
//! frame activation through the VFS) and after commands IdeaGit runs.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, TitlebarOptions,
    Window, WindowBounds, WindowOptions, div, point, px, size,
};
use gpui_component::dock::{DockArea, DockLayout, DockPlacement, PanelEvent, panel_handle};
use ig_git::console::{ConsoleEvent, ConsoleSink};
use ig_git::runner::Git;
use ig_git::version::GitVersion;
use ig_ui_kit::metrics::MAIN_TOOLBAR_HEIGHT;
use ig_ui_kit::notification::{Action as BalloonAction, Balloon, Severity};
use ig_ui_kit::status_bar::{StatusWidget, status_bar};
use ig_ui_kit::stripe::{StripeButton, StripeSide, stripe};
use ig_ui_kit::tool_window::IjDockRenderer;
use ig_ui_kit::{ActiveIjTheme, IjTheme, color};

use crate::actions::{
    ActivateVersionControlToolWindow, CloseProject, HideActiveWindow, RefreshLog,
    ToggleShowReadCommands,
};
use crate::console::{ChannelConsole, ConsoleView};
use crate::log::model::{LogModel, LogModelEvent};
use crate::log::view::LogView;
use crate::menus::ShowReadCommands;
use crate::strings;

/// The bottom dock's initial height. IdeaGit has no editor yet, so the Log gets most of the frame
/// (IntelliJ's default bottom tool window weight is 0.33).
const BOTTOM_DOCK_HEIGHT: f32 = 560.0;

/// Forwards console events and refreshes the log after a non-silent (IdeaGit-run) command
/// finishes (upstream: `GitRepository` updates after write commands trigger a log refresh).
struct ProjectConsole {
    console: ChannelConsole,
    refresh: flume::Sender<()>,
}

impl ConsoleSink for ProjectConsole {
    fn event(&self, event: ConsoleEvent) {
        let finished = matches!(event, ConsoleEvent::Finished { .. });
        self.console.event(event);
        if finished {
            let _ = self.refresh.send(());
        }
    }

    fn silent_command(&self, working_dir: &std::path::Path, command_line: &str) {
        self.console.silent_command(working_dir, command_line);
    }
}

struct Notice {
    id: usize,
    severity: Severity,
    title: String,
    content: String,
}

/// An editor-area placeholder (IdeaGit has no editor yet).
pub struct EmptyEditor(FocusHandle);

impl gpui::EventEmitter<PanelEvent> for EmptyEditor {}
impl Focusable for EmptyEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.0.clone()
    }
}
impl gpui_component::dock::BasePanel for EmptyEditor {
    fn panel_name(&self) -> &'static str {
        "EmptyEditor"
    }
}
impl gpui_component::dock::Panel for EmptyEditor {}
impl Render for EmptyEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(color(cx.ij().tokens.panel_background))
    }
}

/// The console as a dock panel.
pub struct ConsolePanel {
    pub view: Entity<ConsoleView>,
}

impl gpui::EventEmitter<PanelEvent> for ConsolePanel {}
impl Focusable for ConsolePanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.view.read(cx).focus_handle(cx)
    }
}
impl gpui_component::dock::BasePanel for ConsolePanel {
    fn panel_name(&self) -> &'static str {
        "GitConsole"
    }
}
impl gpui_component::dock::Panel for ConsolePanel {
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some(strings::text("VcsBundle", "vcs.console.toolwindow.display.name").into())
    }
}
impl Render for ConsolePanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.view.clone()
    }
}

pub struct ProjectWindow {
    pub path: PathBuf,
    pub dock: Entity<DockArea>,
    pub log: Entity<LogModel>,
    pub log_view: Entity<LogView>,
    pub console: Entity<ConsoleView>,
    notices: Vec<Notice>,
    next_notice: usize,
    menu_bar: Option<Entity<gpui_component::menu::AppMenuBar>>,
    _subscriptions: Vec<Subscription>,
}

impl ProjectWindow {
    /// Builds the window content. `git` is built here so its console feeds this window.
    pub fn new(
        path: PathBuf,
        roots: Vec<PathBuf>,
        executable: PathBuf,
        version: GitVersion,
        window: &mut Window,
        cx: &mut Context<ProjectWindow>,
    ) -> ProjectWindow {
        let (console, sink) = crate::console::create(cx);
        let (refresh_tx, refresh_rx) = flume::unbounded();
        let git = Arc::new(Git::new(
            executable,
            version,
            Arc::new(ProjectConsole {
                console: sink,
                refresh: refresh_tx,
            }),
        ));
        let log = cx.new(|cx| LogModel::new(git, roots, cx));
        let log_view = cx.new(|cx| LogView::new(log.clone(), cx));
        let console_panel = cx.new(|_| ConsolePanel {
            view: console.clone(),
        });
        let editor = cx.new(|cx| EmptyEditor(cx.focus_handle()));

        let this = cx.entity().downgrade();
        let renderer = IjDockRenderer::with_headerless(
            strings::text("GitBundle", "git4idea.vcs.name"),
            move |window, cx| {
                let _ = this.update(cx, |this, cx| this.set_git_tool_window(false, window, cx));
            },
            vec!["EmptyEditor"],
        );
        let dock = cx.new(|cx| DockArea::new("project", None, window, cx).with_renderer(renderer));
        dock.update(cx, |dock, cx| {
            dock.set_center(DockLayout::tabs().panel(editor), window, cx);
            dock.set_dock(
                DockPlacement::Bottom,
                DockLayout::tabs()
                    .panel_view(panel_handle(log_view.clone()), cx)
                    .panel_view(panel_handle(console_panel), cx),
                window,
                cx,
            );
            dock.set_dock_size(DockPlacement::Bottom, px(BOTTOM_DOCK_HEIGHT), window, cx);
        });

        let log_for_refresh = log.downgrade();
        cx.spawn(async move |_, cx| {
            while refresh_rx.recv_async().await.is_ok() {
                while refresh_rx.try_recv().is_ok() {}
                if log_for_refresh.update(cx, |log, _| log.refresh()).is_err() {
                    break;
                }
            }
        })
        .detach();

        let subscriptions = vec![
            cx.subscribe_in(&log, window, |this, _, event: &LogModelEvent, _, cx| {
                if let LogModelEvent::WorkerFailed(message) = event {
                    this.notify(
                        Severity::Error,
                        "Git Log failed",
                        &format!("{message}. Refresh to restart it; details in ideagit.log."),
                        cx,
                    );
                }
                cx.notify();
            }),
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.log.update(cx, |log, _| log.refresh());
                }
            }),
            cx.observe_window_appearance(window, |_, window, cx| {
                if IjTheme::sync_with_window(window, cx) {
                    cx.refresh_windows();
                }
            }),
        ];
        let menu_bar =
            (!cfg!(target_os = "macos")).then(|| gpui_component::menu::AppMenuBar::new(cx));
        window.focus(&log_view.focus_handle(cx), cx);
        ProjectWindow {
            path,
            dock,
            log,
            log_view,
            console,
            notices: Vec::new(),
            next_notice: 0,
            menu_bar,
            _subscriptions: subscriptions,
        }
    }

    pub fn notify(
        &mut self,
        severity: Severity,
        title: &str,
        content: &str,
        cx: &mut Context<Self>,
    ) {
        self.next_notice += 1;
        self.notices.push(Notice {
            id: self.next_notice,
            severity,
            title: title.into(),
            content: content.into(),
        });
        cx.notify();
    }

    pub fn is_git_tool_window_open(&self, cx: &App) -> bool {
        self.dock.read(cx).is_dock_open(DockPlacement::Bottom)
    }

    pub fn set_git_tool_window(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_git_tool_window_open(cx) != open {
            self.dock.update(cx, |dock, cx| {
                dock.toggle_dock(DockPlacement::Bottom, window, cx)
            });
        }
        if open {
            window.focus(&self.log_view.focus_handle(cx), cx);
        }
        cx.notify();
    }

    fn project_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    fn branch_text(&self, cx: &App) -> Option<String> {
        let snapshot = self.log.read(cx).snapshot()?;
        let multi = snapshot.repos.len() > 1;
        let parts: Vec<String> = snapshot
            .repos
            .iter()
            .map(|r| {
                let branch = r.current_branch.clone().unwrap_or_else(|| {
                    strings::text("VcsLogBundle", "vcs.log.references.detached.head")
                });
                if multi {
                    format!(
                        "{}: {branch}",
                        r.root
                            .file_name()
                            .map(|n| n.to_string_lossy())
                            .unwrap_or_default()
                    )
                } else {
                    branch
                }
            })
            .collect();
        (!parts.is_empty()).then(|| parts.join(", "))
    }

    fn render_toolbar(&self, cx: &App) -> AnyElement {
        let t = cx.ij().tokens;
        let branch = self.branch_text(cx);
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.0))
            .h(px(MAIN_TOOLBAR_HEIGHT))
            // room for the macOS traffic lights in the transparent title bar
            .pl(px(if cfg!(target_os = "macos") {
                80.0
            } else {
                12.0
            }))
            .bg(color(t.main_toolbar_background))
            .text_color(color(t.main_toolbar_foreground))
            .text_size(px(ig_ui_kit::metrics::UI_FONT_SIZE))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child(self.project_name())
            .when_some(branch, |d, branch| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .font_weight(gpui::FontWeight::NORMAL)
                        // the main toolbar is dark in both New UI themes
                        .child(ig_ui_kit::icons::icon("vcs/branch", true, 16.0))
                        .child(branch),
                )
            })
            .into_any_element()
    }
}

impl Render for ProjectWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.ij().tokens;
        let git_open = self.is_git_tool_window_open(cx);
        let git_name = strings::text("GitBundle", "git4idea.vcs.name");
        let this = cx.entity().downgrade();
        let git_button = StripeButton::new("stripe-git", "toolwindows/git")
            .selected(git_open)
            .tooltip(git_name)
            .on_click(move |_, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    let open = !this.is_git_tool_window_open(cx);
                    this.set_git_tool_window(open, window, cx);
                });
            });
        let busy = self.log.read(cx).is_busy();
        let mut right: Vec<AnyElement> = Vec::new();
        if let Some(branch) = self.branch_text(cx) {
            right.push(
                StatusWidget::new("status-branch", branch)
                    .icon("vcs/branch")
                    .into_any_element(),
            );
        }
        let left: Vec<AnyElement> = if busy {
            vec![
                StatusWidget::progress(
                    "status-progress",
                    strings::text("VcsLogBundle", "vcs.log.loading.status"),
                )
                .into_any_element(),
            ]
        } else {
            Vec::new()
        };
        let notices: Vec<AnyElement> = self
            .notices
            .iter()
            .map(|n| {
                let id = n.id;
                let this = cx.entity().downgrade();
                Balloon::new(
                    ("notice", id),
                    n.severity,
                    n.title.clone(),
                    n.content.clone(),
                )
                .action(BalloonAction::new("Dismiss", move |_, _, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.notices.retain(|n| n.id != id);
                        cx.notify();
                    });
                }))
                .into_any_element()
            })
            .collect();
        let _ = window;
        div()
            .id("project-window")
            .size_full()
            .flex()
            .flex_col()
            .bg(color(t.panel_background))
            .text_color(color(t.foreground))
            .font_family(ig_ui_kit::fonts::UI_FAMILY)
            .on_action(cx.listener(|this, _: &RefreshLog, _, cx| {
                this.log.update(cx, |log, _| log.refresh())
            }))
            .on_action(
                cx.listener(|this, _: &ActivateVersionControlToolWindow, window, cx| {
                    this.set_git_tool_window(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &HideActiveWindow, window, cx| {
                this.set_git_tool_window(false, window, cx)
            }))
            .on_action(cx.listener(|_, _: &CloseProject, window, _| window.remove_window()))
            .on_action(cx.listener(|this, _: &ToggleShowReadCommands, _, cx| {
                let show = !this.console.read(cx).show_silent();
                this.console.update(cx, |c, cx| c.set_show_silent(show, cx));
                cx.set_global(ShowReadCommands(show));
                crate::menus::install(cx);
            }))
            .when_some(self.menu_bar.clone(), |d, bar| d.child(bar))
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(stripe(StripeSide::Left, vec![git_button], cx))
                    .child(div().flex_1().min_w_0().h_full().child(self.dock.clone()))
                    .child(stripe(StripeSide::Right, Vec::new(), cx)),
            )
            .child(status_bar(left, right, cx))
            .child(
                div()
                    .absolute()
                    .bottom(px(40.0))
                    .right(px(56.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(notices),
            )
    }
}

/// Opens a project window.
pub fn open(
    path: PathBuf,
    roots: Vec<PathBuf>,
    executable: PathBuf,
    version: GitVersion,
    cx: &mut App,
) {
    let title = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(gpui::Bounds::centered(
            None,
            size(px(1400.0), px(900.0)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(12.0), px(13.0))),
        }),
        app_id: Some("IdeaGit".into()),
        ..Default::default()
    };
    let result = cx.open_window(options, move |window, cx| {
        let project = cx.new(|cx| ProjectWindow::new(path, roots, executable, version, window, cx));
        cx.new(|cx| gpui_component::Root::new(project, window, cx))
    });
    if let Err(e) = result {
        log::error!("opening a project window: {e}");
    } else {
        crate::welcome::close(cx);
    }
}
```

`crates/ig-app/src/welcome.rs`:

```rust
//! The welcome window: Open…, Clone (disabled for now), the recent projects list, and the
//! "Git is not installed" banner with a git path field.
//!
//! Look after IntelliJ's New UI welcome screen (`platform/platform-impl/src/com/intellij/openapi/wm/impl/welcomeScreen/`):
//! a left pane with the product name, a projects pane with action buttons on top and the recent
//! projects below. The git banner follows `GitExecutableProblemsNotifier` texts.
//!
//! Deviations: no search field, no project groups or icons, no "Customize" / "Plugins" / "Learn"
//! tabs.

use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, Global, InteractiveElement,
    IntoElement, ParentElement, PathPromptOptions, Render, StatefulInteractiveElement, Styled,
    Window, WindowBounds, WindowOptions, div, px, size,
};
use gpui_component::Disableable as _;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputState};
use ig_ui_kit::notification::{Action, Banner, Severity};
use ig_ui_kit::{ActiveIjTheme, IjTheme, color};

use crate::actions::OpenFile;
use crate::app::{AppState, GitStatus, open_project};
use crate::strings;

/// The open welcome window, if any.
#[derive(Default)]
struct WelcomeWindow(Option<AnyWindowHandle>);

impl Global for WelcomeWindow {}

pub struct Welcome {
    git_path: Entity<InputState>,
}

/// Shows the folder picker and opens the chosen folder (`OpenFileAction`).
pub fn prompt_open(cx: &mut App) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Open".into()),
    });
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = paths.await
            && let Some(path) = paths.into_iter().next()
        {
            cx.update(|cx| open_project(&path, cx));
        }
    })
    .detach();
}

impl Welcome {
    fn new(window: &mut Window, cx: &mut Context<Welcome>) -> Welcome {
        let current = AppState::get(cx)
            .settings
            .git_path
            .clone()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let git_path = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("git")
                .default_value(current)
        });
        cx.observe_window_appearance(window, |_, window, cx| {
            if IjTheme::sync_with_window(window, cx) {
                cx.refresh_windows();
            }
        })
        .detach();
        Welcome { git_path }
    }

    fn git_banner(&self, cx: &App) -> Option<Banner> {
        let text = match &AppState::get(cx).git {
            GitStatus::Ok { .. } => return None,
            GitStatus::Missing(_) => format!(
                "{} {}",
                strings::text("GitBundle", "executable.error.git.not.installed"),
                strings::text("GitBundle", "executable.error.git.not.installed.hint")
            ),
            GitStatus::Unsupported(version) => format!(
                "{}. {}",
                strings::message(
                    "GitBundle",
                    "git.executable.validation.error.version.title",
                    &[&version.presentation()]
                ),
                strings::message(
                    "GitBundle",
                    "git.executable.validation.error.version.message",
                    &[&ig_git::version::GitVersion::min().presentation()]
                ),
            ),
        };
        Some(Banner::new("git-banner", Severity::Warning, text))
    }

    fn save_git_path(&mut self, cx: &mut Context<Welcome>) {
        let text = self.git_path.read(cx).value().trim().to_string();
        AppState::set_git_path((!text.is_empty()).then(|| PathBuf::from(text)), cx);
        cx.notify();
    }
}

impl Render for Welcome {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Welcome>) -> impl IntoElement {
        let theme = cx.ij();
        let t = theme.tokens;
        let state = AppState::get(cx);
        let git_ok = state.git.is_ok();
        let recents = state.settings.recent_projects.clone();
        let banner = self.git_banner(cx);
        let recent_items = recents.into_iter().enumerate().map(|(ix, path)| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let full = path.display().to_string();
            div()
                .id(("recent", ix))
                .flex()
                .items_center()
                .gap(px(10.0))
                .px(px(12.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .hover(|d| d.bg(color(t.list_hover_background)))
                .child(ig_ui_kit::icons::icon(
                    "nodes/ideaProject",
                    theme.dark,
                    20.0,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(name))
                        .child(div().text_color(color(t.secondary_foreground)).child(full)),
                )
                .on_click(move |_, _, cx| open_project(&path, cx))
        });
        div()
            .id("welcome")
            .size_full()
            .flex()
            .bg(color(t.welcome_background))
            .text_color(color(t.foreground))
            .font_family(ig_ui_kit::fonts::UI_FAMILY)
            .text_size(px(ig_ui_kit::metrics::UI_FONT_SIZE))
            .on_action(|_: &OpenFile, _, cx| prompt_open(cx))
            .child(
                div()
                    .w(px(220.0))
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .p(px(20.0))
                    .pt(px(40.0))
                    .border_r_1()
                    .border_color(color(t.border))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("IdeaGit"),
                    )
                    .child(
                        div()
                            .text_color(color(t.secondary_foreground))
                            .child(env!("CARGO_PKG_VERSION")),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .p(px(20.0))
                    .pt(px(40.0))
                    .bg(color(t.welcome_details_background))
                    .when_some(banner, |d, banner| {
                        let this = cx.entity().downgrade();
                        d.child(banner.action(Action::new("Save", move |_, _, cx| {
                            let _ = this.update(cx, |this, cx| this.save_git_path(cx));
                        })))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child("Path to Git executable:")
                                .child(div().flex_1().child(Input::new(&self.git_path))),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                Button::new("open")
                                    .primary()
                                    .label("Open\u{2026}")
                                    .disabled(!git_ok)
                                    .on_click(|_, _, cx| prompt_open(cx)),
                            )
                            .child(
                                Button::new("clone")
                                    .label("Clone Repository\u{2026}")
                                    .disabled(true),
                            ),
                    )
                    .child(
                        div()
                            .id("recents")
                            .flex()
                            .flex_col()
                            .overflow_y_scroll()
                            .children(recent_items),
                    ),
            )
    }
}

/// Opens (or focuses) the welcome window.
pub fn open(cx: &mut App) {
    if let Some(handle) = cx.try_global::<WelcomeWindow>().and_then(|w| w.0)
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(gpui::Bounds::centered(
            None,
            size(px(800.0), px(650.0)),
            cx,
        ))),
        app_id: Some("IdeaGit".into()),
        ..Default::default()
    };
    match cx.open_window(options, |window, cx| {
        let welcome = cx.new(|cx| Welcome::new(window, cx));
        cx.new(|cx| gpui_component::Root::new(welcome, window, cx))
    }) {
        Ok(handle) => cx.set_global(WelcomeWindow(Some(handle.into()))),
        Err(e) => log::error!("opening the welcome window: {e}"),
    }
}

/// Closes the welcome window (a project was opened).
pub fn close(cx: &mut App) {
    if let Some(handle) = cx.try_global::<WelcomeWindow>().and_then(|w| w.0) {
        cx.set_global(WelcomeWindow(None));
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

/// Whether the welcome window was open (and is now closed); clears the record.
pub fn take_open(cx: &mut App) -> bool {
    let was_open = cx
        .try_global::<WelcomeWindow>()
        .is_some_and(|w| w.0.is_some());
    cx.set_global(WelcomeWindow(None));
    was_open
}
```

`crates/ig-app/tests/ui.rs`:

```rust
//! UI tests with GPUI's `TestAppContext` (headless, no GPU): the Log loads, selects and scrolls,
//! an arrow click jumps, Refresh reaches the worker, and the Git tool window toggles.
//!
//! The log worker runs on real threads, so the tests allow parking and poll until it answers.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{AppContext as _, Entity, TestAppContext};
use ig_app::log::model::{LogModel, LogState};
use ig_git::console::NullConsole;
use ig_git::runner::Git;
use ig_log::GraphPrintElement;
use ig_log::print_element::EdgePrintElement;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "T")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "T")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// `main` with `n` commits; `side` forks from the first commit and gets one recent commit, so its
/// edge to the root spans the whole log (drawn with arrows).
fn repo(n: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    git(d, &["init", "-q"]);
    git(d, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    for i in 0..n {
        git(
            d,
            &["commit", "-q", "--allow-empty", "-m", &format!("c{i}")],
        );
        if i == 0 {
            git(d, &["branch", "side"]);
        }
    }
    git(d, &["checkout", "-q", "side"]);
    git(d, &["commit", "-q", "--allow-empty", "-m", "side"]);
    git(d, &["checkout", "-q", "main"]);
    dir
}

fn wait(cx: &mut TestAppContext, model: &Entity<LogModel>, done: impl Fn(&LogModel) -> bool) {
    let started = Instant::now();
    loop {
        cx.run_until_parked();
        if model.read_with(cx, |m, _| done(m)) {
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn log_model(cx: &mut TestAppContext, root: &Path) -> Entity<LogModel> {
    cx.executor().allow_parking();
    let git = Arc::new(Git::detect("git", Arc::new(NullConsole)).unwrap());
    let roots = vec![root.canonicalize().unwrap()];
    let model = cx.new(|cx| LogModel::new(git, roots, cx));
    wait(cx, &model, |m| {
        m.state() == LogState::Ready
            && m.snapshot().is_some_and(|s| s.data.is_full())
            && !m.is_busy()
    });
    model
}

#[gpui::test]
fn log_loads_selects_and_scrolls(cx: &mut TestAppContext) {
    let dir = repo(60);
    let model = log_model(cx, dir.path());
    model.update(cx, |m, _| {
        assert_eq!(m.row_count(), 61);
        m.click_row(3, false, false);
        m.move_selection(1, true);
        m.move_selection(1, true);
        assert_eq!(m.selection.len(), 3);
        m.move_selection(isize::MAX / 2, false);
        assert_eq!(m.selection.len(), 1);
        m.request_metadata(0..10);
    });
    wait(cx, &model, |m| m.cache.len() >= 10);
}

#[gpui::test]
fn arrow_click_jumps_to_the_target_row(cx: &mut TestAppContext) {
    let dir = repo(80);
    let model = log_model(cx, dir.path());
    model.update(cx, |m, _| {
        let arrow = (0..m.row_count())
            .flat_map(|r| m.row(r).unwrap().1)
            .find(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.has_arrow()))
            .expect("the long `side` edge is drawn with arrows");
        let from = arrow_row(&arrow);
        m.click_element(arrow);
        assert_eq!(m.selection.len(), 1, "the jump selects its target");
        let commit = *m.selection.commits().iter().next().unwrap();
        let target = (0..m.row_count())
            .find(|&r| m.row(r).unwrap().0 == commit)
            .unwrap();
        assert!(
            target.abs_diff(from) > 10,
            "jumped from row {from} to {target}"
        );
    });
}

fn arrow_row(element: &GraphPrintElement) -> usize {
    use ig_log::print_element::PrintElement;
    element.row_index() as usize
}

#[gpui::test]
fn refresh_reaches_the_worker(cx: &mut TestAppContext) {
    let dir = repo(5);
    let model = log_model(cx, dir.path());
    git(dir.path(), &["commit", "-q", "--allow-empty", "-m", "new"]);
    model.update(cx, |m, _| m.refresh());
    wait(cx, &model, |m| m.row_count() == 7);
}

#[gpui::test]
fn no_repositories_state(cx: &mut TestAppContext) {
    let git = Arc::new(Git::detect("git", Arc::new(NullConsole)).unwrap());
    let model = cx.new(|cx| LogModel::new(git, Vec::new(), cx));
    model.read_with(cx, |m, _| assert_eq!(m.state(), LogState::NoRoots));
}

#[gpui::test]
fn git_tool_window_toggles(cx: &mut TestAppContext) {
    let dir = repo(3);
    // `init` only reads `settings.toml` (nothing is written unless a project is opened via
    // `open_project`), so the test doesn't touch the user's settings.
    cx.executor().allow_parking();
    cx.update(ig_app::init);
    let root = dir.path().canonicalize().unwrap();
    let version = Git::detect("git", Arc::new(NullConsole)).unwrap().version();
    let (project, cx) = cx.add_window_view(|window, cx| {
        ig_app::project::ProjectWindow::new(
            root.clone(),
            vec![root.clone()],
            "git".into(),
            version,
            window,
            cx,
        )
    });
    project.update_in(cx, |p, window, cx| {
        assert!(p.is_git_tool_window_open(cx));
        p.set_git_tool_window(false, window, cx);
        assert!(!p.is_git_tool_window_open(cx));
        p.set_git_tool_window(true, window, cx);
        assert!(p.is_git_tool_window_open(cx));
    });
}
```

- [ ] **Step 2: `crates/ig-app/src/lib.rs`**: everything but `screenshots`. Write exactly:

```rust
//! IdeaGit's application: windows, actions, the Git tool window (Log and Console) and the log
//! worker. Built on `ig-git` / `ig-log` for Git and `ig-ui-kit` for the IntelliJ New UI look.
//!
//! Every module names the upstream sources it ports or imitates; see also
//! `reference/upstream-map.toml`.

pub mod actions;
pub mod app;
pub mod console;
pub mod log;
pub mod logging;
pub mod menus;
pub mod paths;
pub mod project;
pub mod settings;
pub mod strings;
pub mod welcome;

use std::path::PathBuf;

use gpui::App;

use crate::actions::{About, Exit, OpenFile};

/// Registers IdeaGit's globals, theme, fonts, actions, keymap and menus.
pub fn init(cx: &mut App) {
    gpui_component::init(cx);
    if let Err(e) = ig_ui_kit::init(cx.window_appearance(), cx) {
        ::log::error!("loading fonts: {e}");
    }
    app::AppState::init(cx);
    actions::bind_keys(cx);
    cx.on_action(|_: &OpenFile, cx| welcome::prompt_open(cx));
    cx.on_action(|_: &Exit, cx| cx.quit());
    cx.on_action(|_: &About, cx| show_about(cx));
    cx.on_action(|action: &menus::OpenRecent, cx| {
        let path = app::AppState::get(cx)
            .settings
            .recent_projects
            .get(action.index)
            .cloned();
        if let Some(path) = path {
            app::open_project(&path, cx);
        }
    });
    menus::install(cx);
    cx.on_window_closed(|cx, _| app::on_window_closed(cx))
        .detach();
}

fn show_about(cx: &mut App) {
    let Some(window) = cx
        .active_window()
        .or_else(|| cx.windows().into_iter().next())
    else {
        return;
    };
    let detail = format!(
        "Version {}\nA Rust port of IntelliJ IDEA's Git features (intellij-community {}).",
        env!("CARGO_PKG_VERSION"),
        &crate::UPSTREAM_COMMIT[..7]
    );
    let _ = window.update(cx, |_, window, cx| {
        // the answer isn't needed: the dialog only has OK
        drop(window.prompt(
            gpui::PromptLevel::Info,
            "IdeaGit",
            Some(&detail),
            &["OK"],
            cx,
        ));
    });
}

/// The upstream commit the port matches (`reference/UPSTREAM`).
pub const UPSTREAM_COMMIT: &str = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98";

/// `ideagit [path]`: opens `path`, or the welcome window.
pub fn run(path: Option<PathBuf>) {
    gpui_platform::application()
        .with_assets(ig_ui_kit::icons::Assets)
        .run(move |cx: &mut App| {
            init(cx);
            match &path {
                Some(path) => app::open_project(path, cx),
                None => welcome::open(cx),
            }
            cx.activate(true);
        });
}
```

- [ ] **Step 3: `crates/ig-app/src/main.rs`**, without the `--screenshots` switch. Write exactly:

```rust
//! `cargo run -p ig-app -- [path]`: opens a folder directly, or the welcome window.

fn main() {
    ig_app::logging::init();
    let mut args = std::env::args_os().skip(1);
    let path = args.next().map(std::path::PathBuf::from);
    ig_app::run(path);
}
```

- [ ] **Step 4: Smoke-run it (macOS, by hand if you can).** Expected: a project window with the Git tool window open on the Log tab, showing this repository's history with the graph and ref labels. Close it. Skip this step in a headless environment and say so in the report.

```bash
cargo run --locked -p ig-app -- .
```

- [ ] **Step 5: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code origin/wip/app-shell-prototype -- crates/ig-app/src/log/view.rs crates/ig-app/src/log/mod.rs crates/ig-app/src/app.rs crates/ig-app/src/menus.rs crates/ig-app/src/project.rs crates/ig-app/src/welcome.rs crates/ig-app/tests/ui.rs
```

Expected: no output. The intermediate files written above differ from the branch until their final task.

- [ ] **Step 6: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 460 passed, 1 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(ig-app): Log table, project and welcome windows, menus; ideagit runs"
```

---

### Task 10: Screenshots

**Files:**
- Create: `crates/ig-app/src/screenshots.rs`, `crates/ig-app/src/lib.rs`, `crates/ig-app/src/main.rs`, `docs/ui-reference/**`

**Review focus:** `screenshots.rs` waits for the worker with a timeout and never hangs; feature-gated so default builds don't pull test support.

- [ ] **Step 1: The module and the final `lib.rs` and `main.rs`.**

```bash
git checkout origin/wip/app-shell-prototype -- crates/ig-app/src/screenshots.rs crates/ig-app/src/lib.rs crates/ig-app/src/main.rs
```

`crates/ig-app/src/screenshots.rs`:

```rust
//! `cargo xtask screenshots`: renders a repository's Log in the light and dark themes to PNG
//! without a display or screen-recording permission (GPUI's `HeadlessAppContext` with the
//! platform's offscreen renderer: Metal on macOS, wgpu on Linux).

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{AppContext as _, HeadlessAppContext, px, size};
use ig_ui_kit::IjTheme;

use crate::app::{AppState, GitStatus};
use crate::log::model::LogState;
use crate::project::ProjectWindow;

/// Renders `repo` into `out/ideagit-log-{light,dark}.png`; returns the process exit code.
pub fn run(out: &Path, repo: &Path) -> i32 {
    let repo = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    if let Err(e) = std::fs::create_dir_all(out) {
        eprintln!("can't create {}: {e}", out.display());
        return 1;
    }
    for (name, theme) in [("light", IjTheme::light()), ("dark", IjTheme::dark())] {
        let path = out.join(format!("ideagit-log-{name}.png"));
        match render(&repo, theme, &path) {
            Ok(elapsed) => eprintln!("wrote {} (log ready after {elapsed:?})", path.display()),
            Err(e) => {
                eprintln!("{name}: {e}");
                return 1;
            }
        }
    }
    0
}

fn render(repo: &Path, theme: IjTheme, path: &Path) -> Result<Duration, String> {
    let text_system = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(
        text_system,
        Arc::new(ig_ui_kit::icons::Assets),
        gpui_platform::current_headless_renderer,
    );
    cx.allow_parking();
    cx.update(|cx| {
        crate::init(cx);
        theme.pin(cx);
    });
    let (executable, version) = match cx.update(|cx| AppState::get(cx).git.clone()) {
        GitStatus::Ok {
            executable,
            version,
        } => (executable, version),
        other => return Err(format!("git is not usable: {other:?}")),
    };
    let roots = ig_git::roots::detect_roots(repo, crate::app::ROOTS_SCAN_DEPTH);
    let repo_path = repo.to_path_buf();
    let window = cx
        .open_window(size(px(1400.0), px(900.0)), move |window, cx| {
            cx.new(|cx| ProjectWindow::new(repo_path, roots, executable, version, window, cx))
        })
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    let mut ready_at = None;
    // wait for the log and the metadata of the first rows (the worker runs on real threads)
    while started.elapsed() < Duration::from_secs(60) {
        cx.run_until_parked();
        let done = cx
            .update_window(window.into(), |root, _, cx| {
                let project = root.downcast::<ProjectWindow>().ok()?;
                let log = project.read(cx).log.read(cx);
                let ready = match log.state() {
                    LogState::Ready => {
                        log.row_count() > 0 && log.cache.len() >= log.row_count().min(30)
                    }
                    LogState::Loading => false,
                    _ => true,
                };
                Some(ready && !log.is_busy())
            })
            .ok()
            .flatten()
            .unwrap_or(false);
        if done {
            ready_at.get_or_insert(started.elapsed());
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let elapsed = ready_at.ok_or("the log didn't load within 60 s")?;
    // two frames: the first lays out the table, the second paints rows with metadata requested by it
    for _ in 0..3 {
        cx.update_window(window.into(), |_, window, _| window.refresh())
            .map_err(|e| e.to_string())?;
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(100));
    }
    let image = cx
        .capture_screenshot(window.into())
        .map_err(|e| e.to_string())?;
    image.save(path).map_err(|e| e.to_string())?;
    if std::env::var_os("IDEAGIT_SCROLL_BENCH").is_some() {
        scroll_bench(&mut cx, window.into());
    }
    Ok(elapsed)
}

/// Informal scroll measurement: jumps through the whole log (worst case: every frame shows new
/// rows) and times building each frame (layout + paint into the scene, CPU side; GPU work and
/// presentation are not included).
fn scroll_bench(cx: &mut HeadlessAppContext, window: gpui::AnyWindowHandle) {
    let mut times = Vec::new();
    for step in 0..300usize {
        let started = Instant::now();
        let _ = cx.update_window(window, |root, window, cx| {
            if let Ok(project) = root.downcast::<ProjectWindow>() {
                let log = project.read(cx).log.clone();
                log.update(cx, |log, _| {
                    let rows = log.row_count().max(1);
                    log.scroll
                        .scroll_to_item((step * 151) % rows, gpui::ScrollStrategy::Top);
                });
            }
            window.refresh();
            // free the element arena of this frame, as the platform loop does after each draw
            window.draw(cx).clear(cx);
        });
        times.push(started.elapsed());
        cx.run_until_parked();
    }
    times.sort();
    let pct = |p: usize| times[(times.len() - 1) * p / 100];
    eprintln!(
        "scroll bench: {} frames, median {:?}, p95 {:?}, max {:?}",
        times.len(),
        pct(50),
        pct(95),
        times[times.len() - 1]
    );
}
```

`crates/ig-app/src/lib.rs`:

```rust
//! IdeaGit's application: windows, actions, the Git tool window (Log and Console) and the log
//! worker. Built on `ig-git` / `ig-log` for Git and `ig-ui-kit` for the IntelliJ New UI look.
//!
//! Every module names the upstream sources it ports or imitates; see also
//! `reference/upstream-map.toml`.

pub mod actions;
pub mod app;
pub mod console;
pub mod log;
pub mod logging;
pub mod menus;
pub mod paths;
pub mod project;
#[cfg(feature = "screenshots")]
pub mod screenshots;
pub mod settings;
pub mod strings;
pub mod welcome;

use std::path::PathBuf;

use gpui::App;

use crate::actions::{About, Exit, OpenFile};

/// Registers IdeaGit's globals, theme, fonts, actions, keymap and menus.
pub fn init(cx: &mut App) {
    gpui_component::init(cx);
    if let Err(e) = ig_ui_kit::init(cx.window_appearance(), cx) {
        ::log::error!("loading fonts: {e}");
    }
    app::AppState::init(cx);
    actions::bind_keys(cx);
    cx.on_action(|_: &OpenFile, cx| welcome::prompt_open(cx));
    cx.on_action(|_: &Exit, cx| cx.quit());
    cx.on_action(|_: &About, cx| show_about(cx));
    cx.on_action(|action: &menus::OpenRecent, cx| {
        let path = app::AppState::get(cx)
            .settings
            .recent_projects
            .get(action.index)
            .cloned();
        if let Some(path) = path {
            app::open_project(&path, cx);
        }
    });
    menus::install(cx);
    cx.on_window_closed(|cx, _| app::on_window_closed(cx))
        .detach();
}

fn show_about(cx: &mut App) {
    let Some(window) = cx
        .active_window()
        .or_else(|| cx.windows().into_iter().next())
    else {
        return;
    };
    let detail = format!(
        "Version {}\nA Rust port of IntelliJ IDEA's Git features (intellij-community {}).",
        env!("CARGO_PKG_VERSION"),
        &crate::UPSTREAM_COMMIT[..7]
    );
    let _ = window.update(cx, |_, window, cx| {
        // the answer isn't needed: the dialog only has OK
        drop(window.prompt(
            gpui::PromptLevel::Info,
            "IdeaGit",
            Some(&detail),
            &["OK"],
            cx,
        ));
    });
}

/// The upstream commit the port matches (`reference/UPSTREAM`).
pub const UPSTREAM_COMMIT: &str = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98";

/// `ideagit [path]`: opens `path`, or the welcome window.
pub fn run(path: Option<PathBuf>) {
    gpui_platform::application()
        .with_assets(ig_ui_kit::icons::Assets)
        .run(move |cx: &mut App| {
            init(cx);
            match &path {
                Some(path) => app::open_project(path, cx),
                None => welcome::open(cx),
            }
            cx.activate(true);
        });
}
```

`crates/ig-app/src/main.rs`:

```rust
//! `cargo run -p ig-app -- [path]`: opens a folder directly, or the welcome window.

fn main() {
    ig_app::logging::init();
    let mut args = std::env::args_os().skip(1);
    #[cfg(feature = "screenshots")]
    {
        let all: Vec<_> = std::env::args_os().skip(1).collect();
        if all.first().is_some_and(|a| a == "--screenshots") {
            let out = all
                .get(1)
                .map(std::path::PathBuf::from)
                .expect("--screenshots <out dir> <repo>");
            let repo = all
                .get(2)
                .map(std::path::PathBuf::from)
                .expect("--screenshots <out dir> <repo>");
            std::process::exit(ig_app::screenshots::run(&out, &repo));
        }
    }
    let path = args.next().map(std::path::PathBuf::from);
    ig_app::run(path);
}
```

- [ ] **Step 2: Reference screenshots** (PNG; copy from the branch).

```bash
git checkout origin/wip/app-shell-prototype -- docs/ui-reference
```

- [ ] **Step 3: Render once (macOS, by hand).** Expected: `ideagit-log-light.png` and `ideagit-log-dark.png` that look like `docs/ui-reference/ideagit-log-{light,dark}.png`. Needs a GPU; skip it in a headless environment and say so.

```bash
cargo xtask screenshots --out $CLAUDE_JOB_DIR/tmp/shots   # or any scratch dir
```

- [ ] **Step 4: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code origin/wip/app-shell-prototype -- crates/ig-app/src/screenshots.rs crates/ig-app/src/lib.rs crates/ig-app/src/main.rs docs/ui-reference
```

Expected: no output. The intermediate files written above differ from the branch until their final task.

- [ ] **Step 5: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 460 passed, 1 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(ig-app): headless Log screenshots (cargo xtask screenshots)"
```

---

### Task 11: Module map, README, manual checklist, full verification

**Files:**
- Create: `docs/app-shell-checklist.md`
- Modify: `reference/upstream-map.toml`, `README.md`

- [ ] **Step 1: `reference/upstream-map.toml`** adds the `ig-ui-kit`, `ig-app` and `xtask` entries and the deviation lines for `is_terminal` and `silent_command`.

```bash
git checkout origin/wip/app-shell-prototype -- reference/upstream-map.toml
```

`reference/upstream-map.toml`:

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
  "runCommandWithoutCollectingOutput takes a line-listener closure (GitLineHandlerListener); stdin input is a byte buffer written then closed (setInputProcessor / writeLines)",
]

[modules."ig-git::console"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/commands/GitImplBase.java#GitCommandOutputLogger"]
deviations = ["ConsoleSink::silent_command reports silent (read) command lines for IdeaGit's console debug toggle; upstream never logs them"]

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
deviations = ["Branch names compare case-sensitively (upstream: case-insensitive on case-insensitive file systems)", "Remote branches compare by remote name and branch name (upstream also compares urls/refspecs); no ordering defined yet"]

[modules."ig-git::reader"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/repo/GitRepositoryReader.kt"]
tests = ["plugins/git4idea/tests/git4idea/repo/GitRepositoryReaderTest.kt", "plugins/git4idea/testData/repo"]
deviations = [
  "Hooks info and shallow detection not yet ported",
  "Reftable: CHERRY_PICK_HEAD/REVERT_HEAD also checked via rev-parse (upstream only checks files)",
  "Reftable: current branch asked from git whenever no ref is HEAD (upstream only without local branches)",
  "Sequencer todo read as bytes; current-branch case fix uses Unicode lowercase",
]

[modules."ig-git::roots"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-impl/src/com/intellij/openapi/vcs/roots/VcsRootDetectorImpl.java",
  "plugins/git4idea/backend/src/roots/GitRootChecker.java",
]
deviations = ["Simplified: enclosing root below the user home + bounded downward scan; skips a fixed list of tool/cache dirs instead of .gitignore'd dirs"]

[modules."ig-log::graph_commit"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/GraphCommit.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/AbstractGraphCommit.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/GraphCommitImpl.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/IntGraphCommit.java",
]
deviations = ["Int-specialised commit classes not needed; GraphCommitImpl is generic over the id"]

[modules."ig-log::permanent_graph"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/PermanentGraph.kt"]
deviations = [
  "create_visible_graph returns Result and fails with UnsupportedGraphOptions for Bek, LinearBek, FirstParent and branch/text filters (Plan 2b)",
  "all_commits returns a lazy AllCommits view; the in-branch predicate is a ContainedInBranchCondition",
]

[modules."ig-log::visible_graph"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/VisibleGraph.kt",
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/RowInfo.java",
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/RowType.java",
]
deviations = ["get_row_info returns a boxed RowInfo borrowing the visible graph"]

[modules."ig-log::print_element"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/PrintElement.kt",
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/NodePrintElement.java",
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/EdgePrintElement.kt",
]

[modules."ig-log::graph_color_manager"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/GraphColorManager.java"]

[modules."ig-log::actions"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/actions/ActionController.java",
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/actions/GraphAction.kt",
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/actions/GraphAnswer.java",
]
deviations = [
  "AWT Cursor becomes Cursor::{Default, Hand}",
  "The affected element is always a GraphPrintElement (no lookup of foreign PrintElements by equality)",
  "Runnable graph updater becomes GraphUpdater, run through VisibleGraphImpl::run_graph_updater",
]

[modules."ig-log::api"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/EdgeFilter.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/GraphLayout.java",
]

[modules."ig-log::api::elements"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/elements/GraphElement.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/elements/GraphNode.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/elements/GraphNodeType.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/elements/GraphEdge.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/elements/GraphEdgeType.java",
]
deviations = ["GraphElement is an enum over GraphNode and GraphEdge"]

[modules."ig-log::api::linear_graph"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/LinearGraph.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/LiteLinearGraph.java",
]

[modules."ig-log::api::permanent"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/permanent/PermanentCommitsInfo.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/permanent/PermanentGraphInfo.java",
]
deviations = ["PermanentGraphInfo.linear_graph returns an Arc handle"]

[modules."ig-log::api::printer"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/printer/GraphColorGetter.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/printer/GraphPrintElement.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/printer/PrintElementGenerator.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/printer/PrintElementPresentationManager.java",
]
deviations = ["GraphPrintElement is an enum over the two concrete print element types"]

[modules."ig-log::utils"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/TimestampGetter.java"]

[modules."ig-log::utils::flags"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/Flags.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/BitSetFlags.java",
]
tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/BitSetFlagsTest.java"]

[modules."ig-log::utils::normal_edge"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/NormalEdge.java"]

[modules."ig-log::utils::linear_graph_utils"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/LinearGraphUtils.java"]
deviations = ["DEFAULT_GRAPH_ANSWER is the function default_graph_answer()"]

[modules."ig-log::utils::dfs_util"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/DfsUtil.kt"]

[modules."ig-log::utils::int_int_multi_map"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/IntIntMultiMap.java"]
tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/IntIntMultiMapTest.kt"]

[modules."ig-log::utils::java_sort"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = []
deviations = [
  "IdeaGit helper replacing java.util.TimSort (Collections.sort, print element generator) / fastutil stableSort (IntArrayList.sort, layout builder): exact TimSort small-array path below 32 elements, stable merge sort above; never panics on inconsistent comparators",
  "Consistent comparators (strict weak orders): result identical to any stable sort, including TimSort and fastutil stableSort",
  "Inconsistent comparators: matches TimSort exactly below 32 elements; at 32 and above it can differ from Java, and Java may throw 'Comparison method violates its general contract'",
]

[modules."ig-log::utils::bounded_cache"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = []
deviations = ["IdeaGit helper replacing com.intellij.util.containers.SLRUMap with FIFO eviction; only memoises pure computations"]

[modules."ig-log::permanent::duplicate_parent_fixer"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/DuplicateParentFixer.java"]
deviations = ["Each commit is fixed once (Cow) instead of on every access of a lazy list"]

[modules."ig-log::permanent::permanent_linear_graph_builder"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentLinearGraphBuilder.java"]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphBuilderTest.kt",
  "platform/vcs-log/graph/testData/graphBuilder",
]
deviations = [
  "Bug fix: an edge to a not-loaded parent is written to that parent's slot, not the first free slot (upstream swaps parents when several are not loaded and processed out of order)",
  "Not-loaded parents sharing a smallest child are ordered by first appearance; Rust HashMap order is random per run (Java's is arbitrary but reproducible), so this keeps synthetic ids deterministic",
]

[modules."ig-log::permanent::permanent_linear_graph_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentLinearGraphImpl.java"]
tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphBuilderTest.kt"]

[modules."ig-log::permanent::graph_layout_builder"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutBuilder.kt"]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutBuilderTest.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutBuilderHeadOrderTest.kt",
  "platform/vcs-log/graph/testData/layoutBuilder",
]
deviations = [
  "branches is an ordered slice (upstream: Set in hash order; matters only for comparator ties)",
  "No sortCatching: Rust comparators cannot throw",
]

[modules."ig-log::permanent::graph_layout_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutImpl.kt"]

[modules."ig-log::permanent::permanent_commits_info_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentCommitsInfoImpl.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/RowsMapping.kt",
]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphBuilderHashIndexTest.java",
  "platform/vcs-log/graph/testData/graphHashIndex",
]
deviations = [
  "No Int-specialised storage",
  "get_commit_id panics for an unknown negative id (upstream returns null)",
  "Plan 2c: get_node_id, convert_to_node_ids and contains_all use hash indexes (commit id -> first row, extra rows of duplicate ids, not-loaded id) instead of linear scans (upstream TODO 'optimize with special map'); results unchanged",
]

[modules."ig-log::print::edges_in_row_generator"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/EdgesInRowGenerator.java"]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/print/EdgesInRowTest.java",
  "platform/vcs-log/graph/testData/edgesInRow",
]
deviations = [
  "BTreeSet instead of HashSet (deterministic order); the print generator's edge comparator is not a strict weak order, so ties can resolve differently from IntelliJ in rare rows (mostly rows with not-loaded commit edges) and positions there can differ",
  "BoundedCache instead of SLRUMap",
  "Upstream limitation kept: an edge spanning more than about 2 * walk_size rows is not reported for rows far from both of its ends; get_edges_in_row is not an exact edge index",
]

[modules."ig-log::print::elements"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/elements/PrintElementBase.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/elements/SimplePrintElementImpl.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/elements/EdgePrintElementImpl.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/elements/TerminalEdgePrintElement.java",
]
deviations = [
  "EdgePrintElementImpl::is_terminal exposes TerminalEdgePrintElement-ness (upstream: instanceof) for the painter; equality ignores it",
  "color_id and is_selected are computed when the element is created, not on every access",
  "TerminalEdgePrintElement is the constructor EdgePrintElementImpl::terminal",
]

[modules."ig-log::print::graph_element_comparator_by_layout_index"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/GraphElementComparatorByLayoutIndex.java"]

[modules."ig-log::print::graph_color_getter"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/GraphColorGetterByHead.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/GraphColorGetterByNode.kt",
]

[modules."ig-log::print::print_element_generator_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/PrintElementGeneratorImpl.kt"]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/print/PrintElementGeneratorTest.kt",
  "platform/vcs-log/graph/testData/elementGenerator",
]
deviations = [
  "Owns its presentation manager (upstream shares it with VisibleGraphImpl)",
  "BoundedCache instead of SLRUMap",
  "Upstream bug fixed: recommended_width clamps the variance with max(0.0) before sqrt; upstream returns 0 for linear histories (negative variance, sqrt is NaN, NaN rounds to 0)",
  "In-row edges arrive in BTreeSet order instead of Java HashSet order; the edge comparator is not a strict weak order, so ties can resolve differently from IntelliJ in rare rows (mostly rows with not-loaded commit edges) and positions there can differ",
]

[modules."ig-log::collapsing::edge_storage"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/EdgeStorage.java"]
tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/EdgeStorageTest.kt"]

[modules."ig-log::collapsing::edge_storage_wrapper"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/EdgeStorageWrapper.java"]
tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/EdgeStorageTest.kt"]
deviations = [
  "Storage shared through Arc<RwLock<EdgeStorage>>; mutating methods take &self",
  "get_edges skips ids without a node index (upstream: NullPointerException)",
  "get_edges returns a BTreeSet (deterministic order; Java's HashSet order is deterministic, Rust's is not)",
]

[modules."ig-log::facade::base_controller"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/BaseController.java"]

[modules."ig-log::facade::graph_changes"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/GraphChanges.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/GraphChangesUtil.java",
]
deviations = ["Interfaces and *Impl classes collapse into structs; edges_replaced returns a Vec (upstream HashSet with identity equality)"]

[modules."ig-log::facade::linear_graph_controller"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/LinearGraphController.kt"]
deviations = ["compiled_graph returns an Arc handle; LinearGraphAction is GraphAction"]

[modules."ig-log::facade::linear_graph_wrapper"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/LinearGraphWrapper.kt"]

[modules."ig-log::facade::reachable_nodes"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/ReachableNodes.kt"]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/ContainingBranchesTest.java",
  "platform/vcs-log/graph/testData/containingBranches",
]
deviations = ["Mutex instead of synchronized"]

[modules."ig-log::facade::print_element_presentation_manager_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/PrintElementPresentationManagerImpl.kt"]

[modules."ig-log::facade::permanent_graph_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/PermanentGraphImpl.kt"]
deviations = [
  "Only Options.Base(Normal) without filters; others return UnsupportedGraphOptions (Plan 2b)",
  "Unfiltered Normal uses BaseController directly instead of CollapsedController over an all-visible CollapsedGraph (same compiled graph; no collapse/expand/fragment hover)",
  "Branch node ids sorted ascending before the layout builder",
  "No bekIntMap / SortedBaseController yet",
  "get_children returns an empty list for unknown or not-loaded commits (upstream throws)",
]

[modules."ig-log::facade::visible_graph_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/VisibleGraphImpl.kt"]
deviations = [
  "Implements ActionController itself (upstream inner class)",
  "BUTTON_COLLAPSE/BUTTON_EXPAND reported unsupported until CollapsedController (Plan 2b)",
  "buildSimpleGraphInfo / SimpleGraphInfo not ported",
  "Graph updater run via run_graph_updater",
  "Arrow actions on stale print elements (indices out of range / unknown not-loaded id) are ignored instead of throwing",
]

[modules."ig-log::tests::common"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/AbstractTestWithTextFile.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/AbstractTestWithTwoTextFile.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/GraphStrUtils.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/StrUtils.kt",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/TestGraphBuilder.kt",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/CommitIdManager.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/parser/CommitParser.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/parser/SimpleCommit.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/parser/EdgeNodeCharConverter.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/parser/LinearGraphParser.java",
]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/TestGraphBuilderTest.kt",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/print/TestGraphBuilderTest.kt",
  "platform/vcs-log/graph/testData/testGraphBuilder",
  "platform/vcs-log/graph/testData/testParser",
]
deviations = ["Test support code; asTestGraphString and TestPermanentGraphInfo not ported yet (needed by Plan 2b tests)"]

# ---- Plan 2c: git log data

[modules."ig-git::natural_comparator"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/util/base/src/com/intellij/openapi/util/text/NaturalComparator.java",
  "plugins/git4idea/shared/src/git4idea/GitReference.kt#REFS_NAMES_COMPARATOR",
]
deviations = ["Character case mapping uses Rust's char::to_uppercase/to_lowercase for single-unit results (Java Character.toUpperCase/toLowerCase)"]

[modules."ig-git::log::parser"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/history/GitLogParser.java", "plugins/git4idea/backend/src/GitFormatException.java"]
tests = ["plugins/git4idea/tests/git4idea/history/GitLogParserTest.kt"]
deviations = [
  "Lines are processed byte-wise (markers are ASCII control characters), not as UTF-16 chars",
  "Format errors that upstream logs with LOG.error in parse() are dropped",
  "Random marker tails use std RandomState instead of ThreadLocalRandom",
  "Test harness: record hashes are a deterministic 40-hex digest instead of SHA-1 of the Kotlin map; Change assertions go through GitChangesParser.parseChange's path mapping (Change objects not ported)",
]

[modules."ig-git::log::record"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/history/GitLogRecord.java",
  "plugins/git4idea/backend/src/history/GitLogFullRecord.java",
  "plugins/git4idea/backend/src/history/GitLogRecordBuilder.kt",
  "plugins/git4idea/backend/src/history/GitChangeType.java",
  "plugins/git4idea/backend/src/history/GitChangesParser.java#getChangeType",
]
deviations = [
  "Option values are a fixed array indexed by GitLogOption (upstream Map)",
  "No setUsedHandler (only used in log messages)",
  "GitLogFullRecord keeps --name-status entries; getFilePaths(root)/parseChanges (Change objects) are out of scope until commit details",
  "InternedGitLogRecordBuilder not ported (no path interning)",
  "commit_time/author_time_stamp return 0 for malformed values (upstream throws); millisecond conversion saturates",
]

[modules."ig-git::log::unescape"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-impl/src/com/intellij/vcsUtil/VcsFileUtil.java#unescapeGitPath", "plugins/git4idea/backend/src/GitUtil.java#unescapePath"]
deviations = ["Octal escapes decode as UTF-8 (upstream: default charset)"]

[modules."ig-git::log::splitter"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/history/GitLogOutputSplitter.java"]
deviations = [
  "run_and_split bundles runCommandWithoutCollectingOutput(...).throwOnError() and reportErrors()",
  "Keeps the first parse error (upstream keeps the last)",
]

[modules."ig-git::log::collector"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/history/GitLogRecordCollector.java"]
deviations = [
  "createEmptyCopy is a closure; errors upstream only logs are kept for take_error, and a missing tree hash counts as different",
  "Missing commit/parent tree hashes are recorded as errors and the commit skipped (upstream asserts, then may NPE); GitLogUnorderedRecordCollector not ported (compressed details only)",
]

[modules."ig-git::log::util"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/history/GitLogUtil.java",
  "plugins/git4idea/backend/src/log/GitLogProvider.kt#DONT_CONSIDER_SHA",
  "plugins/git4idea/backend/src/branch/GitBranchUtil.java#getAllTags",
]
tests = ["plugins/git4idea/tests/git4idea/history/GitLogUtilTest.kt"]
deviations = [
  "Only timed commits, metadata and refs; readFullDetails*/GitFullDetailsCollector/GitCommitRequirements (changes) are out of scope",
  "read_timed_commits takes no file paths (filters are out of scope)",
  "GitLogUtilTest: the two in-scope cases read through metadata instead of full details; the renames and merge-diff cases are out of scope",
  "git tag -l gets --no-column (upstream breaks with column.ui=always: multi-tag lines become bogus revisions)",
]

[modules."ig-git::log::ref_manager"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/log/GitRefManager.kt"]
tests = [
  "plugins/git4idea/tests/git4idea/log/GitBranchComparatorTest.kt",
  "plugins/git4idea/tests/git4idea/log/GitLabelComparatorTest.kt",
  "plugins/git4idea/tests/git4idea/log/GitRefManagerTestUtil.kt",
]
deviations = [
  "Ref types carry no colors (UI); compared by name and branch flag instead of identity",
  "Not ported: groupForTable, groupForBranchFilter (GitRefGroupsTest), favorites, serialize/deserialize",
  "The current branch comes from ig_git::log::repository_manager",
  "Adds snapshot-based label sorting (current branch read once per sort): Rust's sort may panic on a comparator whose state changes mid-sort",
]

[modules."ig-git::log::repository_manager"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/repo/GitRepositoryManager.java", "plugins/git4idea/backend/src/repo/GitRepositoryImpl.kt#update"]
deviations = [
  "Minimal stand-in: per-root GitBranchState, remotes and hash length (extensions.objectformat)",
  "State is updated explicitly by callers (no VFS events); fresh (empty) roots are re-read on access so their first commits appear",
  "Adds remove/retain_roots; unknown roots get an empty cached state (upstream returns null)",
]

[modules."ig-git::log::provider"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/log/GitLogProvider.kt",
  "plugins/git4idea/backend/src/log/VcsLogObjectsFactoryExt.kt",
  "plugins/git4idea/backend/src/GitUserRegistry.java",
  "plugins/git4idea/backend/src/config/GitConfigUtil.java#getValue",
]
tests = [
  "plugins/git4idea/tests/git4idea/log/GitLogProviderTest.kt",
  "plugins/git4idea/testFramework/testSrc/git4idea/test/RefParser.java",
  "plugins/git4idea/tests/git4idea/log/TestVcsRefsSequences.kt",
]
deviations = [
  "Data reading only: readFirstBlock, readAllHashes, readMetadata, getCurrentUser, getCurrentBranch, isFullHash, getReferenceManager",
  "Not ported: GitLogExperimentalProvider (experimental refs collection), getCommitsMatchingFilter (filter tests), readFullDetails (test full details), getContainingBranches, subscribeToRootRefreshEvents, resolveReference, getVcsRoot, debug data validation",
  "Branch refs in name order and set-like collections in first-seen order (upstream HashSet order); new unmatched tags are loaded in name order",
  "Upstream-inherited gap: an existing tag moved outside the first block keeps its old hash until a full reload (add_old_still_existing_tags)",
]

[modules."ig-log::data::hash"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-api/shared/src/com/intellij/vcs/log/Hash.java", "platform/vcs-impl/shared/src/com/intellij/vcs/log/impl/HashImpl.java"]
deviations = ["Separate from ig_git::hash::Hash (converted at the provider boundary); try_build returns an error instead of asserting"]

[modules."ig-log::data::commit"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/api/src/com/intellij/vcs/log/TimedVcsCommit.java",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsShortCommitDetails.java",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsCommitMetadata.java",
  "platform/vcs-log/api/src/com/intellij/vcs/log/CommitId.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/TimedVcsCommitImpl.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/VcsShortCommitDetailsImpl.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/VcsCommitMetadataImpl.java",
]

[modules."ig-log::data::vcs_user"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-api/shared/src/com/intellij/vcs/log/VcsUser.java",
  "platform/vcs-impl/shared/src/com/intellij/vcs/log/impl/VcsUserImpl.kt",
  "platform/vcs-impl/shared/src/com/intellij/vcs/log/util/VcsUserUtil.java",
]
deviations = ["Users are not interned (VcsUserRegistryImpl)"]

[modules."ig-log::data::vcs_ref"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsRef.java",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsRefType.java",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsRefsContainer.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/VcsRefImpl.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/EmptyRefs.kt",
]
deviations = ["VcsRefType has a name instead of a background color; names are not interned"]

[modules."ig-log::data::root"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/impl/src/com/intellij/vcs/log/util/VcsLogUtil.java#compareRoots"]
deviations = ["Roots are Arc<Path> instead of VirtualFile"]

[modules."ig-log::data::provider"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsLogProvider.kt",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsLogRefManager.java",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsLogProviderRequirementsEx.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/RequirementsImpl.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/SimpleLogProviderRequirements.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/LogDataImpl.java",
]
deviations = [
  "Synchronous; readRecentCommits/RefsLoadingPolicy not modelled (readFirstBlock is the default)",
  "VcsLogRefManager has the two comparators plus name() (upstream toString); grouping, favorites and serialization not ported",
  "Filters, full details, containing branches and refresh subscriptions not ported",
]

[modules."ig-log::data::storage"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogStorage.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/InMemoryStorage.java",
]
deviations = ["In memory only (no VcsLogStorageImpl); get_commit_indexes interns a commit and its parents under one lock"]

[modules."ig-log::data::refs_model"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/RefsModel.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogRefsOfSingleRootFactory.kt",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsLogAggregatedStoredRefs.kt",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsLogRootStoredRefs.kt",
]
deviations = ["get_ref_for_head_commit also caches 'no ref'; takes ref managers instead of providers"]

[modules."ig-log::data::sorter"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogSorter.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogJoiner.java#NewCommitIntegrator",
]
tests = ["platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogSorterTest.java", "platform/vcs-log/impl/test/com/intellij/vcs/log/TimedCommitParser.java"]
deviations = [
  "Picks the next start commit in input order (upstream: HashMap order). Results equal upstream for distinct, parent-monotone timestamps; with skewed timestamps ours is deterministic in input order while upstream is nondeterministic",
  "Insertion is O(n²) like upstream; used only by the refresh joiner on small batches",
]

[modules."ig-log::data::joiner"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogJoiner.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogRefreshNotEnoughDataException.java",
]
tests = ["platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogJoinerTest.kt"]
deviations = [
  "Exceptions are JoinError values; new commits are integrated in first-block order (upstream HashSet order)",
  "Duplicate ids in the first block: last copy wins everywhere (matches HashMap.put)",
]

[modules."ig-log::data::multi_repo_joiner"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogMultiRepoJoiner.java"]
tests = ["platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogMultiRepoJoinerTest.java"]
deviations = [
  "Commits with the same hash in different logs are all kept (upstream keys its iterator map by hash-equal commits and drops one log)",
  "Timestamp ties go to the later log in caller order; the refresher passes roots in VcsRoot (Path) order",
]

[modules."ig-log::data::graph_color_manager_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/graph/GraphColorManagerImpl.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/graph/HeadCommitsComparator.java",
]
deviations = [
  "Ref managers told apart by Arc identity and ordered by name() (upstream equals/toString); 'no refs at head' not logged",
  "A ref whose root has no ref manager is treated like a head without a ref (upstream would throw)",
]

[modules."ig-log::data::graph_data"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogGraphData.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogGraphDataFactory.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/DataPack.kt",
]
deviations = ["No OverlayData; an empty or failed pack has no permanent graph (upstream EmptyPermanentGraph)"]

[modules."ig-log::data::refresher"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogRefresherImpl.kt"]
tests = [
  "platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogRefresherTest.kt",
  "platform/vcs-log/impl/test/com/intellij/vcs/log/impl/TestVcsLogProvider.java",
]
deviations = [
  "Synchronous: no request channel/batching, progress, cancellation, overlay data or tracing; the caller serializes calls on a background thread",
  "VcsLogRefresherTest: the blocking/cancellation/batching cases (test_first_refresh_waits_for_full_log, test_two_immediately_consecutive_refreshes...) don't apply; the applicable part of test_reinitialize_makes_refresh_cancelled (initializing twice publishes nothing more) is covered in vcs_log_refresher_extra.rs",
  "Every commit list is checked for unique ids and topological order before building the graph (upstream builds silently); a failing joined log forces a full reload, a failing first block or full log gives an error data pack",
  "Roots are processed in path order (upstream HashMap order)",
  "Recent first-block metadata goes to an optional consumer (upstream VcsLogCommitDataConsumer.storeRecentDetails)",
  "initialize() is idempotent (upstream: lazy job start); refresh() before initialize() initializes and returns that result",
  "refresh() dedupes roots (BTreeSet, path order), like upstream's rootsToRefresh.toSet()",
]

[modules."xtask::theme"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/platform-impl/src/com/intellij/ide/ui/UIThemeBean.kt",
  "platform/platform-impl/src/com/intellij/ide/ui/UITheme.kt",
  "platform/platform-impl/src/com/intellij/ide/ui/uiThemeParser.kt",
  "platform/util/src/com/intellij/ui/ColorHexUtil.java",
  "platform/util/ui/src/com/intellij/ui/JBColor.java",
  "platform/editor-ui-ex/src/com/intellij/openapi/editor/colors/impl/AbstractColorsScheme.java",
  "platform/editor-ui-ex/src/com/intellij/openapi/editor/colors/impl/ValueElementReader.java",
]
deviations = [
  "Only os.default values are used (upstream prefers the running OS's os.* key)",
  "LaF defaults set in Java code/properties are absent; a theme without parentTheme has no parent (upstream: default light/dark parent)",
  "A missing color key falls back to the longest matching * pattern (upstream JBColor.findPatternMatch: first match in hash-map order)",
  "Scheme attributes follow the baseAttributes attribute (upstream: the TextAttributesKey fallback declared in code); OS-specific scheme values are ignored",
]

[modules."xtask::tokens"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/util/ui/src/com/intellij/util/ui/JBUI.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/graph/DefaultColorGenerator.kt",
  "platform/platform-api/src/com/intellij/ui/components/ScrollBarPainter.java",
]
deviations = ["Hardcoded upstream fallbacks are literals in assets/theme.keys, each citing its source"]

[modules."xtask::keymap"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/platform-impl/src/com/intellij/openapi/keymap/impl/KeymapImpl.kt",
  "platform/platform-impl/src/com/intellij/openapi/keymap/impl/MacOSDefaultKeymap.kt",
  "platform/platform-impl/src/com/intellij/openapi/keymap/impl/DefaultKeymap.kt",
]
deviations = [
  "Keystrokes keep the upstream text; Mac conversion swaps control/meta tokens instead of KeyStroke modifier masks",
  "use-shortcut-of bindings come only from the XML named in assets/keymap.list (upstream: ActionManagerEx.getActionBinding)",
  "Mouse and gesture shortcuts are ignored; INSERT keystrokes are dropped only for Mac keymaps (upstream: for all keymaps when running on macOS)",
]

[modules."xtask::icons"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/util/ui/src/com/intellij/ui/icons/ImageDescriptor.kt"]
deviations = ["Icons are picked explicitly in assets/icons.list (upstream maps classic paths through *IconMappings.json)"]

[modules."xtask::strings"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/util/src/com/intellij/AbstractBundle.kt"]
deviations = ["Values are copied raw (escapes are not decoded)"]

[modules."xtask::fonts"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts",
  "platform/platform-tests/testData/fonts/Inconsolata_LICENSE.txt",
]
deviations = ["Upstream ships no license next to these fonts; *-OFL.txt = the font's name-table copyright + the OFL 1.1 text from Inconsolata_LICENSE.txt"]

[modules."ig-ui-kit::palette"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/graph/DefaultColorGenerator.kt",
]
deviations = [
  "No color cache; the default color (JBColor.BLACK) is resolved by the theme",
  "java.awt.Color RGBtoHSB/HSBtoRGB ported inline; expected values were derived by hand, not run against a JVM",
]

[modules."ig-ui-kit::metrics"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/util/ui/src/com/intellij/util/ui/JBUI.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/paint/PaintParameters.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/render/LabelPainter.java",
  "platform/platform-impl/src/com/intellij/openapi/wm/impl/SquareStripeButtonLook.kt",
  "platform/platform-impl/src/com/intellij/openapi/wm/impl/customFrameDecorations/header/CustomHeader.kt",
]
deviations = [
  "Status bar height and widget padding are IdeaGit estimates (upstream derives them from font metrics)",
]

[modules."ig-ui-kit::theme"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/platform-resources/src/themes/expUI/expUI_light.theme.json",
  "platform/platform-resources/src/themes/expUI/expUI_dark.theme.json",
]
deviations = [
  "Tokens are generated by xtask::tokens; IjTheme also restyles gpui-component's theme",
  "Theme follows the window appearance (OS), no theme chooser",
]

[modules."ig-ui-kit::stripe"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/platform-impl/src/com/intellij/toolWindow/ToolWindowToolbar.kt",
  "platform/platform-impl/src/com/intellij/openapi/wm/impl/SquareStripeButton.kt",
  "platform/platform-impl/src/com/intellij/openapi/wm/impl/SquareStripeButtonLook.kt",
]
deviations = [
  "No drag and drop, more button or names under icons; icons are drawn as single-color masks",
]

[modules."ig-ui-kit::tool_window"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/platform-impl/src/com/intellij/toolWindow/ToolWindowHeader.kt",
  "platform/platform-impl/src/com/intellij/openapi/wm/impl/content/ContentTabLabel.java",
  "platform/platform-impl/src/com/intellij/openapi/wm/impl/content/TabContentLayout.java",
]
deviations = [
  "A DockAreaRenderer for gpui-base's dock; one title per renderer; no gear menu, view modes or tab drag between docks",
]

[modules."ig-ui-kit::status_bar"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/platform-impl/src/com/intellij/openapi/wm/impl/status/IdeStatusBarImpl.kt",
  "platform/platform-impl/src/com/intellij/openapi/wm/impl/status/TextPanel.kt",
]
deviations = [
  "Text widgets only; progress shown as text",
]

[modules."ig-ui-kit::notification"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/platform-api/src/com/intellij/ui/InlineBanner.kt",
  "platform/platform-api/src/com/intellij/ui/EditorNotificationPanel.java",
  "platform/platform-impl/src/com/intellij/notification/impl/NotificationsManagerImpl.java",
]
deviations = [
  "No balloon animation, timeout or event log",
]

[modules."ig-ui-kit::table"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/VcsLogGraphTable.java",
]

[modules."ig-app::log::graph_painter"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/paint/SimpleGraphCellPainter.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/paint/HeadNodePainter.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/paint/PaintParameters.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/render/GraphCommitCellUtil.kt",
  "platform/util/ui/src/com/intellij/ui/paint/PaintUtil.java#alignToInt",
]
deviations = [
  "Dash pattern has no phase (GPUI dash arrays)",
  "OUTLINE nodes use the ordinary line thickness",
]

[modules."ig-app::log::labels"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/render/LabelPainter.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/render/LabelIcon.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/render/TagPainter.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/render/GraphCommitCellRenderer.kt#getAvailableWidth",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/util/VcsLogUiUtil.java#shortenTextToFit",
]
deviations = [
  "No bookmarks; char counts instead of UTF-16 units; warning icon drawn as a single-color mask",
  "Right-aligned labels only (LABELS_LEFT_ALIGNED default false)",
]

[modules."ig-app::log::ref_groups"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/log/GitRefManager.kt#groupForTable",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/SimpleRefGroup.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/SingletonRefGroup.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/DetachedHeadRefGroup.kt",
]
deviations = [
  "Tracked groups found by remote/<local> name only (no branch tracking info yet)",
]

[modules."ig-app::log::date"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/platform-api/src/com/intellij/util/text/DateFormatUtil.java#formatPrettyDateTime",
]
deviations = [
  "Absolute format is the JDK en_US FormatStyle.SHORT; upstream follows the OS locale via DateTimeFormatManager",
]

[modules."ig-app::log::metadata"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/MiniDetailsGetter.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/TopCommitsCache.java",
]
deviations = [
  "One cache for top commits and loaded details",
]

[modules."ig-app::log::worker"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogRefresherImpl.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/MiniDetailsGetter.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/GraphTableModel.kt",
]
deviations = [
  "Two threads (refresher; snapshot+metadata) instead of upstream's coroutine jobs",
  "LIFO metadata queue keeps the newest 10 requests (SequentialLimitedLifoExecutor)",
]

[modules."ig-app::log::model"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/GraphCommitCellController.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/VcsLogGraphTable.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/GraphTableModel.kt",
]

[modules."ig-app::log::selection"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/VcsLogGraphTable.java",
]
deviations = [
  "Selection is kept as a set of commits, not rows restored after refresh",
]

[modules."ig-app::log::view"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/VcsLogGraphTable.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/render/GraphCommitCellRenderer.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/column/VcsLogDefaultColumn.kt",
]
deviations = [
  "Own table on GPUI uniform_list (spike: gpui-component's table is single-selection)",
  "Fixed column widths, no Root column, no current-branch highlighting, no tooltips; loading rows show Loading…",
]

[modules."ig-app::console"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/commands/GitCommandOutputConsolePrinter.kt",
  "plugins/git4idea/backend/src/util/GitVcsConsoleWriter.java",
]
deviations = [
  "No ANSI decoding; bounded line buffer; read commands shown with a debug toggle (IdeaGit addition)",
]

[modules."ig-app::actions"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/platform-resources/src/keymaps/$default.xml",
  "platform/platform-resources/src/keymaps/Mac OS X 10.5+.xml",
]
deviations = [
  "Only implemented actions are bound; table navigation uses the Editor* keymap ids",
]

[modules."ig-app::settings"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/platform-impl/src/com/intellij/ide/RecentProjectsManagerBase.kt",
  "plugins/git4idea/backend/src/config/GitVcsApplicationSettings.java",
]
deviations = [
  "One settings.toml instead of recentProjects.xml / git.xml",
]
```

- [ ] **Step 2: `README.md`**: status, the new crates, and run and build instructions. Write exactly:

````markdown
# IdeaGit

A cross-platform desktop Git client written in Rust that reproduces the Git features and user interface of
[IntelliJ IDEA](https://www.jetbrains.com/idea/). It is ported from the open-source
[IntelliJ IDEA Community](https://github.com/JetBrains/intellij-community) code base.

IdeaGit is an independent project. It is not affiliated with or endorsed by JetBrains. IntelliJ IDEA is a trademark
of JetBrains s.r.o.

## Status

Early development. The first runnable build opens a folder, shows IntelliJ's New UI look (theme, icons, fonts,
tool-window stripes) and browses the Git history in a read-only **Log** tab with IntelliJ's commit graph, next to the
**Git Console**. Light and dark themes follow the OS appearance.

| Crate | What it does | Ported from (IntelliJ Community) |
|---|---|---|
| [`ig-git`](crates/ig-git) | Runs the `git` CLI the way IntelliJ does (same flags, environment, locking and Git console events). Parses `git status`, refs and remotes, and reads repository state (merge, rebase, cherry-pick, revert, detached HEAD, worktrees, reftable). Detects repository roots. Reads the log (`git log` parser, ref manager, log provider). | [`plugins/git4idea`](https://github.com/JetBrains/intellij-community/tree/b3c1c1958bc81cc96ea882fd4beba72849c1ed98/plugins/git4idea) |
| [`ig-log`](crates/ig-log) | The Git Log commit graph: building, lane layout, and per-row print elements, colors and arrow actions. The VCS Log data layer: storage, refs model, sorter, joiners and refresher. | [`platform/vcs-log/graph`](https://github.com/JetBrains/intellij-community/tree/b3c1c1958bc81cc96ea882fd4beba72849c1ed98/platform/vcs-log/graph), [`platform/vcs-log/graph-api`](https://github.com/JetBrains/intellij-community/tree/b3c1c1958bc81cc96ea882fd4beba72849c1ed98/platform/vcs-log/graph-api), [`platform/vcs-log/impl`](https://github.com/JetBrains/intellij-community/tree/b3c1c1958bc81cc96ea882fd4beba72849c1ed98/platform/vcs-log/impl) |
| [`ig-ui-kit`](crates/ig-ui-kit) | The IntelliJ New UI look on [GPUI](https://www.gpui.rs/): generated theme tokens, `expui` icons, Inter and JetBrains Mono, New UI metrics, the graph palette, tool-window stripes and headers, status bar, notifications. | `platform/platform-resources` themes, `platform/icons`, `JBUI` |
| [`ig-app`](crates/ig-app) | The `ideagit` application: welcome and project windows, menus and keymap, the Git tool window with the Log (graph and label painters, background log worker) and the Console. | [`platform/vcs-log/impl`](https://github.com/JetBrains/intellij-community/tree/b3c1c1958bc81cc96ea882fd4beba72849c1ed98/platform/vcs-log/impl) UI, `GitConsole` |
| [`xtask`](xtask) | `cargo xtask import-ui` imports the UI resources from the pinned upstream commit; `cargo xtask screenshots` renders the Log to PNG. | |

Planned next: the Git Log UI (details, changes, filters, actions), Commit tool window, diff and merge, and branches,
fetch, pull and push. See the [design spec](docs/superpowers/specs/2026-11-08-ideagit-rust-design.md) and the
[implementation plans](docs/superpowers/plans).

## How the port relates to IntelliJ IDEA

IdeaGit tracks a pinned revision of the official sources so upstream fixes can be found and ported:

- **Upstream repository:** <https://github.com/JetBrains/intellij-community>
- **Pinned revision:** branch `263`, commit
  [`b3c1c19`](https://github.com/JetBrains/intellij-community/tree/b3c1c1958bc81cc96ea882fd4beba72849c1ed98), recorded in
  [`reference/UPSTREAM`](reference/UPSTREAM)
- **Module map:** [`reference/upstream-map.toml`](reference/upstream-map.toml) lists, for each Rust module, the upstream
  files it is ported from and the upstream tests it carries. It also lists every intentional deviation, such as fixed
  upstream bugs and Rust-specific adaptations.
- **Tests:** upstream unit tests are ported along with their test data, copied byte-identical (for example
  `GitVersionTest`, `GitRepositoryReaderTest`, and the VCS Log graph tests). Upstream fixture-based tests become
  integration tests against the real `git` executable.

Useful official references:

- IntelliJ IDEA Git documentation: <https://www.jetbrains.com/help/idea/using-git-integration.html>
- IntelliJ IDEA Community source code: <https://github.com/JetBrains/intellij-community>
- Git plugin (`git4idea`) sources:
  <https://github.com/JetBrains/intellij-community/tree/master/plugins/git4idea>
- VCS Log sources: <https://github.com/JetBrains/intellij-community/tree/master/platform/vcs-log>

## Building, running and testing

Requires Rust 1.95 or newer and `git` 2.17 or newer (2.19.2 on Windows). On Linux, GPUI needs the X11, Wayland,
Vulkan and fontconfig development packages; see `.github/workflows/ci.yml` for the list. On Windows it needs
`llvm-rc` (from LLVM).

Zed's gpui revision is pinned only in `Cargo.lock`, so always pass `--locked`:

```bash
cargo run --locked -p ig-app -- [path]   # opens a folder, or the welcome window
cargo test --locked --all
cargo clippy --locked --all-targets -- -D warnings
```

UI resources are imported from an intellij-community clone at the pinned commit with
`cargo xtask import-ui --upstream <clone>`. Before a release, run through
[`docs/app-shell-checklist.md`](docs/app-shell-checklist.md).

## License

Licensed under the [Apache License, Version 2.0](LICENSE). IdeaGit contains code ported from IntelliJ IDEA
Community Edition, Copyright JetBrains s.r.o. and contributors, also licensed under Apache 2.0. See [`NOTICE`](NOTICE)
for attribution.
````

- [ ] **Step 3: `docs/app-shell-checklist.md`**, the manual checklist from the spec. Write exactly:

```markdown
# App shell: manual checklist

Run these by hand on each platform before a release, and after changes to the app shell or the Log. They cover what
the headless tests can't: real windows, GPU rendering, OS integration and large histories.

Start the app with `cargo run --locked --release -p ig-app -- [path]`.

## Open a real repository

- [ ] `ideagit <repo>` opens a project window with the Git tool window on the **Log** tab.
- [ ] The graph, ref labels, author, date and hash columns match IntelliJ IDEA's Log for the same repository
      (compare with `docs/ui-reference/`).
- [ ] Click, Shift+click, Cmd/Ctrl+click and the arrow keys select rows as in IntelliJ.
- [ ] Hovering a long-edge arrow highlights it; clicking it jumps to and selects the target row.
- [ ] The **Console** tab shows the commands IdeaGit ran; **View → Show read commands in Console** also shows reads.

## Large history

- [ ] Open the 50k-commit repository from the `log_perf` test (or any large one). The first rows appear quickly, the
      full log follows, and scrolling stays smooth. Rows not yet loaded show "Loading…" briefly.

## Refresh

- [ ] Make a commit in a terminal, then focus the IdeaGit window. The Log shows the new commit.
- [ ] **Git → Refresh Log** (and its shortcut) refreshes.

## Theme

- [ ] Switch the OS between light and dark. The window follows.

## Multiple repositories

- [ ] Open a folder containing several repositories. All roots load; the status bar shows each root's branch.

## Welcome window

- [ ] `ideagit` with no path shows the welcome window with Open… and the recent projects; Clone is disabled.
- [ ] Opening and closing projects updates the recent list; reopening one works.
- [ ] The native menu (macOS) or the in-window menu (Windows, Linux) has File, View, Git and Help.

## Git missing

- [ ] With no `git` on `PATH` (or a git path setting pointing nowhere), the welcome window shows the git-missing
      banner. Setting a valid path fixes it.
```

- [ ] **Step 4: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code origin/wip/app-shell-prototype -- reference/upstream-map.toml
```

Expected: no output. The intermediate files written above differ from the branch until their final task.

- [ ] **Step 5: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 460 passed, 1 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "docs: upstream map, README and manual checklist for the app shell"
```

---

