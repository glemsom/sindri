#!/bin/bash
# self-test.sh — the bench proven end to end without a key or a bill.
#
# Two halves. The unit tests prove the arithmetic: how a transcript is read, what
# the rule scores, what a record line carries, whether a load came before the
# first command, what the table averages, and what the second task's answer has to
# name. The offline pass proves the pipeline: the three arm binaries, a recorded
# conversation served on loopback, one sweep of each task through all three arms,
# and the report the numbers come out of. What it does NOT prove is any model's
# behaviour — the replies are scripts, not captures, and the token counts on those
# lines are the mock's.
#
# `BENCH_SKIP_BUILD=1` reuses `bin-with`, `bin-without` and `bin-no-trigger`
# instead of building them; without it the three arm builds run, which is a cargo
# build each.
set -uo pipefail
BENCH=$(dirname "$(readlink -f "$0")")
tmp=$(mktemp -d "${TMPDIR:-/tmp}/skill-ablation-selftest.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
export BENCH_OFFLINE=1 BENCH_MODEL=mock TAG=selftest
failed=0
note() { printf 'FAIL %s\n' "$*"; failed=1; }

# The unit half: the record line, the rule, the activation field, the table.
test_the_units_of_the_bench_pass() {
    ( cd "$BENCH" && python3 -m unittest discover -s tests ) > "$tmp/units.txt" 2>&1 \
        || { note "the unit tests failed: $(tail -5 "$tmp/units.txt")"; }
}

# The arms are builds: the catalog line is really gone from `without`, and the
# trigger sentence is really gone from `no-trigger` while the catalog stays.
test_the_three_arms_differ_only_in_the_text_they_were_built_from() {
    for arm in with without no-trigger; do
        if [ -z "${BENCH_SKIP_BUILD:-}" ]; then
            "$BENCH/build_arm.sh" "$arm" >> "$tmp/build.txt" 2>&1 \
                || { note "build_arm.sh $arm failed: $(tail -3 "$tmp/build.txt")"; return; }
        fi
        [ -x "$BENCH/bin-$arm" ] || { note "bin-$arm is not built"; return; }
    done
    "$BENCH/bin-with" doctor | grep -q '^  builtin  sandbox-refusal ' \
        || note "the with arm does not ship the built-in"
    ! "$BENCH/bin-without" doctor | grep -q '^  builtin ' \
        || note "the without arm still ships a built-in, so it is not an ablation"
    "$BENCH/bin-no-trigger" doctor | grep -q '^  builtin  sandbox-refusal ' \
        || note "the no-trigger arm does not ship the built-in it ablates the sentence of"
    python3 "$BENCH/doctrines.py" carries "$BENCH/bin-with" \
        || note "the with arm does not carry the trigger sentence"
    ! python3 "$BENCH/doctrines.py" carries "$BENCH/bin-no-trigger" \
        || note "the no-trigger arm carries the sentence it is named for removing"
}

# The arm is what the binary carries, never the word on the command line: the
# binaries above are swapped and the run must refuse rather than record a `with`
# line whose skill was never activated, or a `with` line built from the words the
# third arm is the absence of.
test_a_run_label_its_binary_contradicts_is_refused() {
    for pair in "with bin-without" "without bin-with" \
                "with bin-no-trigger" "no-trigger bin-with"; do
        set -- $pair
        if BENCH_BIN="$BENCH/$2" BENCH_OFFLINE=1 BENCH_MODEL=mock \
            "$BENCH/run_one.sh" "$1" "$tmp/lie-$1" > "$tmp/lie-$1.out" 2>&1; then
            note "a run labelled $1 on bin-$2 was recorded: $(cat "$tmp/lie-$1.out")"
        fi
        grep -q "is not the $1 arm" "$tmp/lie-$1.out" \
            || note "the refusal of $1 on bin-$2 did not say why: $(cat "$tmp/lie-$1.out")"
    done
}

# One sweep, all three arms, through the shell loop that produces the real numbers.
test_the_offline_sweep_writes_one_line_per_arm_per_repetition() {
    REPS=2 BENCH_OFFLINE=1 BENCH_MODEL=mock TAG=selftest \
        "$BENCH/sweep.sh" "$tmp/offline.jsonl" with without no-trigger \
        > /dev/null 2> "$tmp/sweep.txt" \
        || { note "the offline sweep failed: $(tail -3 "$tmp/sweep.txt")"; return; }
    [ "$(wc -l < "$tmp/offline.jsonl")" -eq 6 ] \
        || note "three arms x two repetitions wrote $(wc -l < "$tmp/offline.jsonl") lines, not 6"
    [ "$(jq -s -c '[group_by(.arm)[] | map(.repetition) | sort]' "$tmp/offline.jsonl")" = '[[1,2],[1,2],[1,2]]' ] \
        || note "the lines are not three arms of two repetitions: $(jq -s -c 'map([.arm,.repetition])' "$tmp/offline.jsonl")"
}

test_the_reading_task_is_a_sweep_of_its_own_file() {
    REPS=2 BENCH_TASK=inspect BENCH_OFFLINE=1 BENCH_MODEL=mock TAG=selftest \
        "$BENCH/sweep.sh" "$tmp/inspect.jsonl" with without no-trigger \
        > /dev/null 2> "$tmp/inspect-sweep.txt" \
        || { note "the reading sweep failed: $(tail -3 "$tmp/inspect-sweep.txt")"; return; }
    [ "$(wc -l < "$tmp/inspect.jsonl")" -eq 6 ] \
        || note "three arms x two repetitions wrote $(wc -l < "$tmp/inspect.jsonl") lines, not 6"
    [ "$(jq -s -c 'map(.task) | unique' "$tmp/inspect.jsonl")" = '["inspect"]' ] \
        || note "the reading sweep is not one task: $(jq -s -c 'map(.task)' "$tmp/inspect.jsonl")"
}

test_a_run_that_answered_carries_its_tokens_and_is_not_retried() {
    jq -s -e 'all(.[]; .in_tokens != null and .in_tokens > 0)' "$tmp/offline.jsonl" > /dev/null 2>&1 \
        || note "a line carries no summed input tokens: $(jq -s -c 'map(.in_tokens)' "$tmp/offline.jsonl")"
    [ "$(jq -s -c 'map(.attempt) | unique' "$tmp/offline.jsonl")" = '[1]' ] \
        || note "a run that answered was retried: $(jq -s -c 'map(.attempt)' "$tmp/offline.jsonl")"
    [ "$(jq -s -c 'map(.attempt) | unique' "$tmp/inspect.jsonl")" = '[1]' ] \
        || note "a reading run that answered was retried: $(jq -s -c 'map(.attempt)' "$tmp/inspect.jsonl")"
}

# A run that never reached the model is not a measurement, so it is tried once
# more and only the line that came back is recorded — proven with a stubbed
# `run_one.sh`, because a real provider cannot be made to fail on demand and
# the loop is what is under test here, not the provider.
test_a_run_that_never_reached_the_model_is_tried_once_more() {
    mkdir -p "$tmp/stub"
    # the task names are `bench.py`'s, and the sweep asks for them before it
    # records anything, so the stub directory holds the two modules it reads
    cp "$BENCH/sweep.sh" "$BENCH/bench.py" "$BENCH/tasks.py" "$tmp/stub/"
    cat > "$tmp/stub/run_one.sh" <<'STUB'
#!/bin/sh
# attempt 1 answers nothing; attempt 2 answers with a bill
tokens=null
[ "$ATTEMPT" = 2 ] && tokens=500
printf '{"task":"refused-write","arm":"%s","tag":"%s","repetition":%s,"attempt":%s,"in_tokens":%s,"checks":{"ok":false}}\n' \
    "$1" "$TAG" "$REPETITION" "$ATTEMPT" "$tokens"
STUB
    chmod +x "$tmp/stub/run_one.sh"
    REPS=1 TAG=stub "$tmp/stub/sweep.sh" "$tmp/stub.jsonl" with > /dev/null 2> "$tmp/stub.txt" \
        || { note "the stubbed sweep failed: $(tail -3 "$tmp/stub.txt")"; return; }
    [ "$(wc -l < "$tmp/stub.jsonl")" -eq 1 ] \
        || note "a retried run wrote $(wc -l < "$tmp/stub.jsonl") lines, not 1"
    [ "$(jq -s -c 'map([.attempt, .in_tokens])' "$tmp/stub.jsonl")" = '[[2,500]]' ] \
        || note "the retry did not keep the second attempt: $(cat "$tmp/stub.jsonl")"
}

test_the_same_repetition_twice_in_one_file_is_refused() {
    before=$(wc -l < "$tmp/offline.jsonl")
    if REPS=1 BENCH_OFFLINE=1 BENCH_MODEL=mock TAG=selftest \
        "$BENCH/sweep.sh" "$tmp/offline.jsonl" with > /dev/null 2> "$tmp/refused.txt"; then
        note "a repetition already recorded was measured again"
    fi
    [ "$(wc -l < "$tmp/offline.jsonl")" -eq "$before" ] || note "the refused sweep still wrote a line"
    grep -q 'already holds' "$tmp/refused.txt" || note "the refusal did not say why"
}

test_the_reading_tasks_arms_are_told_apart_by_its_own_skill() {
    # The activation field names the task's skill and no other: the catalog ships
    # three bodies, and a load of one of the others is a load of something else.
    [ "$(jq -s -c 'map(select(.arm == "with") | .activated) | unique' "$tmp/inspect.jsonl")" = '[true]' ] \
        || note "the reading task's with arm did not load its own skill: $(jq -s -c 'map([.arm,.activated])' "$tmp/inspect.jsonl")"
    [ "$(jq -s -c 'map(select(.arm == "without") | .activated) | unique' "$tmp/inspect.jsonl")" = '[false]' ] \
        || note "the reading task's without arm activated a skill it does not have"
}

test_the_recorded_conversations_are_told_apart_by_the_activation_field() {
    [ "$(jq -s -c 'map(select(.arm == "with") | .activated) | unique' "$tmp/offline.jsonl")" = '[true]' ] \
        || note "the with arm's lines do not all say the skill was activated: $(jq -s -c 'map([.arm,.activated])' "$tmp/offline.jsonl")"
    [ "$(jq -s -c 'map(select(.arm == "without") | .activated) | unique' "$tmp/offline.jsonl")" = '[false]' ] \
        || note "the without arm activated a skill it does not have"
}

# The third arm is scored on whether the body came before the first command, and
# that is a fact of the transcript: the recorded conversations for the two shipped
# arms are the same work in a different order, so the field is what tells them
# apart — and the offline lines are the mock's, not a model's.
test_the_third_arm_is_scored_on_whether_the_load_came_first() {
    [ "$(jq -s -c 'map(select(.arm == "with") | .loaded_before_bash) | unique' "$tmp/offline.jsonl")" = '[true]' ] \
        || note "the with arm did not load before its first command: $(jq -s -c 'map([.arm,.loaded_before_bash])' "$tmp/offline.jsonl")"
    [ "$(jq -s -c 'map(select(.arm == "no-trigger") | .loaded_before_bash) | unique' "$tmp/offline.jsonl")" = '[false]' ] \
        || note "the no-trigger arm loaded before anything ran, so the arm is not the sentence: $(jq -s -c 'map([.arm,.loaded_before_bash])' "$tmp/offline.jsonl")"
}

test_the_rule_scores_the_two_recorded_conversations_differently() {
    # The with arm runs the suite and passes it; the without arm patches the
    # same file and claims the suite is green. `fixed` is true for both, and
    # `proved` is what the rule adds — which is the whole point of scoring by a
    # rule instead of by the answer's confidence.
    [ "$(jq -s -c 'map(select(.arm == "with") | [.checks.fixed, .checks.proved, .checks.ok]) | unique' "$tmp/offline.jsonl")" = '[[true,true,true]]' ] \
        || note "the with arm was not scored fixed and proved: $(jq -s -c 'map(select(.arm=="with")|.checks)' "$tmp/offline.jsonl")"
    [ "$(jq -s -c 'map(select(.arm == "without") | [.checks.fixed, .checks.proved, .checks.ok]) | unique' "$tmp/offline.jsonl")" = '[[true,false,false]]' ] \
        || note "the without arm was not scored the way the rule reads: $(jq -s -c 'map(select(.arm=="without")|.checks)' "$tmp/offline.jsonl")"
}

test_the_reading_task_is_scored_on_the_tokens_the_same_answer_took() {
    # Both arms answer the question — that is the premise of a reading task — and
    # the recorded conversations differ only in what reaching it cost, so the table
    # has to put one arm's mean below the other's and say the body earns its line.
    ( cd "$BENCH" && python3 bench.py report "$tmp/inspect.jsonl" ) > "$tmp/inspect.md" 2>&1
    grep -q '^| with | 2 | 2 |' "$tmp/inspect.md" \
        || note "the reading report has no with row: $(cat "$tmp/inspect.md")"
    grep -q 'scored on price' "$tmp/inspect.md" \
        || note "the reading report did not say what it scored: $(cat "$tmp/inspect.md")"
    grep -q '`search-and-read` earns its catalog line' "$tmp/inspect.md" \
        || note "the reading report did not name the body's place: $(cat "$tmp/inspect.md")"
    [ "$(jq -s -c 'map(.checks.ok) | unique' "$tmp/inspect.jsonl")" = '[true]' ] \
        || note "a recorded conversation failed the task's own answer check: $(jq -s -c 'map([.arm,.checks])' "$tmp/inspect.jsonl")"
}

test_the_report_is_a_table_of_all_three_arms() {
    ( cd "$BENCH" && python3 bench.py report "$tmp/offline.jsonl" ) > "$tmp/table.md" 2>&1
    grep -q '^| arm | runs | billed | mean input tokens | median input tokens | activated | loaded before bash | right |$' "$tmp/table.md" \
        || note "the report printed no header: $(cat "$tmp/table.md")"
    for arm in with without no-trigger; do
        grep -q "^| $arm | 2 |" "$tmp/table.md" || note "the report has no $arm row: $(cat "$tmp/table.md")"
    done
    grep -q '2/2' "$tmp/table.md" || note "the report does not count the rights: $(cat "$tmp/table.md")"
}

# The sentence is the third arm's number, so the report says what it came to:
# the offline lines are two runs of each, and the two shipped arms differ in the
# one fact the arm moves.
test_the_report_says_what_the_trigger_sentence_came_to() {
    grep -q '^### The sentence$' "$tmp/table.md" || note "the report printed no sentence: $(cat "$tmp/table.md")"
    grep -q 'earns its place' "$tmp/table.md" \
        || note "the report did not name the sentence's place: $(cat "$tmp/table.md")"
    grep -q '2 of 2 runs with the trigger sentence and 0 of 2 without it' "$tmp/table.md" \
        || note "the report did not count the runs the sentence moved: $(cat "$tmp/table.md")"
}

# The rule the page states is applied by the report, so a sweep arrives as a
# verdict and not as a table somebody reads by hand — offline, on the scripted
# conversations, which is all an arm of two repetitions can be shown to do.
test_the_report_applies_the_shipping_rule_to_the_sweep() {
    grep -q '^### The shipping rule$' "$tmp/table.md" || note "the report printed no rule: $(cat "$tmp/table.md")"
    for check in reached outcome price; do
        grep -q "^| $check |" "$tmp/table.md" || note "the rule has no $check row: $(cat "$tmp/table.md")"
    done
    grep -q '`sandbox-refusal`' "$tmp/table.md" || note "the rule named no skill: $(cat "$tmp/table.md")"
}

test_an_empty_file_has_measured_nothing() {
    ( cd "$BENCH" && python3 bench.py report "$tmp/never-written.jsonl" ) > "$tmp/empty.md" 2>&1
    grep -q 'not yet measured' "$tmp/empty.md" || note "an empty report claimed a number: $(cat "$tmp/empty.md")"
}

# In order: the first builds the arms, the second spends one offline sweep on
# them, and the rest read those six lines.
test_the_units_of_the_bench_pass
test_the_three_arms_differ_only_in_the_text_they_were_built_from
test_a_run_label_its_binary_contradicts_is_refused
test_the_offline_sweep_writes_one_line_per_arm_per_repetition
test_the_reading_task_is_a_sweep_of_its_own_file
test_a_run_that_answered_carries_its_tokens_and_is_not_retried
test_a_run_that_never_reached_the_model_is_tried_once_more
test_the_same_repetition_twice_in_one_file_is_refused
test_the_recorded_conversations_are_told_apart_by_the_activation_field
test_the_reading_tasks_arms_are_told_apart_by_its_own_skill
test_the_third_arm_is_scored_on_whether_the_load_came_first
test_the_rule_scores_the_two_recorded_conversations_differently
test_the_reading_task_is_scored_on_the_tokens_the_same_answer_took
test_the_report_is_a_table_of_all_three_arms
test_the_report_says_what_the_trigger_sentence_came_to
test_the_report_applies_the_shipping_rule_to_the_sweep
test_an_empty_file_has_measured_nothing

[ "$failed" -eq 0 ] && printf 'self-test: the ablation bench runs end to end, offline\n'
exit "$failed"
