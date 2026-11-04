#!/bin/bash
# run_one.sh ARM RUNDIR — one ablation run, in a fresh copy of the task's fixture
# repo, printed as one JSON line: the arm, the run's summed input tokens, whether
# the task's built-in was activated, and the score of what the run left behind.
#
# The arm is a binary, not a flag: `bin-with` was built with the built-in table as
# it ships and the shipped doctrine, `bin-without` with the table emptied and
# `bin-no-trigger` with the doctrine one sentence shorter (see build_arm.sh), so
# what changes between two runs is the text the binary was built from and nothing
# else.
#
# The task is `BENCH_TASK`'s, and it is the question, the fixture and the skill
# whose load counts: `inspect` is a read, scored on the tokens it took to reach the
# same answer, and it stands in a repository of its own under `fixture/`.
#
# A run never sees the developer's own skills: `HOME` points into the run
# directory and holds no `.agents/skills`, so the only skills in the catalog are the
# built-ins. Offline (`BENCH_OFFLINE=1`) the provider is the recorded replies in
# `recorded/<task>/<arm>/`; live it is the provider named by SINDRI_PROVIDER,
# SINDRI_API_KEY and SINDRI_BASE_URL, exactly as the prompt bench names one.
set -u
BENCH=$(dirname "$(readlink -f "$0")")
arm=${1:?usage: run_one.sh with|without|no-trigger RUNDIR}
RUNDIR=$(readlink -m "$2")
BIN=${BENCH_BIN:-$BENCH/bin-$arm}
TAG=${TAG:-run}
REPETITION=${REPETITION:-1}
ATTEMPT=${ATTEMPT:-1}
TASK=${BENCH_TASK:-inspect}
: "${BENCH_MODEL:?set BENCH_MODEL to the model this arm is measured on}"

# shellcheck source=arms.sh
. "$BENCH/arms.sh"
if ! arm_is_a_name "$arm"; then
  echo "run_one: arm must be $(arms_words), not '$arm'" >&2; exit 2
fi
# The task is a name the bench holds, and asking `tasks.py` is what keeps a typo
# from becoming a fixture path a run half-reads: the question, the repository and
# the skill are all that task's, and a name nothing answers to is refused here.
ask_task() { python3 "$BENCH/tasks.py" "$1" "$TASK"; }
if ! fixture=$(ask_task fixture) 2>/dev/null; then
  echo "run_one: the bench holds no task '$TASK' — it holds $(python3 "$BENCH/bench.py" tasks)" >&2
  exit 2
fi
PROMPT=$(ask_task prompt)

[ -x "$BIN" ] || { echo "run_one: $BIN is not built — run ./build_arm.sh $arm" >&2; exit 2; }

# The arm is what the binary carries, not the word on the command line: a run
# labelled `with` on another arm's binary would land in the with row with the
# wrong words in front of the model, which is the one number this bench exists to
# be honest about. So the binary is asked (see arms.sh), and a mismatch is refused
# before it runs.
if ! reason=$(arm_mismatch "$arm" "$BIN"); then
  echo "run_one: $BIN is not the $arm arm: $reason — build it with ./build_arm.sh $arm" >&2
  exit 2
fi

rm -rf "$RUNDIR"; mkdir -p "$RUNDIR/repo" "$RUNDIR/state" "$RUNDIR/config" "$RUNDIR/home"
cp -r "$BENCH/$fixture/." "$RUNDIR/repo/"
git -C "$RUNDIR/repo" init --quiet
export XDG_STATE_HOME="$RUNDIR/state" HOME="$RUNDIR/home"
export SINDRI_MODEL="$BENCH_MODEL" SINDRI_THINKING=false
mock=
if [ -n "${BENCH_OFFLINE:-}" ]; then
  # the recorded conversation for this task and arm, served on loopback: no key,
  # no bill
  export XDG_CONFIG_HOME="$RUNDIR/config"
  python3 "$BENCH/mock_provider.py" "$BENCH/recorded/$TASK/$arm" > "$RUNDIR/mock.url" 2> "$RUNDIR/mock.err" &
  mock=$!
  for _ in $(seq 1 100); do [ -s "$RUNDIR/mock.url" ] && break; sleep 0.1; done
  url=$(cat "$RUNDIR/mock.url")
  [ -n "$url" ] || { echo "run_one: the mock provider did not start: $(cat "$RUNDIR/mock.err")" >&2; kill "$mock" 2>/dev/null; exit 9; }
  export SINDRI_PROVIDER=anthropic SINDRI_API_KEY=sk-ablation-not-a-real-key SINDRI_BASE_URL="$url"
fi

cd "$RUNDIR/repo" || exit 9
start=$(date +%s)
"$BIN" -b "$PROMPT" --format json --max-turns 20 --timeout 60 > "$RUNDIR/out.json" 2> "$RUNDIR/err.txt"
rc=$?
elapsed=$(( $(date +%s) - start ))
[ -n "$mock" ] && { kill "$mock" 2>/dev/null; wait "$mock" 2>/dev/null; }
printf '%s\n' "$rc" > "$RUNDIR/rc.txt"

python3 "$BENCH/bench.py" record "$RUNDIR" --task "$TASK" \
  --arm "$arm" --tag "$TAG" --repetition "$REPETITION" --attempt "$ATTEMPT" \
  --model "$BENCH_MODEL" --elapsed "$elapsed"
