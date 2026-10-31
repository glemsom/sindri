# The module-doc audit, driven as a process against a git tree of its own: a
# module is placed by its first line, so the audit is proven on the two shapes a
# reader meets — a paragraph first, and boilerplate first — rather than on the
# tree it happens to be run in.

audit() { # the script under test, in this test's own tree
    set +e
    OUT=$("$SINDRI_ROOT/scripts/audit-module-docs.sh" 2>"$PWD/stderr")
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

test_a_module_that_opens_with_its_paragraph_passes() {
    module src/loop.rs <<'RS'
//! What the loop proves, in one paragraph.

#![allow(clippy::expect_used, reason = "a test reports by panicking")]

mod support;
RS
    module crates/tui/tests/frame.rs <<'RS'
//! The frame's seam, proven through a pseudo-terminal.
RS
    track
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "2 files"
}

test_a_module_that_opens_with_an_attribute_is_reported_by_name() {
    module src/loop.rs <<'RS'
//! What the loop proves.

#![allow(clippy::expect_used, reason = "a test reports by panicking")]
RS
    module src/sandbox.rs <<'RS'
#![allow(clippy::expect_used, reason = "a test reports by panicking")]

//! What the sandbox proves.
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "src/sandbox.rs"
    assert_contains_absent "$ERR" "src/loop.rs"
}

test_a_file_with_no_paragraph_at_all_is_reported() {
    module src/allowed.rs <<'RS'
#![allow(dead_code)]
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "src/allowed.rs opens with"
}

test_an_empty_file_is_reported_rather_than_printed_blank() {
    module src/empty.rs </dev/null
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "src/empty.rs is empty"
}

test_only_rust_files_are_checked() {
    module scripts/thing.sh <<'SH'
#!/usr/bin/env bash
SH
    module README.md <<'MD'
# Nothing to place here.
MD
    track
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "0 files"
}
