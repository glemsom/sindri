#!/bin/bash
# run_one.sh BIN TASK RUNDIR — one benchmark run in a fresh copy of the fixture
# repo, printed as one JSON line: the task's verdict plus what the run cost.
#
# The provider is named by the caller, because a prompt is measured against a
# model and this script must not guess which one:
#   SINDRI_PROVIDER, SINDRI_API_KEY, SINDRI_BASE_URL, BENCH_MODEL,
#   BENCH_THINKING (optional), BENCH_HEADER (optional, one `Name: value`).
set -u
BIN=$(readlink -f "$1") TASK=$2 RUNDIR=$(readlink -m "$3"); shift 3
BENCH=$(dirname "$(readlink -f "$0")")
: "${BENCH_MODEL:?set BENCH_MODEL to the model this variant is measured on}"
rm -rf "$RUNDIR"; mkdir -p "$RUNDIR/repo"
cp -r "$BENCH/fixture/." "$RUNDIR/repo/"
PROMPT=$(python3 -c "import sys; sys.path.insert(0,'$BENCH'); import tasks; print(tasks.TASKS['$TASK']['prompt'])")
cd "$RUNDIR/repo" || exit 9
export XDG_STATE_HOME="$RUNDIR/state" XDG_CONFIG_HOME="$RUNDIR/config"
export SINDRI_MODEL="$BENCH_MODEL" SINDRI_THINKING="${BENCH_THINKING:-false}"
[ -n "${BENCH_HEADER:-}" ] && export SINDRI_EXTRA_HEADERS="$BENCH_HEADER"
mkdir -p "$XDG_STATE_HOME" "$XDG_CONFIG_HOME"
start=$(date +%s)
"$BIN" -b "$PROMPT" --format json --max-turns 25 --timeout 60 "$@" > "$RUNDIR/out.json" 2> "$RUNDIR/err.txt"
rc=$?
elapsed=$(( $(date +%s) - start ))
python3 - "$TASK" "$RUNDIR" "$rc" "$elapsed" "$BENCH" <<'PY'
import json, os, sys
task, rundir, rc, elapsed, bench = sys.argv[1:6]
sys.path.insert(0, bench); import tasks
try: run = json.load(open(os.path.join(rundir, "out.json")))
except Exception: run = {}
answer = run.get("answer", "")
open(os.path.join(rundir, "answer.md"), "w").write(answer)
check = tasks.check(task, os.path.join(rundir, "repo"), answer)
usage = run.get("usage") or {}
# the bytes of tool output the model read, and the commands it ran: the transcript
# is the only place a run says what it actually did
read_bytes = 0; turns = 0; calls = []
sessions = os.path.join(rundir, "state", "sindri", "sessions")
for d in sorted(os.listdir(sessions)) if os.path.isdir(sessions) else []:
    path = os.path.join(sessions, d, "transcript.jsonl")
    if not os.path.exists(path): continue
    for line in open(path):
        try: entry = json.loads(line)
        except Exception: continue
        kind = entry.get("type")
        if kind == "tool_result": read_bytes += len(entry.get("content", ""))
        elif kind == "assistant": turns += 1
        elif kind == "tool_call": calls.append(entry.get("command") or "")
print(json.dumps({"task": task, "rc": rc, "checks": check,
                  "turns": run.get("turns", turns), "tool_calls": run.get("tool_calls", len(calls)),
                  "in_tokens": usage.get("input_tokens"), "out_tokens": usage.get("output_tokens"),
                  "reason_tokens": usage.get("reasoning_tokens"),
                  "read_bytes": read_bytes, "elapsed_s": int(elapsed),
                  "stopped": run.get("stopped"), "calls": calls}))
PY
