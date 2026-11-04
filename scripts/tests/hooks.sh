# The pre-commit hook, driven as a process: a repository of the test's own, with
# the tree's hook and audits copied in, and a real `git commit` to carry them.
# What is asserted is what a developer meets — the commit refused, the audit's
# own words in the output, and the escape hatch still open — rather than the
# hook's text.
#
# The repository is isolated twice over: a null global config, so no template
# hook or `core.hooksPath` of the developer's is inherited, and a local
# `core.hooksPath` pointing at the copy, so a test that commits can only ever
# run the hook it shipped with.

isolate() {
    # Nothing of the developer's git is visible: a `--no-verify` this test
    # forgets, or a recursive `commit-msg` in a global hooks directory, would
    # otherwise reach the checkout the test is standing in.
    export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
    export GIT_AUTHOR_NAME='hook test' GIT_AUTHOR_EMAIL=hook@example.invalid
    export GIT_COMMITTER_NAME='hook test' GIT_COMMITTER_EMAIL=hook@example.invalid
    # A wrapper make would hand down to the hook's own make: `-n` in MAKEFLAGS
    # from `make scripts-check` would make the audits a dry run that passes.
    unset MAKEFLAGS MFLAGS MAKELEVEL GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE
    # The hook must not need a pinned toolchain; a missing devbox proves it.
    export DEVBOX=/bin/false
}

# A repository holding this tree's hook, audits and Makefile, so the commit
# under test runs the gate's own definition of the audits.
repo() {
    git init -q -b main .
    git config core.hooksPath .githooks
    cp -R "$SINDRI_ROOT/.githooks" .githooks
    mkdir -p scripts
    cp "$SINDRI_ROOT"/scripts/audit-*.sh "$SINDRI_ROOT"/scripts/tree-listing.sh scripts/
    cp "$SINDRI_ROOT/Makefile" .
    printf 'target\n' >.gitignore
}

module() { # path, then the file's lines on stdin
    mkdir -p "$(dirname "$1")"
    cat >"$1"
}

long_file() { # path lines
    mkdir -p "$(dirname "$1")"
    for _ in $(seq "$2"); do echo '// a line'; done >"$1"
}

commit() { # args to `git commit`
    set +e
    OUT=$(git commit "$@" 2>&1)
    STATUS=$?
    set -e
}

assert_committed() {
    git rev-parse --verify -q HEAD >/dev/null || fail "no commit was made: $OUT"
}

assert_no_commit() {
    ! git rev-parse --verify -q HEAD >/dev/null ||
        fail "the commit was made anyway: $OUT"
}

# The hook is a process of its own, run by git; `make audit` is what it calls,
# so the audits it runs are the audits the gate runs.
test_an_ordinary_commit_passes_and_costs_well_under_a_second() {
    isolate
    repo
    module src/fine.rs <<'RS'
//! What the module is for, in a paragraph of its own, before any attribute.

pub fn fine() {}

#[test]
fn a_short_name_is_fine() {}
RS
    git add -A
    start=$(date +%s%N)
    commit -m 'a change with nothing to answer for'
    elapsed_ms=$((($(date +%s%N) - start) / 1000000))
    assert_status 0 "$STATUS"
    assert_committed
    [ "$elapsed_ms" -lt 1000 ] ||
        fail "an ordinary commit took ${elapsed_ms}ms, not well under a second"
}

test_a_commit_that_trips_the_line_limit_is_refused_with_that_message() {
    isolate
    repo
    long_file src/long.rs 801
    git add -A
    commit -m 'a file that has outgrown the limit'
    assert_status 1 "$STATUS"
    assert_no_commit
    assert_contains "$OUT" "src/long.rs has 801 lines, over the 800-line limit"
    assert_contains "$OUT" "split the file"
}

test_a_commit_that_trips_the_module_docs_audit_is_refused_with_that_message() {
    isolate
    repo
    module src/bare.rs <<'RS'
pub fn bare() {}
RS
    git add -A
    commit -m 'a module with no paragraph'
    assert_status 1 "$STATUS"
    assert_no_commit
    assert_contains "$OUT" "src/bare.rs opens with"
    assert_contains "$OUT" "open the module with a //! paragraph"
}

test_a_commit_that_trips_the_test_name_audit_is_refused_with_that_message() {
    isolate
    repo
    module src/named.rs <<'RS'
//! What the module is for, in a paragraph of its own, before any attribute.

#[test]
fn test_it() {}
RS
    git add -A
    commit -m 'a test named for the fact that it is a test'
    assert_status 1 "$STATUS"
    assert_no_commit
    assert_contains "$OUT" "opens with \`test\`"
}

test_no_verify_gets_you_out_when_you_insist() {
    isolate
    repo
    long_file src/long.rs 801
    git add -A
    commit --no-verify -m 'the audits are wrong this one time'
    assert_status 0 "$STATUS"
    assert_committed
}

test_make_hooks_is_what_turns_the_hook_on() {
    isolate
    repo
    # Off by default: a fresh clone has no hooks until it asks for them.
    git config --unset core.hooksPath
    long_file src/long.rs 801
    git add -A
    commit -m 'not audited, because nothing is wired yet'
    assert_status 0 "$STATUS"
    assert_committed

    make -s -f "$SINDRI_ROOT/Makefile" hooks >/dev/null
    [ "$(git config core.hooksPath)" = .githooks ] ||
        fail "make hooks left core.hooksPath at '$(git config core.hooksPath)'"
    long_file src/later.rs 801
    git add -A
    commit -m 'and now the audits run before the commit lands'
    assert_status 1 "$STATUS"
    assert_contains "$OUT" "src/later.rs has 801 lines, over the 800-line limit"
}

test_the_hook_that_ships_runs_directly() {
    assert_executable "$SINDRI_ROOT/.githooks/pre-commit"
}
