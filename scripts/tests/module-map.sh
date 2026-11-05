# The module map, driven as a process against a git tree of its own: a crate root
# is where the search starts, so the audit is proven on the shapes a reader meets
# there — a declaration trailed by a clause cut from the module's paragraph, and
# the four ways it stops being one. It reads the other way too, so it is proven
# on the file nothing declares as well as the declaration nothing holds.

audit() { # the script under test, in this test's own tree
    set +e
    OUT=$("$SINDRI_ROOT/scripts/audit-module-map.sh" 2>"$PWD/stderr")
    STATUS=$?
    set -e
    ERR=$(cat "$PWD/stderr")
}

module() { # path, then the file's lines on stdin
    mkdir -p "$(dirname "$1")"
    cat >"$1"
}

crate_root() { # crate name, then the declarations on stdin
    module "crates/$1/src/lib.rs"
}

track() {
    git init -q
    git add -A
}

test_a_crate_root_that_places_every_module_passes() {
    module crates/core/src/sandbox.rs <<'RS'
//! The sandbox: one policy value, and the two derivations from it.
RS
    module crates/core/src/record.rs <<'RS'
//! The record every run leaves: `session.json` for metadata, and a
//! transcript for what happened in order.
RS
    crate_root core <<'RS'
//! The agent core.

pub mod record;  // the record every run leaves
pub mod sandbox; // the sandbox: one policy value
RS
    track
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "2 declarations"
}

test_a_declaration_with_no_clause_is_reported_by_its_line() {
    module crates/core/src/sandbox.rs <<'RS'
//! The sandbox: one policy value, and the two derivations from it.
RS
    crate_root core <<'RS'
//! The agent core.

pub mod sandbox;
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "crates/core/src/lib.rs:3 places sandbox with no clause"
}

test_a_clause_no_paragraph_holds_is_reported() {
    # A clause of its own is a second description, and the second one drifts: the
    # audit reads the module's paragraph and holds the clause to its words.
    module crates/core/src/sandbox.rs <<'RS'
//! The sandbox: one policy value, and the two derivations from it.
RS
    crate_root core <<'RS'
//! The agent core.

pub mod sandbox; // the jail the policy builds
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "crates/core/src/sandbox.rs does not say 'jail'"
}

test_a_bare_clause_is_not_a_placement() {
    # A word is a label, and a label sends the reader back to the file it was
    # meant to spare them: the clause is prose, so it is held to words.
    module crates/core/src/exec.rs <<'RS'
//! The bash executor: one port, two implementations.
RS
    crate_root core <<'RS'
//! The agent core.

pub mod exec; // executor
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "crates/core/src/lib.rs:3 places exec with a label, not a clause"
}

test_a_module_below_the_root_is_not_a_map() {
    # Only a crate root places modules: the modules under it are found by opening
    # it, so the rule stops where the search starts.
    module crates/core/src/agent/clock.rs <<'RS'
//! The wall clock a record is dated by.
RS
    module crates/core/src/agent/mod.rs <<'RS'
//! The agent: the turn loop and the tools it drives.

pub mod clock;
mod support;
RS
    module src/lib.rs <<'RS'
//! A library outside the workspace's crates, which places nothing for a reader.

pub mod thing;
RS
    track
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "0 declarations"
}

test_a_declaration_naming_no_file_is_reported() {
    crate_root core <<'RS'
//! The agent core.

pub mod sandbox; // the sandbox: one policy value
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "crates/core/src/lib.rs:3 declares sandbox, which has no file"
}

test_a_declaration_restricted_to_the_crate_is_read_too() {
    # A root may hold its modules to the crate; the reader asking "which file?"
    # is just as lost without a clause, so visibility is not the question.
    module crates/core/src/support.rs <<'RS'
//! The support a test states its provider answer in, and nothing else.
RS
    crate_root core <<'RS'
//! The agent core.

pub(crate) mod support;
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "crates/core/src/lib.rs:3 places support with no clause"
}

test_the_third_word_is_where_a_label_becomes_a_clause() {
    # The boundary is the audit's own: two words is a label and three is a
    # clause, so the shortest clause a module may carry is pinned rather than
    # left to taste.
    module crates/core/src/clock.rs <<'RS'
//! The wall clock a record is dated by.
RS
    module crates/core/src/exec.rs <<'RS'
//! The bash executor: one port, two implementations.
RS
    crate_root core <<'RS'
//! The agent core.

pub mod clock; // the wall clock
pub mod exec; // the executor
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "crates/core/src/lib.rs:4 places exec with a label, not a clause"
    assert_contains_absent "$ERR" "places clock"
}

test_a_clause_out_of_the_paragraphs_order_is_reported() {
    # The clause is cut, so it is a run of the paragraph's words: the same words
    # in another order are a second description, and the audit says which word
    # it could not find after the ones before it.
    module crates/core/src/sandbox.rs <<'RS'
//! The sandbox: one policy value, and the two derivations from it.
RS
    crate_root core <<'RS'
//! The agent core.

pub mod sandbox; // one value policy
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "crates/core/src/sandbox.rs does not say 'policy'"
}

test_a_test_module_nothing_declares_is_reported_by_path() {
    # A module nothing declares is a module nothing compiles, so the map reads
    # both ways: a file under a crate's tests tree with no declaration in front
    # of it is reported where it lies, and the message names the fix.
    module crates/tui/tests/common/mod.rs <<'RS'
//! Fixtures shared by the frame's tests.
RS
    module crates/tui/tests/frame/main.rs <<'RS'
//! The frame seam.

#[path = "../common/mod.rs"]
mod common;
RS
    module crates/tui/tests/frame/border.rs <<'RS'
//! The composer's border as a terminal is sent it.
RS
    track
    audit
    assert_status 1 "$STATUS"
    assert_contains "$ERR" "crates/tui/tests/frame/border.rs is a module nothing declares"
    assert_contains "$ERR" "declare it in the module that owns it, or delete it"
}

test_a_module_reached_through_a_path_attribute_is_named() {
    # The shared harness is included by path, under a name the file does not
    # carry: a declaration names wherever its attribute points, so a file
    # reached that way is as named as one beside a `mod` of its own name.
    module crates/cli/tests/common/harness.rs <<'RS'
//! The harness every test binary of the crate shares.
RS
    module crates/cli/tests/frame.rs <<'RS'
//! The frame, at its door.

#[path = "common/harness.rs"]
mod harness;
RS
    track
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "1 test files, every one is named by a declaration"
}

test_a_test_binary_cargo_names_is_not_reported() {
    # Cargo names the test binaries: `tests/foo.rs` is an entry point with no
    # declaration in front of it, so the map reads it as named and says so.
    module crates/core/tests/run.rs <<'RS'
//! One agent run, from the prompt to the record it leaves.
RS
    track
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "0 test files"
}

test_a_module_below_a_module_is_named_by_its_owners_directory() {
    # A module below a root takes its files into a directory named after
    # itself: `mark.rs` holds `mark/block.rs`, so the declaration in `mark.rs`
    # is the one that names it and the root is not asked again.
    module crates/tui/tests/frame/main.rs <<'RS'
//! The frame seam.

mod mark;
RS
    module crates/tui/tests/frame/mark.rs <<'RS'
//! The mark a drag leaves.

mod block;
RS
    module crates/tui/tests/frame/mark/block.rs <<'RS'
//! The mark over the one block whose copy differs from the screen.
RS
    track
    audit
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "2 test files, every one is named by a declaration"
}
