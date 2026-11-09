# Technical debt

Known compromises, why we accepted them, and how to pay them down. Keep each entry short, and remove it once it's resolved.

## gpui-component pinned to an old commit (2026-11-09)

- **What:** `gpui-component` is pinned to gpui-kit commit `c0946e6acdc9e2f984f317ef7f998ee2c79f1a87`. That is version 0.5.2, from 2026-09-03.
- **Why:**
  - IdeaGit builds on Zed's official `gpui` from git. It doesn't use the third-party `gpui-pre` snapshots. That choice keeps the GPUI source official and reproducible.
  - Upstream gpui-component switched to `gpui-pre` right after that commit. Newer releases, 0.9 and later, can't build on official gpui without a fork.
- **Impact:**
  - We get no gpui-component fixes or features newer than 2026-09-03.
  - We can only bump Zed's gpui as far as that gpui-component commit still builds against.
- **Exposure:** small.
  - IdeaGit uses gpui-component only for the dock, which is rendered through our own `IjDockRenderer`, and a few small widgets.
  - The Log table, tool-window stripes and graph painter are our own code.
- **Pay down:** vendor the dock module into `ig-ui-kit` and remove the `gpui-component` dependency. Do it when the pin blocks a gpui bump or a fix we need.

## Zed gpui commit pinned through `Cargo.lock` (2026-11-09)

- **What:** Zed `f66ed399cdde86092af8af3dc7b418abf45f37f8` is pinned in `Cargo.lock`, not with `rev =` in `Cargo.toml`.
- **Why:** gpui-kit depends on Zed's default branch without a `rev`. A `rev` pin on our side creates a second gpui source.
- **Rules:**
  - Every build and CI command uses `--locked`.
  - Bump with `cargo update -p gpui@0.2.2 --precise <zed sha>`, using the sha that a gpui-kit commit's own `Cargo.lock` records.
  - Never run a plain `cargo update`. It would move Zed to the tip of its default branch.

## App shell gaps left by Plan 3 (2026-11-09)

- **Dock resize:** our `IjDockRenderer` edge doesn't emit `DockEvent::LayoutChanged` after a resize and doesn't reopen a closed bottom dock when dragged, as gpui-component's own skin does. Fix this with layout persistence.
- **Log cancellation:** closing a window cancels the log worker between steps, but doesn't reach inside `VcsLogRefresher` or kill a running `git` process. A long first load of a huge repository keeps running in the background until it finishes. Pay down with runner cancellation (already open from Plan 2c).
- **Status-bar branch widget:** shows one repository's branch or a short hash. It doesn't show the tag name for a detached HEAD on a tag, or the merge/rebase/cherry-pick/revert prefixes of `GitBranchWidget`.
- **Synchronous work on the UI thread:** `detect_roots` and `Git::detect` (after saving a git path) run on the UI thread. Moving them off needs a way to keep "one window per folder" while opening is asynchronous.
- **Windows and Linux are unverified** until CI runs the GPUI build there (see the CI notes in the app-shell plan).
