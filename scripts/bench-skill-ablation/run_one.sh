#!/bin/bash
# run_one.sh ARM RUNDIR — one ablation run, in a fresh copy of the fixture repo,
# printed as one JSON line: the arm, the run's summed input tokens, whether the
# built-in was activated, and the rule's verdict on what the run left behind.
#
# The arm is a binary, not a flag: `bin-with` was built with the built-in in the
# catalog and `bin-without` with the table emptied (see build_arm.sh), so what
# changes between the two runs is the catalog and nothing else.
#
# A run never sees the developer's own skills: `HOME` points into the run
# directory and holds no `.agents/skills`, so the only skill in the catalog is
# the built-in under test. Offline (`BENCH_OFFLINE=1`) the provider is the
# recorded replies in `recorded/<arm>/`; live it is the provider named by
# SINDRI_PROVIDER, SINDRI_API_KEY and SINDRI_BASE_URL, exactly as the prompt
# bench names one.
set -u
BENCH=$(dirname "$(readlink -f "$0")")
arm=${1:?usage: run_one.sh with|without RUNDIR}
RUNDIR=$(readlink -m "$2")
BIN=${BENCH_BIN:-$BENCH/bin-$arm}
TAG=${TAG:-run}
REPETITION=${REPETITION:-1}
ATTEMPT=${ATTEMPT:-1}
PROMPT=$(python3 -c "import sys; sys.path.insert(0,'$BENCH'); import tasks; print(tasks.PROMPT)")
: "${BENCH_MODEL:?set BENCH_MODEL to the model this arm is measured on}"

case $arm in with|without) : ;; *) echo "run_one: arm must be with or without, not '$arm'" >&2; exit 2;; esac
[ -x "$BIN" ] || { echo "run_one: $BIN is not built — run ./build_arm.sh $arm" >&2; exit 2; }

# The arm is the binary's catalog, not the word on the command line: a run
# labelled `with` on the `without` binary would land in the with row with the
# skill never activated, which is the one number this bench exists to be honest
# about. So the binary is asked, and a mismatch is refused before it runs.
if "$BIN" doctor 2>/dev/null | grep -q '^  builtin '; then
  [ "$arm" = with ] || {
    echo "run_one: $BIN ships a built-in, so it is not the without arm" >&2; exit 2; }
else
  [ "$arm" = without ] || {
    echo "run_one: $BIN ships no built-in, so it is not the with arm — ./build_arm.sh $arm" >&2; exit 2; }
fi

rm -rf "$RUNDIR"; mkdir -p "$RUNDIR/repo" "$RUNDIR/state" "$RUNDIR/config" "$RUNDIR/home"
cp -r "$BENCH/fixture/." "$RUNDIR/repo/"
git -C "$RUNDIR/repo" init --quiet
export XDG_STATE_HOME="$RUNDIR/state" HOME="$RUNDIR/home"
export SINDRI_MODEL="$BENCH_MODEL" SINDRI_THINKING=false
mock=
if [ -n "${BENCH_OFFLINE:-}" ]; then
  # the recorded conversation for this arm, served on loopback: no key, no bill
  export XDG_CONFIG_HOME="$RUNDIR/config"
  python3 "$BENCH/mock_provider.py" "$BENCH/recorded/$arm" > "$RUNDIR/mock.url" 2> "$RUNDIR/mock.err" &
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

python3 "$BENCH/bench.py" record "$RUNDIR" --task "$(python3 -c "import sys; sys.path.insert(0,'$BENCH'); import tasks; print(tasks.TASK)")" \
  --arm "$arm" --tag "$TAG" --repetition "$REPETITION" --attempt "$ATTEMPT" \
  --model "$BENCH_MODEL" --elapsed "$elapsed"
