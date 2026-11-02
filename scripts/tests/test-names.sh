# The test-name audit, driven as a process against a git tree of its own: a name
# is the spec written down, so the audit is proven on the two shapes a reader
# meets — a name that states the behaviour, and a name that states nothing —
# rather than on the tree it happens to be run in.

audit() { # the script under test, in this test's own tree
    set +e
    OUT=$("$SINDRI_ROOT/scripts/audit-test-names.sh" 2>"$PWD/stderr")
    STATUS=$?
    set -e
    ERR=$(cat "$PWD/stderr")
}

module() { # path, then the file's lines on stdin
    mkdir -p "$(dirname "$1")"
    cat >"$1"
}

track() {
    git init -q
    git add -A
}

test_a_name_that_states_the_behaviour_passes() {
    module src/loop.rs <<'RS'
#[test]
fn a_second_call_is_already_loaded_and_carries_no_body() {}

#[tokio::test]
async fn an_unrecoverable_setup_failure_exits_three_and_writes_nothing() {}
RS
    track
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "2 tests"
}

test_a_vague_name_is_reported_by_file_and_name() {
    module src/loop.rs <<'RS'
#[test]
fn test_bash() {}
RS
    module src/sandbox.rs <<'RS'
#[test]
fn the_sandbox_refuses_a_root_that_is_writable() {}
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "src/loop.rs"
    assert_contains "$ERR" "test_bash"
    assert_contains_absent "$ERR" "src/sandbox.rs"
}

test_a_name_under_four_words_is_reported_at_the_boundary() {
    # Four words is the floor, so three is reported and four is not: the pair is
    # what proves the count rather than the shape of the sentence.
    module src/three.rs <<'RS'
#[test]
fn reads_the_file() {}
RS
    module src/four.rs <<'RS'
#[test]
fn reads_the_file_once() {}
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "src/three.rs"
    assert_contains "$ERR" "reads_the_file"
    assert_contains_absent "$ERR" "src/four.rs"
}

test_each_prefix_that_names_the_fact_of_a_test_is_reported() {
    # The `#[test]` above the function already says it is a test, so the prefix
    # spends the name's first word saying it again. Each one is its own case
    # because a pattern that swallows one has swallowed the rule.
    module src/prefix.rs <<'RS'
#[test]
fn test_the_loop() {}

#[test]
fn it_loops() {}

#[test]
fn works() {}

#[test]
fn check_the_loop() {}
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "test_the_loop"
    assert_contains "$ERR" "it_loops"
    assert_contains "$ERR" "works"
    assert_contains "$ERR" "check_the_loop"
}

test_an_attribute_apart_from_its_function_still_finds_it() {
    # An outer attribute annotates the item that follows it, and rustfmt writes
    # it above rather than beside. The gap is stepped over, because a test lost
    # in one is a bad name nothing reports: a blank line, a further attribute,
    # a visibility modifier, or the whole item on the attribute's own line.
    module src/gap.rs <<'RS'
#[test]

fn test_after_a_blank_line() {}

#[tokio::test]
#[ignore]
async fn works_after_an_ignore() {}
RS
    module src/oneline.rs <<'RS'
#[test] fn test_on_one_line() {}
RS
    module src/visible.rs <<'RS'
#[test]
pub(crate) fn test_crate_visible() {}
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "test_after_a_blank_line"
    assert_contains "$ERR" "works_after_an_ignore"
    assert_contains "$ERR" "test_on_one_line"
    assert_contains "$ERR" "test_crate_visible"
}

test_a_run_of_underscores_is_not_a_word() {
    # The count is of words, not of separators: `reads__the_file` says three
    # things, however many underscores were typed between them.
    module src/three.rs <<'RS'
#[test]
fn reads__the_file() {}
RS
    module src/four.rs <<'RS'
#[test]
fn reads__the_file__once() {}
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "reads__the_file"
    assert_contains_absent "$ERR" "reads__the_file__once"
}

test_the_message_names_the_fix_rather_than_scolding() {
    module src/loop.rs <<'RS'
#[test]
fn test_loop() {}
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "sentence_name_the_case"
}

test_a_plain_function_is_not_a_test_and_is_not_read() {
    module src/loop.rs <<'RS'
fn reads_the_file() {}

fn test_bash() {}
RS
    track
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "0 tests"
}

test_only_rust_files_are_read() {
    module scripts/thing.sh <<'SH'
test_a_thing() { :; }
SH
    track
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "0 tests"
}

test_a_test_written_but_not_staged_is_read_too() {
    # The moment the rule matters most: the name is chosen as the test is
    # written, and the file is not in the index when it is written.
    module src/tracked.rs <<'RS'
#[test]
fn the_sandbox_refuses_a_root_that_is_writable() {}
RS
    track
    module src/written.rs <<'RS'
#[test]
fn test_written() {}
RS
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "src/written.rs"
    assert_contains_absent "$ERR" "src/tracked.rs"
}

test_a_gitignored_file_stays_out_of_the_audit() {
    printf '.scratch/\n' >.gitignore
    track
    module .scratch/draft.rs <<'RS'
#[test]
fn test_draft() {}
RS
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "0 tests"
}
