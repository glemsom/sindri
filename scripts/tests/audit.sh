# `make audit` driven as a process: a repository of the test's own holding the
# tree's Makefile and its audits, and a real `make` to carry them. What is
# asserted is what a developer meets — every audit printing its own words even
# when an earlier one refused, the target's status still a refusal, and an audit
# script the Makefile has never heard of running all the same — rather than the
# recipe's text.
#
# That last one is the whole reason the target reads the directory: a list of
# scripts in the Makefile is a second place to edit, and the commit hook calls
# this target so that it can hold no list of its own.

isolate() {
    # A wrapper make would hand down to the audits' own make: `-n` in MAKEFLAGS
    # from `make scripts-check` would make every audit a dry run that passes.
    unset MAKEFLAGS MFLAGS MAKELEVEL GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE
}

# A repository holding this tree's Makefile and audits, so the target under test
# is the gate's own definition of the fast audits.
repo() {
    git init -q -b main .
    mkdir -p scripts
    cp "$SINDRI_ROOT"/scripts/audit-*.sh "$SINDRI_ROOT"/scripts/tree-listing.sh scripts/
    cp "$SINDRI_ROOT/Makefile" .
    printf 'target\n' >.gitignore
}

# `make` reports a failed recipe as its own status 2, and the hook turns that
# into the 1 a commit is refused with: what the target owes anyone scripting it
# is only that a broken tree never reads as zero.
assert_refused() { # status
    [ "$1" -ne 0 ] || fail "the target exited 0 with an audit broken: $OUT"
}

audit() {
    set +e
    OUT=$(make -s -f Makefile audit 2>&1)
    STATUS=$?
    set -e
}

test_a_tree_that_breaks_two_rules_is_told_of_both_audits() {
    isolate
    repo
    long_file src/long.rs 801
    module src/bare.rs <<'RS'
pub fn bare() {}
RS
    git add -A
    audit
    assert_refused "$STATUS"
    assert_contains "$OUT" "src/long.rs has 801 lines, over the 800-line limit"
    assert_contains "$OUT" "src/bare.rs opens with"
}

test_a_new_audit_script_reaches_the_target_with_no_second_list() {
    isolate
    repo
    cat >scripts/audit-fixture.sh <<'SH'
#!/usr/bin/env bash
# An audit the Makefile was never told about.
echo "audit: the fixture's own words" >&2
exit 1
SH
    chmod +x scripts/audit-fixture.sh
    git add -A
    audit
    assert_refused "$STATUS"
    assert_contains "$OUT" "the fixture's own words"
    # The fixture sorts first, so the audits after it running is the point.
    assert_contains "$OUT" "every module opens with its own paragraph"
}

test_a_clean_tree_passes_and_every_audit_prints_its_count() {
    isolate
    repo
    module src/fine.rs <<'RS'
//! What the module is for, in a paragraph of its own, before any attribute.

pub fn fine() {}

#[test]
fn a_short_name_is_fine() {}
RS
    git add -A
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "all at or under 800 lines"
    assert_contains "$OUT" "every module opens with its own paragraph"
}
