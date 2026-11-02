#!/bin/bash
# self-test.sh — the sweep's loop and the median, proven without a key or a bill.
# The scripts are copied to a temporary directory and `run_one.sh` is replaced by
# a stub that prints one fixed line, so what is asserted is the contract of the
# loop — one line per task per repetition, the tag and the repetition on each
# line, no run directory overwritten, no repetition recorded twice — and the
# median's, never a model's answer.
#
# Each behaviour is its own `test_*` function, as in scripts/tests/, because the
# name is the spec, and each builds its own file so no test can lean on another's
# leftovers. It is not wired into `make check`: the bench stays manual.
set -uo pipefail
BENCH=$(dirname "$(readlink -f "$0")")
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sweep-selftest.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
cp "$BENCH/sweep.sh" "$BENCH/median.jq" "$tmp/"
cat > "$tmp/run_one.sh" <<'STUB'
#!/bin/bash
mkdir -p "$3" && printf '%s' "$3" > "$3/rundir"
printf '{"task":"%s","in_tokens":%s,"turns":1,"checks":{"ok":true}}\n' "$2" "${STUB_TOKENS:-100}"
STUB
chmod +x "$tmp/run_one.sh"
failed=0
note() { printf 'FAIL %s\n' "$*"; failed=1; }
sweep() { "$tmp/sweep.sh" "$@" >/dev/null 2>"$tmp/err.txt"; }
lines() { wc -l < "$1"; }
median() { jq -s -f "$tmp/median.jq" "$1"; }
# Two passes over the four tasks: 1010 and 600, written by hand so the median has
# a right answer that owes nothing to the loop.
pass() { # file
    printf '%s\n' \
      '{"task":"T1","in_tokens":10,"turns":1,"variant":"v02","tag":"a","repetition":1}' \
      '{"task":"T2","in_tokens":1000,"turns":1,"variant":"v02","tag":"a","repetition":1}' \
      '{"task":"T1","in_tokens":300,"turns":1,"variant":"v02","tag":"a","repetition":2}' \
      '{"task":"T2","in_tokens":300,"turns":1,"variant":"v02","tag":"a","repetition":2}' > "$1"
}

test_reps_multiply_the_four_tasks_into_one_file() {
    REPS=3 sweep "$tmp/three.jsonl" v02 || { note "REPS=3 refused to run: $(cat "$tmp/err.txt")"; return; }
    [ "$(lines "$tmp/three.jsonl")" -eq 12 ] || note "REPS=3 over 4 tasks wrote $(lines "$tmp/three.jsonl") lines, not 12"
    [ "$(jq -s -c '[map(.repetition) | group_by(.) | map(length)]' "$tmp/three.jsonl")" = "[[4,4,4]]" ] \
        || note "the 12 lines are not 4 tasks x 3 repetitions: $(jq -s -c 'map(.repetition)' "$tmp/three.jsonl")"
    [ "$(jq -s -c 'map(.attempt) | unique' "$tmp/three.jsonl")" = '[1]' ] \
        || note "a run that answered was retried: attempts $(jq -s -c 'map(.attempt) | unique' "$tmp/three.jsonl")"
    made=$(ls "$tmp/runs" | grep -c '^run-v02-')
    [ "$made" -eq 12 ] || note "12 repetitions wrote $made run directories of their own"
}

test_a_second_tag_appends_alongside_the_first() {
    STUB_TOKENS=200 TAG=b REPS=1 sweep "$tmp/appended.jsonl" v02 || { note "the appended sweep failed: $(cat "$tmp/err.txt")"; return; }
    STUB_TOKENS=300 TAG=c REPS=1 sweep "$tmp/appended.jsonl" v02 || { note "the third sweep failed: $(cat "$tmp/err.txt")"; return; }
    [ "$(lines "$tmp/appended.jsonl")" -eq 8 ] || note "two tagged sweeps left $(lines "$tmp/appended.jsonl") lines, not 8"
    [ "$(jq -s -c 'map(.tag) | unique' "$tmp/appended.jsonl")" = '["b","c"]' ] \
        || note "the two sweeps are not told apart by tag: $(jq -s -c 'map(.tag)' "$tmp/appended.jsonl")"
    # 800 for b and 1200 for c: the median of two passes, not of the eight lines.
    [ "$(median "$tmp/appended.jsonl")" = 1000 ] || note "two passes of 800 and 1200 have the median 1000, jq said $(median "$tmp/appended.jsonl")"
}

test_a_repetition_already_recorded_is_refused() {
    STUB_TOKENS=200 TAG=b REPS=1 sweep "$tmp/refused.jsonl" v02 || { note "the first sweep failed: $(cat "$tmp/err.txt")"; return; }
    before=$(lines "$tmp/refused.jsonl")
    if STUB_TOKENS=200 TAG=b REPS=1 sweep "$tmp/refused.jsonl" v02; then note "a second b/rep-1 sweep was recorded instead of refused"; fi
    [ "$(lines "$tmp/refused.jsonl")" -eq "$before" ] || note "the refused sweep still wrote into the file"
    grep -q 'already holds' "$tmp/err.txt" || note "the refusal did not say why: $(cat "$tmp/err.txt")"
}

test_reps_must_be_a_positive_number() {
    REPS=abc sweep "$tmp/none.jsonl" v02 && note "REPS=abc was accepted"
    [ -f "$tmp/none.jsonl" ] && note "REPS=abc created a file"
    REPS=0 sweep "$tmp/zero.jsonl" v02 && note "REPS=0 was accepted"
}

test_the_median_is_the_middle_pass_or_the_mean_of_the_two_middles() {
    pass "$tmp/even.jsonl"
    [ "$(median "$tmp/even.jsonl")" = 805 ] || note "the median of [600, 1010] is 805, jq said $(median "$tmp/even.jsonl")"
    jq -c 'select(.repetition == 2) | . + {repetition: 3}' "$tmp/even.jsonl" >> "$tmp/even.jsonl"
    [ "$(median "$tmp/even.jsonl")" = 600 ] || note "the median of [600, 600, 1010] is 600, jq said $(median "$tmp/even.jsonl")"
}

test_a_run_that_never_reached_the_model_is_not_a_measurement() {
    pass "$tmp/null.jsonl"
    jq -c 'if .repetition == 1 then .in_tokens = null else . end' "$tmp/null.jsonl" > "$tmp/null2.jsonl"
    [ "$(median "$tmp/null2.jsonl")" = 600 ] || note "a null in_tokens changed the median to $(median "$tmp/null2.jsonl")"
}

test_a_file_holding_two_variants_is_refused() {
    pass "$tmp/mixed.jsonl"
    jq -c 'select(.repetition == 1) | . + {variant: "v11"}' "$tmp/mixed.jsonl" >> "$tmp/mixed.jsonl"
    median "$tmp/mixed.jsonl" >/dev/null 2>&1 && note "median.jq averaged two variants instead of refusing"
}

for name in $(declare -F | awk '{print $3}' | grep '^test_' | sort); do
    "$name"
done
[ "$failed" -eq 0 ] && printf 'self-test: sweep.sh and median.jq behave\n'
exit "$failed"
