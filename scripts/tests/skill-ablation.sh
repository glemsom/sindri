# The ablation bench's shell, driven as a process and with nothing that costs
# anything: no cargo build, no arm binary, no model. What is asserted is the
# bench's guard rails — the ones that stand between a mistake and a number
# nobody can trust, including the third arm's — because the pipeline itself is
# `bench-skill-ablation/self-test.sh`'s job and that one is manual, needs three
# builds and is not on this gate.

BENCH="$SINDRI_ROOT/scripts/bench-skill-ablation"

# Runs a script with the bench's own variables unset, so a developer's exported
# BENCH_MODEL or BENCH_OFFLINE cannot decide what a test sees.
detached() { # command…
    unset BENCH_MODEL BENCH_OFFLINE BENCH_BIN TAG REPETITION ATTEMPT REPS
    set +e
    "$@" >out 2>err
    status=$?
    set -e
    ERR=$(cat err)
}

test_a_build_of_an_arm_that_is_not_a_name_is_refused() {
    before=$(sha256sum "$SINDRI_ROOT/crates/core/src/skills/builtin.rs" \
        "$SINDRI_ROOT/crates/core/src/agent/skill/doctrine.md")
    detached "$BENCH/build_arm.sh" sideways
    assert_status 2 "$status"
    assert_contains "$ERR" "arm must be with, without or no-trigger"
    after=$(sha256sum "$SINDRI_ROOT/crates/core/src/skills/builtin.rs" \
        "$SINDRI_ROOT/crates/core/src/agent/skill/doctrine.md")
    [ "$before" = "$after" ] || fail "a refused build touched the text an arm is built from"
}

# The third arm is the shipped doctrine one line shorter, so the line has to be
# there: a doctrine that lost it would build the shipped arm under another name
# and the sweep would report one wording twice.
test_the_third_arms_text_is_the_shipped_doctrine_less_the_trigger_sentence() {
    shipped="$SINDRI_ROOT/crates/core/src/agent/skill/doctrine.md"
    # The sentence is the module's to name, so a shell never keeps a copy of it.
    sentence=$(python3 "$BENCH/doctrines.py" sentence)
    detached python3 "$BENCH/doctrines.py" without-trigger "$shipped"
    assert_status 0 "$status"
    assert_contains_absent "$(cat out)" "$sentence"
    [ "$(awk 'END { print NR }' out)" -eq "$(( $(awk 'END { print NR }' "$shipped") - 1 ))" ] \
        || fail "the third arm's text is not the shipped one with a single line removed"
}

test_a_doctrine_that_lost_the_trigger_sentence_is_refused() {
    printf 'Loads one skill into this conversation.\n' > doctrine.md
    detached python3 "$BENCH/doctrines.py" without-trigger doctrine.md
    [ "$status" -ne 0 ] || fail "a doctrine with no trigger sentence became an arm"
    assert_contains "$ERR" "no trigger sentence"
    assert_contains_absent "$(cat out)" "Loads one skill"
}

# The second task is the reading skill's, and it is a different question in a
# different repository: a name the bench does not hold would become a fixture path
# and a prompt a run half-reads, so it is refused before a run happens.
test_a_run_of_a_task_that_is_not_a_name_is_refused() {
    detached env BENCH_MODEL=mock BENCH_TASK=sideways "$BENCH/run_one.sh" with "$PWD/run"
    assert_status 2 "$status"
    assert_contains "$ERR" "the bench holds no task 'sideways'"
    assert_contains "$ERR" "it holds inspect"
}

test_a_sweep_of_a_task_that_is_not_a_name_is_refused() {
    detached env BENCH_TASK=sideways "$BENCH/sweep.sh" out.jsonl with
    assert_status 2 "$status"
    assert_contains "$ERR" "the bench holds no task 'sideways'"
    assert_no_file out.jsonl
}

test_a_record_line_for_a_task_that_is_not_a_name_is_refused() {
    mkdir -p run
    detached python3 "$BENCH/bench.py" record run --task sideways --arm with
    [ "$status" -ne 0 ] || fail "a line was recorded for a task that is not a name"
    assert_contains "$ERR" "the bench holds no task 'sideways': inspect"
}

# A run's label and its binary are two claims, and only the binary can be asked:
# the sentence is in the text it ships, so the words are what the question reads.
test_a_binary_is_asked_whether_it_carries_the_trigger_sentence() {
    printf 'x %s y\n' "$(python3 "$BENCH/doctrines.py" sentence)" > bin-carries
    printf 'Loads one skill into this conversation.\n' > bin-short
    detached python3 "$BENCH/doctrines.py" carries bin-carries
    assert_status 0 "$status"
    detached python3 "$BENCH/doctrines.py" carries bin-short
    assert_status 1 "$status"
}

test_a_run_of_an_arm_that_is_not_a_name_is_refused() {
    detached env BENCH_MODEL=mock "$BENCH/run_one.sh" sideways "$PWD/run"
    assert_status 2 "$status"
    assert_contains "$ERR" "arm must be with, without or no-trigger"
}

test_a_run_without_the_model_it_is_measured_on_is_refused() {
    detached "$BENCH/run_one.sh" with "$PWD/run"
    [ "$status" -ne 0 ] || fail "a run with no model was recorded"
    assert_contains "$ERR" "set BENCH_MODEL"
}

test_a_sweep_of_no_arm_is_refused() {
    detached "$BENCH/sweep.sh" out.jsonl
    assert_status 2 "$status"
    assert_contains "$ERR" "name at least one arm"
}

test_a_sweep_of_zero_repetitions_is_refused() {
    detached env REPS=0 "$BENCH/sweep.sh" out.jsonl with
    assert_status 2 "$status"
    assert_contains "$ERR" "REPS must be a positive number"
    assert_no_file out.jsonl
}

test_a_record_line_for_an_arm_that_is_not_a_name_is_refused() {
    mkdir -p run
    detached python3 "$BENCH/bench.py" record run --task inspect --arm sideways
    [ "$status" -ne 0 ] || fail "a line was recorded for an arm that is not a name"
    assert_contains "$ERR" "arm must be with, without or no-trigger"
}

# The third arm is a name a run is recorded under, and its line carries the fact
# it is scored on: whether a body entered the conversation before the first
# command, read from the transcript rather than from the arm's own label. A run
# that left no transcript reads null — absent is not zero, and a zero here would
# read as a run the sentence failed to reach.
test_a_record_line_for_the_third_arm_carries_the_fact_it_is_scored_on() {
    mkdir -p run
    detached python3 "$BENCH/bench.py" record run --task inspect --arm no-trigger
    assert_status 0 "$status"
    assert_contains "$(cat out)" '"arm": "no-trigger"'
    assert_contains "$(cat out)" '"loaded_before_bash": null'
}

# The bench's own arithmetic is tests/test_bench.py and tests/test_tasks.py, and
# they cost nothing: no model, no binary, no build. They are on this gate rather
# than only in the manual self-test because they are what a number is read with.
test_the_benchs_arithmetic_passes_its_own_unit_tests() {
    detached python3 -m unittest discover -s "$BENCH/tests"
    assert_status 0 "$status"
    assert_contains "$ERR" "OK"
}

test_a_report_over_a_file_no_sweep_wrote_claims_no_number() {
    detached python3 "$BENCH/bench.py" report never-written.jsonl
    assert_status 0 "$status"
    assert_contains "$(cat out)" "not yet measured"
}

# The rule the page states is applied by the report, so a sweep arrives as a
# verdict rather than as a table a reader scores by hand — and it is scored from
# the lines, which is what makes six repetitions of each arm the whole argument.
test_a_report_of_two_arms_applies_the_shipping_rule() {
    for rep in 1 2 3 4 5 6; do
        printf '{"task":"inspect","arm":"with","model":"m","repetition":%s,"in_tokens":1000,"activated":true,"checks":{"ok":true}}\n' "$rep"
        printf '{"task":"inspect","arm":"without","model":"m","repetition":%s,"in_tokens":1200,"activated":false,"checks":{"ok":false}}\n' "$rep"
    done > sweep.jsonl
    detached python3 "$BENCH/bench.py" report sweep.jsonl
    assert_status 0 "$status"
    assert_contains "$(cat out)" "### The shipping rule"
    assert_contains "$(cat out)" "| outcome | 6/6 with, 0/6 without |"
    assert_contains "$(cat out)" '`search-and-read-files` earns its catalog line:'
}

# The reading task's rule is the same three checks read on a second axis: a body
# that answered the same question for fewer tokens earns its line, and one that
# answered it for more does not — whatever its outcome was.
test_a_report_of_the_reading_task_applies_the_rule_to_its_price() {
    for rep in 1 2 3 4 5 6; do
        printf '{"task":"inspect","arm":"with","model":"m","repetition":%s,"in_tokens":600,"activated":true,"checks":{"ok":true}}\n' "$rep"
        printf '{"task":"inspect","arm":"without","model":"m","repetition":%s,"in_tokens":1400,"activated":false,"checks":{"ok":true}}\n' "$rep"
    done > sweep.jsonl
    detached python3 "$BENCH/bench.py" report sweep.jsonl
    assert_status 0 "$status"
    assert_contains "$(cat out)" "| outcome | 6/6 with, 6/6 without | every run with it right | yes |"
    assert_contains "$(cat out)" '`search-and-read-files` earns its catalog line'
    assert_contains "$(cat out)" "0.43x"
}

# A split that loses is a recorded verdict and not a silence: the reading task's
# numbers read the other way say so in the same table, and the gate holds the words.
test_a_reading_task_that_costs_more_says_so_in_the_table() {
    for rep in 1 2 3 4 5 6; do
        printf '{"task":"inspect","arm":"with","model":"m","repetition":%s,"in_tokens":1400,"activated":true,"checks":{"ok":true}}\n' "$rep"
        printf '{"task":"inspect","arm":"without","model":"m","repetition":%s,"in_tokens":600,"activated":false,"checks":{"ok":true}}\n' "$rep"
    done > sweep.jsonl
    detached python3 "$BENCH/bench.py" report sweep.jsonl
    assert_status 0 "$status"
    assert_contains "$(cat out)" "| price | 2.33x | under 1.00x | no |"
    assert_contains "$(cat out)" '`search-and-read-files` does not pay for itself'
    assert_contains_absent "$(cat out)" "earns its catalog line"
}

test_the_mock_provider_refuses_a_directory_of_no_replies() {
    mkdir -p empty
    detached python3 "$BENCH/mock_provider.py" empty
    [ "$status" -ne 0 ] || fail "the mock provider served a directory holding no reply"
    assert_contains "$ERR" "holds no recorded reply"
}
