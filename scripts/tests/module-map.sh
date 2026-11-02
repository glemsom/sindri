# The module map, driven as a process against a git tree of its own: a crate root
# is where the search starts, so the audit is proven on the shapes a reader meets
# there — a declaration trailed by a clause cut from the module's paragraph, and
# the four ways it stops being one.

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
