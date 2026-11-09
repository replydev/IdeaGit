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
