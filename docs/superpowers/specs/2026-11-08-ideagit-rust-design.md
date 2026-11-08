# IdeaGit — IntelliJ-style Git client, ported to Rust + GPUI

Date: 2026-11-08
Status: Approved design, pending user review of this document
Supersedes: `2026-11-08-ideagit-standalone-design.md` (Java/IntelliJ Platform approach, abandoned)

## Goal

Build a cross-platform (Windows, macOS, Linux) desktop Git client in **Rust** that closely reproduces
IntelliJ IDEA's Git features and UI. Use the open-source `intellij-community` code as the porting
source, and keep a traceable mapping so upstream fixes can be found and ported quickly.

Upstream fixes cannot be *merged*. They are *ported by hand*, with tooling that shows exactly which
upstream commits are relevant.

## Scope

Full target feature set, delivered in slices:

- Commit tool window: changelists, partial (chunk/line) commits, amend, message history
- Git Log: graph, filters, details, cherry-pick, revert, reset, undo/edit commit,
  interactive rebase (drop, squash, fixup, reword, reorder)
- Branches popup; tags; remotes
- Smart checkout / merge / rebase; fetch, pull, push, update project
- Merge conflicts dialog and 3-way merge window with magic (auto) resolve
- Diff viewers, file history, blame
- Stash and shelve
- Git console (command log)
- Basic file editing
- Multi-repo and worktrees

Out of scope: GitHub/GitLab integration, Local History, IDE features unrelated to Git.

### Slices

1. **Slice 1 (this spec's implementation target):** app shell, Git Log, Commit tool window, Git console,
   branches popup + checkout/smart checkout, fetch/pull/push/update, read-only diff, basic editor.
2. **Slice 2:** conflicts dialog, 3-way merge window with magic resolve, merge/rebase/cherry-pick
   conflict flows, interactive rebase, undo/edit commit.
3. **Slice 3:** stash/shelve UI, file history, blame, remotes/tags management, worktrees UI.
4. **Slice 4:** polish, signed installers, auto-update.

Slices 2–4 each get their own spec addendum and implementation plan.

## Key decisions

| Decision | Choice | Reason |
|---|---|---|
| Language | Rust | User requirement. |
| UI toolkit | GPUI (+ `gpui-component`) | GPU-rendered and built for editor-class apps (Zed): fast text, virtualized lists, cross-platform. IntelliJ itself draws custom widgets, so a self-drawn toolkit makes close visual fidelity possible. Risk: pre-1.0 API churn, mitigated by pinning the git revision. |
| Git backend | `git` CLI, like IntelliJ, including the log (`gix` was considered for fast reads, but plain `git log` was fast enough). | Same behavior, hooks, credential helpers, and config as IntelliJ. Command builders and parsers port almost 1:1, and the Git console is accurate by construction. |
| Port style | Hybrid: logic ported faithfully, platform/UI idiomatic Rust | Logic stays diffable against upstream for fix-porting. Swing/platform idioms don't map to GPUI. |
| Upstream tracking | Pinned stable branch commit (e.g. `idea/253.x`) + per-module file map | Makes "which upstream fixes affect us" a mechanical query. |
| Window model | IDE-like: welcome screen, one window per opened folder, tool windows docked around a central editor/diff area | Matches IntelliJ. |
| Minimum git | Same as upstream: 2.17.0 (2.19.2 on Windows) | Same parser assumptions. |

## Licensing and branding

- Upstream's root `LICENSE.txt` now holds the "JetBrains Open-Source Build Terms" (v1.3, June 2026), which
  govern JetBrains' binary builds of IntelliJ IDEA and PyCharm. The source code itself is Apache 2.0 (file
  headers; the terms themselves say the builds consist of Apache-2.0 software). IdeaGit ports source only and
  ships the Apache-2.0 text plus upstream's NOTICE contents. *To be confirmed by legal.*
- `intellij-community` is Apache 2.0. Ported files carry a header crediting the upstream file(s). The
  repository includes the upstream `LICENSE` and a `NOTICE` file listing ported/imported material.
- Imported assets (themes, icons, strings, keymaps) are Apache 2.0 with the same attribution.
  Fonts (Inter, JetBrains Mono) are OFL.
- The product is named **IdeaGit** for now. No JetBrains or IntelliJ logos, product icons, or names are
  used as branding.

## Architecture

```
IdeaGit/                     (Cargo workspace)
├── crates/
│   ├── ig-git/              # git CLI runner, console events, askpass server, parsers,
│   │                        # repository model/reader/updater, repo detection, git log provider
│   ├── ig-vcs/              # change model: status, changelists, line status / partial-commit tracking
│   ├── ig-log/              # commit data, filters, graph (faithful port of vcs-log/graph)
│   ├── ig-diff/             # comparison algorithms (ByLine/ByWord), fragments; later 3-way merge
│   ├── ig-ops/              # high-level operations: checkout/smart checkout, branch ops,
│   │                        # commit workflow, fetch/pull/push/update
│   ├── ig-ui-kit/           # theme, icons, metrics, docking tool windows, popups, trees, tables,
│   │                        # notifications, progress (on gpui + gpui-component)
│   ├── ig-editor/           # text, diff, and (later) merge editor views; gutters; tree-sitter highlighting
│   ├── ig-askpass/          # tiny binary used as GIT_ASKPASS / SSH_ASKPASS
│   └── ig-app/              # main(), windows, action registry + keymap, settings, tool windows, welcome
├── assets/                  # generated by import-ui: themes, icons, strings, keymaps (+ fonts)
├── reference/
│   ├── UPSTREAM             # pinned intellij-community branch + commit
│   └── upstream-map.toml    # Rust module -> upstream files + per-module pinned commit
├── xtask/                   # cargo xtask: import-ui, upstream-watch, dist
└── docs/
```

**Dependency rule:** `ig-app` → `ig-ui-kit`/`ig-editor` → `ig-ops` → `ig-git`/`ig-vcs`/`ig-log`/`ig-diff`.
Logic crates (`ig-git`, `ig-vcs`, `ig-log`, `ig-diff`, `ig-ops`) **do not depend on GPUI** and are fully
testable headless.

**Threading:** git processes and heavy computation run on a background executor (the counterpart of
`Task.Backgroundable` with progress indicators and cancellation). Results go to GPUI entities on the
main thread. Long-running operations show progress in the status bar and can be cancelled.

**Persistence:** global settings live in the OS config dir (`settings.toml`, recent projects). Per-project
state (changelists, message history, tool-window layout) lives in `<repo-root>/.ideagit/workspace.toml`,
which is added to `.git/info/exclude` automatically.

## UI fidelity

`cargo xtask import-ui --clone <intellij-community>` copies these from the pinned commit into `assets/`
(rerun on every upstream bump):

| Asset | Upstream source | Use |
|---|---|---|
| Theme colors | New UI (`expui`/Islands) light and dark `*.theme.json`; editor color schemes for diff/merge/VCS colors | Generated into a Rust `Theme` |
| Icons | `platform/icons/src/expui/**.svg` and Git/VCS/diff SVGs, including dark variants | Embedded SVGs |
| Strings | `GitBundle`, `VcsBundle`, `DiffBundle`, `VcsLogBundle` `.properties` | Same UI text; supports localization |
| Keymaps | `$default.xml`, `Mac OS X 10.5+.xml` (relevant action IDs only) | Same shortcuts per OS |

- **Metrics** (insets, row heights, paddings) are hand-copied from `JBUI.CurrentTheme.*` / New UI sources
  into `ig-ui-kit::metrics`, with a source reference per constant.
- **Action IDs:** the action registry and keymaps use IntelliJ action IDs (`Git.Branches`, `Vcs.Push`,
  `ChangesView.Commit`, …). This keeps upstream lookup straightforward.
- **Behavioral components** (tool-window stripes/docking, speed search, branches popup, checkbox trees)
  are new implementations. Each component's doc comment names the upstream Swing class(es) it mimics.
- **Visual review:** a headless screenshot harness renders key views against fixture repos. Reference
  screenshots of IntelliJ IDEA Community (same fixtures and theme) live in `docs/ui-reference/` for
  side-by-side review. Comparison is visual, not a pixel diff.

## Slice 1: features and upstream sources

Paths are relative to `intellij-community/`. Every item gets an entry in `upstream-map.toml`.

### Foundation

- **Command runner** (`ig-git`): port of `git4idea/commands/{GitHandler, GitLineHandler, GitImplBase,
  GitCommand}`, `GitImpl` wrappers, and environment setup (`GitConfigUtil`, locale/`LANGUAGE` handling).
  It emits structured events (command line, stdout/stderr lines, exit code, duration). All git
  invocations go through it.
- **Git console:** subscribes to runner events and renders them like IntelliJ's VCS console (command line
  vs. output styling, timestamps, per-project tab).
- **Credentials:** `ig-askpass` acts as `GIT_ASKPASS`/`SSH_ASKPASS` and connects to the app over a local
  socket with a per-session token (mirroring `GitAskPassApp`, `GitNativeSshAskPassApp`, and the
  handler-token handshake). The app shows login/passphrase dialogs and can store secrets in the OS
  keychain (`keyring`). Configured git credential helpers still take precedence.
- **Repository model:** port of `GitRepositoryImpl`, `GitRepositoryReader` (HEAD, refs, packed-refs,
  merge/rebase/cherry-pick state read directly from `.git`), `GitRepositoryUpdater` (`.git` watching via
  `notify`), and `GitUntrackedFilesHolder`. Root detection and multi-repo/worktree discovery follow
  `GitRepositoryManager` / `VcsRootDetector`.
- **Change model** (`ig-vcs`): port of `GitChangeProvider` / `GitStagingAreaHolder` (`status
  --porcelain -z`, format v1 as upstream), `ChangeListManagerImpl` semantics (named changelists, default list, persistence),
  and `LineStatusTracker` / `PartialLocalLineStatusTracker` (hunk tracking, editor gutter markers,
  per-hunk inclusion for partial commits).

### Git Log

- **Data:** commits, refs and metadata are loaded via `git log`, using IntelliJ's format and parsing
  (`GitLogProvider`, `GitLogUtil`, `GitLogParser`, `GitLogRecord`), as upstream does. `gix` is not needed
  for this: in Plan 2c a full load of 50k commits took about 240 ms, almost all of it in `git log` itself.
  Filters (branch, user, date, path, text/regex, hash) follow `VcsLogFilterer` /
  `VcsLogFilterCollection` semantics.
- **Graph:** faithful port of `platform/vcs-log/graph` (`PermanentGraphImpl`, `GraphLayoutBuilder`,
  `LinearGraph`, `CollapsedGraph`, `PrintElementGenerator`, edge/arrow printing, color assignment).
  Rendered by a custom GPUI element.
- **UI:** virtualized table (graph+subject, author, date, hash) with ref labels; changes tree for the
  selected commit(s); details pane; double-click opens a read-only diff. Context actions: copy hash,
  checkout, new branch, new tag, cherry-pick, revert, reset (soft/mixed/hard). Cherry-pick/revert
  conflicts in slice 1 stop with a notification ("resolve conflicts"); the merge UI arrives in slice 2.

### Commit tool window

- Changes tree grouped by changelist, then directory, with tri-state inclusion checkboxes; unversioned
  and ignored nodes; show diff.
- Commit message editor with history, amend, author override, sign-off, Commit and Commit-and-Push.
- Partial commits: per-hunk/line inclusion in the diff view, committed through a temporary index as
  `GitCheckinEnvironment` does, so the user's own staging area is left untouched.
- Sources: `ChangesViewCommitPanel`, `ChangesViewCommitWorkflow`, `GitCheckinEnvironment`, commit
  options panels.

### Branches popup and operations

- New UI branches tree popup (`GitBranchesTreePopup*`, `GitBranchesTreeModel`): speed search,
  recents/favorites, local/remote groups, per-branch actions (checkout, new branch from, checkout and
  rebase onto current, compare with current, rename, delete, push).
- Behavior from `GitBranchWorker`, `GitCheckoutOperation`, `GitDeleteBranchOperation`, and smart
  checkout (`GitSmartOperationDialog`, `GitPreservingProcess`: stash → checkout → unstash). If unstash
  conflicts in slice 1, the app shows a notification and keeps the stash. Full resolution comes in slice 2.

### Fetch / Pull / Push / Update

- Fetch: `GitFetchSupportImpl`.
- Pull: `GitPullDialog` (merge/rebase and options).
- Update Project: `GitUpdateProcess` (merge or rebase per settings), with an updated-files summary.
- Push: `VcsPushDialog`, `GitPushSupport`, `GitPushOperation` (commit tree, target editing,
  force-with-lease, push tags).

### Shared infrastructure built in slice 1

Docking tool windows with stripes; action registry and keymap; notifications (balloons and an event log);
status-bar progress; settings (Appearance: theme; Version Control → Git: executable path, update method,
etc.); welcome screen (Open, Clone, recent projects); read-only side-by-side and unified diff viewers with
`ByLine`/`ByWord` ported from `com.intellij.diff.comparison`; basic editable text editor with tree-sitter
highlighting for common languages.

## Error handling

- Every git failure surfaces as a notification that shows the command's stderr and links to the Git
  console entry, matching IntelliJ (`VcsNotifier` semantics, including the "error"/"important"/"silent"
  levels).
- Operations that change repository state (checkout, pull, smart operations) follow upstream's
  rollback and preservation behavior: stashes created by IdeaGit are labelled like IntelliJ's
  (`Uncommitted changes before <op> at <date>`) and are never dropped on failure.
- A missing or too-old `git` is detected at startup and shows a settings prompt.
- Panics in background tasks are caught, logged to `idea-git.log` in the OS log dir, and shown as an
  error notification instead of crashing the app.

## Testing

- **Ported upstream tests**, including their fixtures: `vcs-log/graph` tests, `com.intellij.diff.comparison`
  tests, and `git4idea` parser tests. These give direct evidence of behavioral equivalence.
- **Git integration tests** against real temp repos and local bare remotes: commit, partial commit,
  checkout, smart checkout, fetch/pull/push, branch operations, and console event capture.
- **UI tests** with GPUI's `TestAppContext` for view logic (popup filtering, tree inclusion, keymap
  dispatch), plus the screenshot harness for visual review.
- **CI** (GitHub Actions): macOS arm64/x64, Windows x64, Linux x64; `cargo test`, `cargo clippy -D
  warnings`, `cargo fmt --check`; one job pinned to the minimum supported git version (added in plan 9).

## Distribution

- Slice 1: `cargo xtask dist` produces macOS `.app` (zipped), Windows zip, Linux tar.gz and AppImage.
  Each contains the main binary plus `ig-askpass`. Requires `git` on PATH or a configured path.
- Slice 4: signed and notarized installers (dmg, msi, deb/rpm) and auto-update.

## Upstream-fix workflow

1. `reference/UPSTREAM` pins the intellij-community branch and commit. `upstream-map.toml` records, for
   each Rust module, its mapped upstream files and the commit it last matched.
2. `cargo xtask upstream-watch --clone <path>` lists upstream commits since each module's pin that
   touched its mapped files, grouped by module, with messages and YouTrack IDs.
3. `cargo xtask import-ui` refreshes themes, icons, strings, and keymaps automatically.
4. Porting a fix: read the upstream diff → apply the equivalent Rust change → port any new upstream test →
   bump the module's pin → commit with `upstream: <sha> <YouTrack-ID>`.
