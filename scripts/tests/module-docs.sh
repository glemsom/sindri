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
//! What the loop proves, in one paragraph, at the seam.

#![allow(clippy::expect_used, reason = "a test reports by panicking")]

mod support;
RS
    module crates/tui/tests/frame.rs <<'RS'
//! The frame's seam, proven through a pseudo-terminal, byte by byte.
RS
    track
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "2 files"
}

test_a_module_that_opens_with_an_attribute_is_reported_by_name() {
    module src/loop.rs <<'RS'
//! What the loop proves, in one paragraph, at the seam.

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

test_a_bare_marker_is_reported_for_its_missing_words() {
    # A `//!` on its own satisfies the shape the audit reads and says nothing, so
    # the paragraph is held to words. The count spans the wrapped lines, because
    # that is how the paragraphs in this tree are written, and the eighth word is
    # the edge the rule turns on: seven is a stub, eight is a paragraph.
    module src/bare.rs <<'RS'
//!

fn main() {}
RS
    module src/thin.rs <<'RS'
//! The loop proves itself and nothing else.
RS
    module src/real.rs <<'RS'
//! The frame's seam, proven through
//! a pseudo-terminal, byte by byte.
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "src/bare.rs says //! and 8 words are not there"
    assert_contains "$ERR" "src/thin.rs says //! and 8 words are not there"
    assert_contains_absent "$ERR" "src/real.rs"
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

test_a_module_written_but_not_staged_is_checked_too() {
    # The audit sees what `git status` sees: a file an agent has just written is
    # checked before anyone stages it.
    module src/tracked.rs <<'RS'
//! What the loop proves, in one paragraph, at the seam.
RS
    track
    module src/written.rs <<'RS'
#![allow(dead_code)]
RS
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "src/written.rs opens with"
    assert_contains_absent "$ERR" "src/tracked.rs"
}

test_a_gitignored_module_stays_out_of_the_audit() {
    printf '.scratch/\n' >.gitignore
    track
    module .scratch/draft.rs <<'RS'
#![allow(dead_code)]
RS
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "0 files"
}
