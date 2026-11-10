# IdeaGit — Checkpoint for the next agent (2026-11-10, Plan 2b planned and simulated, ready to execute)

Read this first. It records where the project is, how the work has been done, and what comes next.

## What IdeaGit is

A cross-platform desktop Git client in **Rust + GPUI**. It reproduces IntelliJ IDEA's Git features and UI as closely as possible and is ported from the open-source `intellij-community` code. Upstream fixes are found and ported by hand; there is no merging.

- **Main spec:** `docs/superpowers/specs/2026-11-08-ideagit-rust-design.md`
- **App-shell spec:** `docs/superpowers/specs/2026-11-08-app-shell-design.md`
- **Technical debt:** `docs/tech-debt.md`
- **README:** `README.md` (build, run, crates)
- **Manual checklist:** `docs/app-shell-checklist.md`
- **Plan 2b spec:** `docs/superpowers/specs/2026-11-10-plan-2b-graph-extras-design.md`
- **Plan 2b plan:** `docs/superpowers/plans/2026-11-10-plan-2b-graph-extras.md`

## Repository state

- **`master`** on `origin` = https://github.com/replydev/IdeaGit. CI is green on Linux, macOS, Windows and the MSRV (1.95) job, including the GPUI build (last checked at `c816733`; the later commits are docs only). `master` is at `23a98c4` and matches `origin`.
- **Crates:**
  - `ig-git`: git CLI runner and console events; status, refs, config remotes, repository reader, root detection; `log` (parser, collector, `GitLogProvider`, `GitRefManager`).
  - `ig-log`: the IntelliJ commit graph (permanent graph, layout, print elements, facade) and `data` (storage, refs model, sorter, joiners, graph data, synchronous `VcsLogRefresher`).
  - `ig-ui-kit`: the IntelliJ New UI look on GPUI. Generated theme tokens, `expui` icons, Inter and JetBrains Mono, New UI metrics, graph palette, stripes, tool-window header (`IjDockRenderer` on gpui-component's dock), status bar, notifications.
  - `ig-app`: the `ideagit` binary. It has welcome and project windows, menus and keymap, the read-only Log (graph and label painters, background log worker, metadata cache, selection) and the Git Console. `cargo run --locked -p ig-app -- [path]`.
- **Project window layout** (the user's decision after manual testing, recorded as deviations in the spec and the upstream map):
  - The Log fills the main area, like IntelliJ's "Log in an editor tab". There is no editor area yet.
  - The Git Console is its own tool window: a left stripe button and a bottom dock, which starts hidden.
  - Hiding a tool window removes its dock entirely, with no leftover header strip, and showing it restores its height.
  - The Git stripe button and Cmd/Alt+9 focus the Log.
  - `xtask`: `cargo xtask import-ui --upstream ~/Code/intellij-community` (assets from the pinned commit) and `cargo xtask screenshots` (macOS only).
- **Tests:** 514 pass, plus 1 ignored perf test. They cover ported upstream tests (byte-identical fixtures), IdeaGit tests, and headless GPUI UI tests (`crates/ig-app/tests/ui.rs`, `TestAppContext`).
- **Reference map:** `reference/UPSTREAM` pins `intellij-community` branch `263` at commit `b3c1c1958bc81cc96ea882fd4beba72849c1ed98`. `reference/upstream-map.toml` maps every Rust module to its upstream files, tests and deviations. `#member` anchors and directory entries are intentional.
- **Plan generators:** `docs/superpowers/plangen/gen*.py`. `gen4.py` (Plan 3) holds the tasks as data. The same script renders the plan and applies a task to a scratch clone, which makes the simulation step exact.
  - `gen5.py` (Plan 2b) is simpler. Each task is one prototype commit, copied at that commit (tests first, then code). `sim5.sh` replays the tasks on a fresh clone and writes `sim5.json`: each task's red-step error and test counts.

## Plans

| Plan | Status |
|---|---|
| 1 — git core (`ig-git`) | Done, merged |
| 2 — commit graph core (`ig-log`) | Done, merged |
| 2c — git log data layer (`ig-log::data`, `ig-git::log`) | Done, merged |
| 3 — app shell and read-only Log (`ig-ui-kit`, `ig-app`, `xtask`) | Done, merged (PR #1) |
| 2b — graph extras (BEK, linear BEK, filters, collapsing, first-parent, fragment hover) | **Spec approved, prototype verified, plan generated and simulated. Ready to execute.** |
| Log UI, read-only (details pane, changes tree, `VcsLogFilterer` filters, View Options popup, non-modifying context actions) | Not started; comes after 2b. Repository-changing actions (checkout, new branch/tag, cherry-pick, revert, reset) moved to the Branches/operations plan; double-click diff waits for `ig-diff`. |
| Diff comparison (`ig-diff`, port of `com.intellij.diff.comparison` with upstream tests) | Not started |
| Change model + commit backend (changelists, line-status tracking, partial commit via temp index) | Not started |
| Commit tool window, diff viewer, basic editor | Not started |
| Branches popup, checkout/smart checkout, askpass, fetch/pull/push/update | Not started |
| Packaging (`xtask dist`), `xtask upstream-watch` | Not started |

Plan files are in `docs/superpowers/plans/`. Each one has "Not in this plan" / "Known gaps" and "Notes for …" sections that collect deferred review findings. **Read the notes in Plans 2c and 3 and `docs/tech-debt.md` before the Log UI and file-watching work.** Important open items include:

- **Tag-fetch refresh is slow.** One `git log` runs per new tag. This needs a refresh-strategy decision.
- **Some repo changes need a full reload.** The watcher must force one when `.git/shallow`, grafts or `refs/replace` change.
- **Runner cancellation and grandchild pipe hangs are still open.** Closing a window cancels the log worker between steps, but a running `git` is not killed.
- **Two `Hash` types** exist and should be unified into a leaf crate.
- **`Git` should be split** into an app-wide executable part and per-project parts.
- **The error contract** needs settling.
- **App-shell gaps:**
  - Log table: column resize, Root column, current-branch highlight, tooltips.
  - Dock resize: no `LayoutChanged` event, a closed dock doesn't reopen on drag.
  - Branch widget: no tag name or merge/rebase prefixes.
  - Root detection still runs on the UI thread.
- **Manual testing has started.** The user runs the app on real repositories and reports bugs. Fixed so far:
  - **Dark Log hover was near-white.** The hover color is mixed into the row background, as `VcsLogGraphTable.getHoveredBackgroundColor` does.
  - **A hidden dock left a header strip.** Hiding now removes the dock entirely.
  - **The Log was one row tall on Windows and Linux** (found by CI after `a2102b7`). gpui-component's `AppMenuBar` is `size_full` and took the whole column height. It now sits in a `flex_none` wrapper at its preferred height, as upstream's `IdeRootPane` lays it out (`c816733`).
  - **Linked worktrees were loaded as extra roots.** Worktrees under `.claude/worktrees` made the Log load the whole history once per worktree, so it was slow and every commit was duplicated. Root detection now skips dot-directories and `vendor`, as upstream's `vcs.root.detector.ignore.pattern` does.
- **Still not checked by eye:**
  - the welcome window
  - the native menus
  - the open Console dock

  Finish `docs/app-shell-checklist.md` on macOS.

## How the work is done (keep doing this)

1. **Brainstorm → spec**, using `superpowers:brainstorming`. Ask one question at a time and get user approval.
2. **Verified prototype first**, in a scratch clone. A strong-model subagent builds the full implementation plus tests. You then check fmt, clippy, docs, tests and fixtures yourself.
3. **Generate the plan from the prototype.** Model it on `docs/superpowers/plangen/gen4.py`: tasks as data, files copied with `git checkout <prototype ref> -- <paths>`, intermediate files written in full.
4. **Simulate the task order** on a fresh clone, with every step green. `gen4.py apply` plus a small driver script does this.
5. **Execute subagent-driven** on a feature branch:
   1. A cheap (haiku) implementer per task copies the files and runs the checks. It reports BLOCKED instead of editing code.
   2. Spec compliance: diff the commit's tree against the simulation's tree for that task.
   3. Quality review: sonnet for small modules, opus for core or concurrency-heavy ones. Point it at the upstream sources.
   4. Fixes run in **separate git worktrees** (`git worktree add -b fix/<x> <dir> <base>`) with their own `CARGO_TARGET_DIR`. Then cherry-pick them. This lets implementers keep going in the main tree in parallel.
   5. Fixers must keep public APIs source-compatible with files that later tasks copy verbatim. Check this by temporarily checking out those later files and running `cargo check`.
   6. Re-review if any finding was Important.
   7. Collect deviations as you go, and apply them in the plan's map task.
6. **CI before merge.** CI runs on pushes to master and on pull requests, so open a draft PR for the feature branch. Fix CI on the branch.
7. **Final whole-branch review** with opus. Fix blockers, then fast-forward merge to `master`, push, and watch CI.

Lessons from Plan 3:
- **zsh has no `PIPESTATUS`.** Don't pipe verify commands, or use `set -o pipefail`.
- **Check copies by staging first.** `git diff <ref>` ignores untracked files, so the exact-copy check is `git add -A && git diff --cached --exit-code <ref> -- <paths>`.
- **`cargo test` stops at the first failing test binary.** A red CI job can hide more failures.
- **Test fixtures need `-text`** in `.gitattributes`, otherwise Windows checks them out with CRLF.
- **Tests that race worker threads must be made deterministic** with test hooks set before the worker starts.

## Conventions

- **Faithful port.**
  - Keep upstream algorithms and quirks.
  - Fix only real bugs, and record each fix as a deviation in the module docs and `reference/upstream-map.toml`.
  - Upstream exceptions become panics only where unreachable from the UI. Public facade or UI paths must not panic on stale or unknown input.
- **Tests.**
  - Ported upstream tests stay faithful. Only minimal harness changes are allowed, and expected values never change.
  - IdeaGit-only tests are marked as such.
  - Tests are hermetic: `Git::with_base_environment` with an empty global config, `GIT_CONFIG_NOSYSTEM=1`, and the scrubbed `GIT_*` variables. UI tests use scratch settings through `AppState`'s git environment and settings path.
  - Never glob-import `gpui` in test modules: `#[gpui::test]` then crashes rustc.
- **Builds.**
  - Always pass `--locked`: the Zed rev is pinned only in `Cargo.lock`.
  - Locally, set `CARGO_NET_GIT_FETCH_WITH_CLI=true`.
  - Never run a plain `cargo update`.
- **Commits.**
  - Never add AI attribution or `Co-Authored-By` (user preference).
  - Commits are SSH-signed through 1Password. A transient "failed to fill whole buffer" error means retry; never disable signing. `%G?` shows `N` locally because no allowed-signers file is configured; that's expected.
- **Pushing as `replydev`.** The global git config rewrites `https://github.com/` to SSH as `lorenzopatane`, who has no write access. Push with:
  ```
  git -c credential.helper= -c 'credential.helper=!f(){ echo username=replydev; echo "password=$(gh auth token --user replydev)"; }; f' push origin <branch>
  ```
  Check CI with `GH_TOKEN=$(gh auth token --user replydev) gh run list -R replydev/IdeaGit`.

## Environment notes (this machine)

- **Upstream reference clone:** `~/Code/intellij-community`, a sparse, blob-less clone of branch `263` at `b3c1c19`. Read any file with `git show HEAD:<path>`.
- **Rust toolchains:** the default is 1.96. CI needs 1.95 (the MSRV). Extra toolchains (1.85, 1.93, 1.94, 1.97–1.99) can be removed with `rustup toolchain uninstall`.
- **Plan 2b prototype — keep until Plan 2b is merged:**
  - The local branch `wip/graph-extras-prototype` in this repository (not pushed). Its 8 commits are the plan's 8 tasks, and every `git checkout <commit> -- <paths>` in the plan needs them.
  - The scratch clone `/tmp/ig-proto-2b` holds the same branch.
  - The simulation clone `/tmp/ig-sim-2b` has one commit per task, the tree you should get after each task. It is useful for the spec-compliance diff.
- **Disposable:**
  - `/tmp/ig-proto` and `/tmp/ig-proto[0-9]*` (not `/tmp/ig-proto-2b`), `/tmp/ig-tasks*`, `/tmp/lec-shots`, `/tmp/ig-menubar-shots`.
  - The `*-target` build directories.
  - The `wip/app-shell-prototype` and `feat/app-shell` branches (both merged).

  Delete them once you no longer want to compare against them.
- **Quick visual check without a display:** `cargo xtask screenshots --repo <repo> --out <dir>` renders the window as it opens and prints when the Log was ready. It only works on macOS. Read the PNGs to check the layout.

## Next steps

1. **Execute Plan 2b** (`docs/superpowers/plans/2026-11-10-plan-2b-graph-extras.md`) subagent-driven, as in "How the work is done" step 5:
   - **Branch:** create `feat/graph-extras` from `master` (`23a98c4`).
   - **Implementer:** a haiku implementer per task, which follows the task's steps exactly and reports BLOCKED instead of editing code. Each task copies from its own prototype commit, runs the tests red (compile error) and then green, checks the copy is exact, runs the full checks, and commits.
   - **Expected counts:** 566, 570, 610, 621, 645, 650, 652 and 652 passing tests after tasks 1–8, with 1, 1, 2, 2, 2, 3, 3 and 3 ignored.
   - **Spec compliance:** after each task, the commit's tree must equal both the prototype commit named in the task and the matching `/tmp/ig-sim-2b` commit: `git diff <task commit> <sim commit>` must be empty.
   - **Quality review:**
     - sonnet for task 1 (utilities), task 2 (BEK sort) and task 8 (map);
     - opus for task 3 (collapsed graph: lock re-entrancy), task 4 (controllers, default-method downcasts, stale-element guard), task 5 (linear BEK, Java `PriorityQueue` fidelity), task 6 (controller chain against `PermanentGraphImpl.createFilteredController`, the `OnceLock` BEK map, the empty-repository fix) and task 7 (app click/refresh).
     - Point reviewers at the upstream sources under `platform/vcs-log/graph`.
   - **Fixes:** fixes go in separate worktrees and are cherry-picked. Keep APIs source-compatible with files later tasks copy verbatim. If a fix changes a later task's copied file, record the difference and update the expected tree for that task.
   - **Finish:** a draft PR for CI, a final opus review of the whole branch, then a fast-forward merge to `master`, push and watch CI. Afterwards, mark Plan 2b done here, add its notes for the Log UI plan, and delete `wip/graph-extras-prototype`, `/tmp/ig-proto-2b` and `/tmp/ig-sim-2b`.
2. **Decisions to look at closely in review** (from the prototype; all recorded in the plan's Deviations section):
   - **Clicks now collapse.** A click on a graph node collapses or expands its fragment, as in IntelliJ. The spec was updated to include this; the user had only asked for hover.
   - **Debug-only assertions.** `SortChecker` and BEK use `debug_assert!`, so a BEK ordering bug panics the worker in debug builds.
   - **Script-converted test.** The `LinearBekTest` port was converted by script. Its 20 tests' graph data matches upstream digit for digit.
3. **Keep fixing what the user finds** while testing by hand, and finish `docs/app-shell-checklist.md`. For each bug:
   - use `superpowers:systematic-debugging` and compare against the upstream source
   - write a failing test first
   - record any difference from IntelliJ in `reference/upstream-map.toml`
   - commit as separate logical commits
   - push to `master` only when the user asks
4. **After Plan 2b, brainstorm the read-only Log UI plan** with the user:
   - details, changes tree, `VcsLogFilterer` filters, the View Options popup, and the Log table gaps;
   - first read the notes in Plans 2c and 3, which include the tag-fetch refresh decision and the forced full reload for watched repo changes.
