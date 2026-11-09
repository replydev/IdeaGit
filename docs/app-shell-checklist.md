# App shell: manual checklist

Run these by hand on each platform before a release, and after changes to the app shell or the Log. They cover what
the headless tests can't: real windows, GPU rendering, OS integration and large histories.

Start the app with `cargo run --locked --release -p ig-app -- [path]`.

## Open a real repository

- [ ] `ideagit <repo>` opens a project window with the **Log** filling the main area (focused) and the Console hidden.
- [ ] The graph, ref labels, author, date and hash columns match IntelliJ IDEA's Log for the same repository
      (compare with `docs/ui-reference/`).
- [ ] Click, Shift+click, Cmd/Ctrl+click and the arrow keys select rows as in IntelliJ.
- [ ] Hovering a long-edge arrow highlights it; clicking it jumps to and selects the target row.
- [ ] The **Console** stripe button (below Git) shows the Console tool window at the bottom with the commands IdeaGit
      ran; **View → Show read commands in Console** also shows reads.
- [ ] Shift+Esc in the Console, its header's Hide button, or the stripe button hides it completely (no strip left) and
      focus returns to the Log; showing it again keeps the height it was resized to.
- [ ] Cmd+9 (macOS) / Alt+9 focuses the Log; the Git stripe button is always selected, the Console button while the
      Console is open. **View → Tool Windows** lists Git and Console.

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
