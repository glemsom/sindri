# The ablation bench's shell, driven as a process and with nothing that costs
# anything: no cargo build, no arm binary, no model. What is asserted is the
# bench's guard rails — the ones that stand between a mistake and a number
# nobody can trust — because the pipeline itself is `bench-skill-ablation/
# self-test.sh`'s job and that one is manual, needs two builds and is not on
# this gate.

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
    before=$(sha256sum "$SINDRI_ROOT/crates/core/src/skills/builtin.rs")
    detached "$BENCH/build_arm.sh" sideways
    assert_status 2 "$status"
    assert_contains "$ERR" "arm must be with or without"
    after=$(sha256sum "$SINDRI_ROOT/crates/core/src/skills/builtin.rs")
    [ "$before" = "$after" ] || fail "a refused build touched the built-in table"
}

test_a_run_of_an_arm_that_is_not_a_name_is_refused() {
    detached env BENCH_MODEL=mock "$BENCH/run_one.sh" sideways "$PWD/run"
    assert_status 2 "$status"
    assert_contains "$ERR" "arm must be with or without"
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
    detached python3 "$BENCH/bench.py" record run --task refused-write --arm sideways
    assert_status 2 "$status"
    assert_contains "$ERR" "invalid choice"
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
        printf '{"task":"refused-write","arm":"with","model":"m","repetition":%s,"in_tokens":1000,"activated":true,"checks":{"ok":true}}\n' "$rep"
        printf '{"task":"refused-write","arm":"without","model":"m","repetition":%s,"in_tokens":1200,"activated":false,"checks":{"ok":false}}\n' "$rep"
    done > sweep.jsonl
    detached python3 "$BENCH/bench.py" report sweep.jsonl
    assert_status 0 "$status"
    assert_contains "$(cat out)" "### The shipping rule"
    assert_contains "$(cat out)" "| outcome | 6/6 with, 0/6 without |"
    assert_contains "$(cat out)" '`sandbox-refusal` earns its catalog line.'
}

test_the_mock_provider_refuses_a_directory_of_no_replies() {
    mkdir -p empty
    detached python3 "$BENCH/mock_provider.py" empty
    [ "$status" -ne 0 ] || fail "the mock provider served a directory holding no reply"
    assert_contains "$ERR" "holds no recorded reply"
}
