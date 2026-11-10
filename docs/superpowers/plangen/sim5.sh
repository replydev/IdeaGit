#!/usr/bin/env bash
# Replays Plan 2b's tasks on a fresh clone, exactly as gen5.py renders them, and writes sim.json
# (red-step output and test counts per task) for the plan. Stops at the first step that isn't as expected.
#   sim5.sh <prototype checkout> <scratch dir> <sim.json>
set -euo pipefail
PROTO=$(cd "$1" && pwd); SIM=$2; OUT=$3
GEN=$(cd "$(dirname "$0")" && pwd)/gen5.py
export CARGO_TARGET_DIR=${SIM}-target CARGO_NET_GIT_FETCH_WITH_CLI=true
rm -rf "$SIM"; git clone -q "$PROTO" "$SIM"; cd "$SIM"; git checkout -q -b sim 75fe6f9
n=$(python3 "$GEN" count)
echo "{" > "$OUT"
for i in $(seq 1 "$n"); do
  commit=$(python3 - "$GEN" "$i" <<'EOF'
import importlib.util, sys
s = importlib.util.spec_from_file_location("g", sys.argv[1]); g = importlib.util.module_from_spec(s); s.loader.exec_module(g)
print(g.TASKS[int(sys.argv[2]) - 1].commit)
EOF
)
  red=$(python3 "$GEN" red "$PROTO" "$i")
  redout=""
  if [ -n "$red" ]; then
    python3 "$GEN" apply "$PROTO" "$SIM" "$i" tests
    if bash -c "$red" > /tmp/sim5-red.log 2>&1; then echo "task $i: red step PASSED unexpectedly"; exit 1; fi
    redout=$(grep -m1 -E "^error(\[E[0-9]+\])?:" /tmp/sim5-red.log | sed 's/"/\\"/g' || true)
    echo "task $i red: $redout"
    python3 "$GEN" apply "$PROTO" "$SIM" "$i" code
    bash -c "$red" > /tmp/sim5-green.log 2>&1 || { echo "task $i: new tests fail after the code"; tail -30 /tmp/sim5-green.log; exit 1; }
  else
    python3 "$GEN" apply "$PROTO" "$SIM" "$i" tests
    python3 "$GEN" apply "$PROTO" "$SIM" "$i" code
  fi
  git add -A
  git diff --cached --exit-code "$commit" -- $(git -C "$PROTO" show --name-only --format= "$commit") > /dev/null || { echo "task $i: copy not exact"; exit 1; }
  # the whole tree must equal the prototype at this commit
  git diff --cached --quiet "$commit" || { echo "task $i: tree differs from $commit"; git diff --cached --stat "$commit"; exit 1; }
  cargo fmt --all --check
  cargo clippy -q --locked --all-targets -- -D warnings
  cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
  RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
  cargo test -q --locked --all > /tmp/sim5-test.log 2>&1 || { echo "task $i: tests fail"; grep -E "FAILED|panicked" /tmp/sim5-test.log | head; exit 1; }
  counts=$(grep -E "^test result" /tmp/sim5-test.log | awk '{p+=$4; f+=$6; ig+=$8} END {print p, ig}')
  set -- $counts
  echo "task $i: passed $1 ignored $2"
  [ "$i" -gt 1 ] && echo "," >> "$OUT"
  printf '"%s": {"passed": %s, "ignored": %s, "red": "%s"}' "$i" "$1" "$2" "$redout" >> "$OUT"
  git commit -q --no-gpg-sign -m "sim task $i"
done
echo "}" >> "$OUT"
echo "simulation green"
