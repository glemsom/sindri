# The line-length audit, driven as a process against a git tree of its own: the
# limit and the one way out of it are proven on a made tree, not on the tree the
# audit happens to be run in. The one check that reads this tree is the last:
# the escape the ADR forbids has no file to be reinstated into, and that absence
# is asserted rather than assumed.

audit() { # the script under test, in this test's own tree
    set +e
    OUT=$("$SINDRI_ROOT/scripts/audit-line-length.sh" 2>"$PWD/stderr")
    STATUS=$?
    set -e
    ERR=$(cat "$PWD/stderr")
}

module() { # path, then the file's lines on stdin
    mkdir -p "$(dirname "$1")"
    cat >"$1"
}

long_file() { # path lines
    mkdir -p "$(dirname "$1")"
    for _ in $(seq "$2"); do echo '// a line'; done >"$1"
}

track() {
    git init -q
    git add -A
}

test_a_tree_under_the_limit_passes_and_names_the_limit() {
    long_file src/short.rs 3
    track
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "800"
}

test_a_file_over_the_limit_is_reported_by_path_and_size() {
    long_file src/long.rs 801
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "src/long.rs has 801 lines, over the 800-line limit"
}

test_the_only_way_out_is_to_split_the_file() {
    long_file src/long.rs 801
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "split the file"
    assert_contains_absent "$ERR" "allowlist"
}

test_no_allowlist_sits_there_to_be_reinstated() {
    assert_no_file "$SINDRI_ROOT/scripts/line-length-allowlist.txt"
}

test_only_rust_files_are_measured() {
    long_file scripts/thing.sh 900
    long_file GLOSSARY.md 900
    track
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "0 files"
}

test_a_file_written_but_not_staged_is_measured_too() {
    # The moment the rule matters most: a new file is where the split has not
    # happened yet, and it is not in the index when it is written.
    long_file src/tracked.rs 3
    track
    long_file src/written.rs 801
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "src/written.rs has 801 lines, over the 800-line limit"
}

test_a_gitignored_file_stays_out_of_the_audit() {
    printf '.scratch/\n' >.gitignore
    track
    long_file .scratch/notes.rs 801
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "0 files"
}
