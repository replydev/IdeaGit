#!/usr/bin/env python3
"""Plan 2b (graph extras) generator.

Each task is one commit of the verified prototype branch `wip/graph-extras-prototype`. A task first
copies that commit's test files (upstream test classes and test support) and runs them red, then
copies the code they cover. Files are taken at the task's own commit, so a file that several tasks
touch is copied in its intermediate form until its last task. The same data renders the plan and
applies a task (or half of one) to a scratch clone, so the simulation runs exactly what the plan says.

    gen5.py plan  <prototype checkout> <out.md> [sim.json]
    gen5.py apply <prototype checkout> <target clone> <task number> tests|code
    gen5.py red   <prototype checkout> <task number>      # prints the red-step command (empty if none)
    gen5.py count
"""
import json, os, re, subprocess, sys

PROTO = "wip/graph-extras-prototype"
BASE = "75fe6f9"  # master with the Plan 2b spec
R = None  # prototype checkout


class Task:
    def __init__(self, commit, title, review, intro, red_note):
        self.commit, self.title, self.review, self.intro, self.red_note = commit, title, review, intro, red_note


TASKS = [
    Task(
        "8da092b",
        "Graph utilities (BFS, GraphUtil, UnsignedBitSet, IntToIntMap family)",
        "`GraphUtil` / `BfsUtil` against `utils/GraphUtil.kt` and `utils/BfsUtil.kt` (walk order, the `BfsSearch` limit quirk), "
        "`UnsignedBitSet` negative indexes, and the `IntToIntMap` family's update semantics (`UpdatableIntToIntMap.update`)",
        "The leaf utilities the rest of the plan builds on. The `test_graph!` macro in the shared test builder mirrors "
        "upstream's Kotlin `graph { 1(2, 3) ... }` DSL, so ported tests read like upstream.",
        "the new modules don't exist yet, so the test crates fail to compile (`unresolved import ig_log::utils::...`)",
    ),
    Task(
        "8f9df63",
        "BEK sort (SortIndexMap, BekSorter, SortedBaseController)",
        "`BekSorter`, `BekBranchCreator`, `BekBranchMerger` and `BekEdgeRestrictions` against `impl/facade/sort/bek/*`; "
        "comparator sorts go through `utils::java_sort`; `SortChecker`'s `LOG.error` became `debug_assert!`",
        "The BEK (\"Standard\") ordering: a permutation of the permanent graph's rows, applied by `SortedBaseController`.",
        "`ig_log::facade::sort` doesn't exist yet, so `tests/bek.rs` fails to compile",
    ),
    Task(
        "741cdf8",
        "Collapsed graph, dotted filter edges and fragment generators",
        "`CollapsedGraph`'s `Arc<RwLock<_>>` state and lock re-entrancy (the compiled graph's edge wrapper takes the read "
        "lock per call; no lock may be held while the same lock is taken again), and the ascending iteration that "
        "replaces upstream's hash-set order in `FragmentGenerator`",
        "The collapsing machinery the filtering and collapsing controllers sit on. It comes before the controllers because "
        "`FirstParentTest` needs `CollapsedGraph` and the dotted-edge generator.",
        "the collapsing modules don't exist yet, so both test crates fail to compile",
    ),
    Task(
        "005e40a",
        "Cascade, filtered, first-parent, collapsed and branch filter controllers",
        "`CascadeController` as a trait plus template function, the new `LinearGraphController` default methods "
        "(`delegate`, `delegate_mut`, `delegate_graph_changed`, `as_any_mut`, `supports_collapse`) that replace upstream's "
        "downcasts, `GraphModificationUtil`, and the stale-element guard (out-of-range node indexes act as \"no element\")",
        "The controller chain pieces. `TestPermanentGraphInfo` is ported into the test builder here.",
        "the controllers don't exist yet, so `tests/first_parent.rs` and `tests/collapsing_controllers.rs` fail to compile",
    ),
    Task(
        "6ac6c8a",
        "Linear BEK (LinearBekGraph, LinearBekGraphBuilder, LinearBekController)",
        "`LinearBekGraphBuilder` against `linearBek/LinearBekGraphBuilder.java`, and `utils::java_priority_queue`: "
        "the order in which tied edges leave Java's `PriorityQueue` changes the result, so it must match `java.util.PriorityQueue` exactly",
        "The \"IntelliJ style\" sort. `LinearBekTest` was converted from Kotlin by script; its graph data was checked "
        "digit for digit against upstream.",
        "`ig_log::linear_bek` doesn't exist yet, so the test crates fail to compile",
    ),
    Task(
        "1ec0a3d",
        "Every visible graph option: PermanentGraphImpl's controller chain; create_visible_graph is infallible",
        "`create_visible_graph` against `PermanentGraphImpl.createFilteredController` row by row, the `OnceLock` BEK map, "
        "the empty-repository fix (`getReachableNodes(null)`), and the removed `UnsupportedGraphOptions` in every caller",
        "Wires everything into `PermanentGraphImpl`. Unfiltered `Normal` goes back to upstream's `CollapsedController`, "
        "which brings fragment hover and collapse/expand. The app and `ig-git` callers lose their `Result` handling.",
        "the tests call the infallible `create_visible_graph`, which still returns `Result`, so they fail to compile",
    ),
    Task(
        "61fd29a",
        "App: fragment hover; a click on a node collapses its fragment",
        "the after-click refresh (new generation, hover cleared, selection kept by commit, as upstream's `handleGraphAnswer`) "
        "and the new `GraphGeometry::node_center` helper",
        "Hovering a node highlights its linear fragment, and a click collapses or expands it, as in IntelliJ.",
        "`GraphGeometry::node_center` doesn't exist yet, so `tests/ui.rs` fails to compile",
    ),
    Task(
        "9b5fce5",
        "Upstream map and docs",
        "every new module's upstream paths, tests and deviations in `reference/upstream-map.toml`, and the removed Plan 2 "
        "deviations (unsupported options, collapse buttons, no `SimpleGraphInfo`)",
        "",
        None,
    ),
]

CHECK = """cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all"""


def git(*args):
    return subprocess.run(["git", *args], cwd=R, capture_output=True, check=True).stdout


def files(task):
    out = git("show", "--name-only", "--format=", task.commit).decode().split()
    return sorted(out)


def is_test(p):
    return "/tests/" in p


def split(task):
    fs = files(task)
    return [p for p in fs if is_test(p)], [p for p in fs if not is_test(p)]


def show(commit, p):
    return git("show", f"{commit}:{p}").decode()


def message(task):
    return git("log", "-1", "--format=%B", task.commit).decode().strip()


def red_cmd(task):
    tests, _ = split(task)
    targets = []
    for p in tests:
        m = re.match(r"crates/([^/]+)/tests/([^/]+)\.rs$", p)
        if m:
            targets.append((m.group(1), m.group(2)))
    if not targets or task.red_note is None:
        return ""
    by_crate = {}
    for c, t in targets:
        by_crate.setdefault(c, []).append(t)
    return "\n".join(
        f"cargo test -q --locked -p {c} " + " ".join(f"--test {t}" for t in ts) for c, ts in by_crate.items()
    )


def apply(target, n, half):
    task = TASKS[n - 1]
    tests, code = split(task)
    paths = tests if half == "tests" else code
    for p in paths:
        dst = os.path.join(target, p)
        os.makedirs(os.path.dirname(dst) or ".", exist_ok=True)
        open(dst, "wb").write(git("show", f"{task.commit}:{p}"))


# ---------------------------------------------------------------------------------------------
# plan
# ---------------------------------------------------------------------------------------------

def fence(p):
    return "rust" if p.endswith(".rs") else "toml" if p.endswith(".toml") else "text"


def block(lang, body):
    ticks = "````" if "```" in body else "```"
    return f"{ticks}{lang}\n{body if body.endswith(chr(10)) else body + chr(10)}{ticks}\n"


def parent_has(task, p):
    return subprocess.run(["git", "cat-file", "-e", f"{task.commit}^:{p}"], cwd=R, capture_output=True).returncode == 0


def embed(task, p):
    """New files in full; changed files as a diff against the previous commit."""
    if parent_has(task, p):
        d = git("diff", f"{task.commit}^", task.commit, "--", p).decode()
        return f"`{p}` (changed; diff against the previous task):\n\n" + block("diff", d) + "\n"
    return f"`{p}`:\n\n" + block(fence(p), show(task.commit, p)) + "\n"


def render_task(i, task, sim):
    tests, code = split(task)
    new = [p for p in tests + code if not parent_has(task, p)]
    changed = [p for p in tests + code if parent_has(task, p)]
    s = sim.get(str(i), {}) if sim else {}
    o = [f"### Task {i}: {task.title}\n\n**Files:**\n"]
    if new:
        o.append("- Create: " + ", ".join(f"`{p}`" for p in new) + "\n")
    if changed:
        o.append("- Modify: " + ", ".join(f"`{p}`" for p in changed) + "\n")
    o.append(f"\n**Source commit:** `{task.commit}` on `{PROTO}`.\n\n**Review focus:** {task.review}.\n\n")
    if task.intro:
        o.append(task.intro + "\n\n")
    step = 1
    red = red_cmd(task)
    if tests and red:
        o.append(f"- [ ] **Step {step}: Copy the tests first**\n\n" + block("bash", f"git checkout {task.commit} -- " + " ".join(tests)) + "\n")
        o.append("".join(embed(task, p) for p in tests))
        step += 1
        out = s.get("red", "a compile error")
        o.append(f"- [ ] **Step {step}: Run them and watch them fail**\n\n" + block("bash", red) +
                 f"\nExpected: FAIL — {task.red_note}. The simulation's first error:\n\n" + block("text", out) + "\n")
        step += 1
        o.append(f"- [ ] **Step {step}: Copy the code**\n\n" + block("bash", f"git checkout {task.commit} -- " + " ".join(code)) + "\n")
        o.append("".join(embed(task, p) for p in code))
        step += 1
        o.append(f"- [ ] **Step {step}: Run the new tests and watch them pass**\n\n" + block("bash", red) + "\nExpected: PASS.\n\n")
        step += 1
    else:
        o.append(f"- [ ] **Step {step}: Copy the files**\n\n" + block("bash", f"git checkout {task.commit} -- " + " ".join(tests + code)) + "\n")
        o.append("".join(embed(task, p) for p in tests + code))
        step += 1
    o.append(f"- [ ] **Step {step}: Check that the copies are exact**\n\n" +
             block("bash", f"git add -A\ngit diff --cached --exit-code {task.commit} -- " + " ".join(tests + code)) +
             "\nExpected: no output.\n\n")
    step += 1
    exp = f"all clean; `cargo test` reports {s['passed']} passed, {s['ignored']} ignored" if "passed" in s else "all clean"
    o.append(f"- [ ] **Step {step}: Verify**\n\n" + block("bash", CHECK) + f"\nExpected: {exp}. Fix the source, never the expected values of ported tests.\n\n")
    step += 1
    o.append(f"- [ ] **Step {step}: Commit**\n\n" + block("bash", f"git add -A\ngit commit -m {sh(message(task))}") + "\n---\n\n")
    return "".join(o)


def sh(s):
    return "'" + s.replace("'", "'\\''") + "'"


def plan(out, sim):
    head = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "gen5_head.md"), encoding="utf-8").read()
    body = "".join(render_task(i, t, sim) for i, t in enumerate(TASKS, start=1))
    open(out, "w", encoding="utf-8").write(head + "\n---\n\n" + body)
    print("tasks", len(TASKS), "lines", sum(1 for _ in open(out, encoding="utf-8")))


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "count":
        print(len(TASKS))
    elif cmd == "apply":
        R = os.path.abspath(sys.argv[2])
        apply(os.path.abspath(sys.argv[3]), int(sys.argv[4]), sys.argv[5])
    elif cmd == "red":
        R = os.path.abspath(sys.argv[2])
        print(red_cmd(TASKS[int(sys.argv[3]) - 1]))
    elif cmd == "plan":
        R = os.path.abspath(sys.argv[2])
        sim = json.load(open(sys.argv[4])) if len(sys.argv) > 4 else None
        plan(sys.argv[3], sim)
