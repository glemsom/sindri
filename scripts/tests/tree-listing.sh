# The listing the four audits share, driven as a process against trees that are
# not git repositories: each audit walks what `git ls-files` says, so a tree it
# cannot list is an audit that never looked. It must fail and quote the listing's
# own error, because the alternative — reporting a clean sweep of nothing — is
# the gate saying the rule holds when nobody checked.

bash_bin=$(command -v bash)

audits() {
    printf '%s\n' audit-line-length.sh audit-module-docs.sh audit-module-map.sh audit-test-names.sh
}

run_audit() { # a PATH to run under, then the audit script's name
    set +e
    OUT=$(PATH=$1 "$bash_bin" "$SINDRI_ROOT/scripts/$2" 2>"$PWD/stderr")
    STATUS=$?
    set -e
    ERR=$(cat "$PWD/stderr")
}

# No git tree above this test's own directory, so the shape the issue names — an
# exported tarball, a build context — is the one the audits meet. Git climbs
# upwards, so the ceiling is the directory holding this one.
no_git_above() {
    export GIT_CEILING_DIRECTORIES=$(dirname "$PWD")
    set +e
    git rev-parse --show-toplevel >/dev/null 2>&1
    STATUS=$?
    set -e
    [ "$STATUS" -ne 0 ] || fail "this test's directory is inside a git tree after all"
}

test_an_audit_that_cannot_list_the_tree_fails_and_quotes_the_listing() {
    no_git_above
    for audit in $(audits); do
        run_audit "$PATH" "$audit"
        [ "$STATUS" -eq 1 ] \
            || fail "$audit: wanted exit 1 over a tree it could not list, got $STATUS"
        case "$ERR" in
            *"could not list the tree: fatal: not a git repository"*) ;;
            *) fail "$audit does not quote the listing's error: ${ERR:-no stderr}" ;;
        esac
        assert_empty "$OUT"
    done
}

test_an_audit_where_git_is_missing_fails_and_quotes_the_listing() {
    # A machine with no `git` is the same failure reached another way: the
    # listing never ran, and the audit quotes what the shell said about it.
    for audit in $(audits); do
        run_audit /nonexistent "$audit"
        [ "$STATUS" -eq 1 ] \
            || fail "$audit: wanted exit 1 with no git on the path, got $STATUS"
        case "$ERR" in
            *"could not list the tree:"*"git: command not found"*) ;;
            *) fail "$audit does not quote the listing's error: ${ERR:-no stderr}" ;;
        esac
        assert_empty "$OUT"
    done
}

test_the_listing_needs_no_tool_but_git() {
    # The pattern is matched by the shell, so `git` is the only tool the seam
    # calls: a `grep` that went missing would otherwise leave the same empty,
    # passing tree the seam exists to refuse.
    git_bin=$(command -v git)
    mkdir bin
    ln -s "$git_bin" bin/git
    printf '#!/bin/sh\n' >thing.sh
    git init -q
    git add -A
    for audit in $(audits); do
        run_audit "$PWD/bin" "$audit"
        assert_status 0 "$STATUS"
        assert_contains "$OUT" "audit: 0 "
        assert_empty "$ERR"
    done
}

test_a_git_tree_with_no_rust_file_passes_with_the_count_it_prints() {
    # The other side of the same seam: listed and empty is a pass, with the count
    # today's tree prints, because an empty listing that was read is not a
    # listing that failed.
    printf '#!/bin/sh\n' >thing.sh
    git init -q
    git add -A
    for audit in $(audits); do
        run_audit "$PATH" "$audit"
        assert_status 0 "$STATUS"
        assert_contains "$OUT" "audit: 0 "
    done
}
