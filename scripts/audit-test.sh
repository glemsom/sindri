#!/usr/bin/env bash
# Acceptance test for scripts/audit.sh, the four fast audits (spec §16).
# Each case builds a throwaway tree under $TMPDIR holding one violation, and
# asserts both the exit code and the `file:line` the audit names. Fixtures live
# in a temp dir on purpose: the audit scans every file it is given, so a
# deliberately-broken fixture inside the repo would be a real violation.
#
#     bash scripts/audit-test.sh
set -uo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
audit="$repo_root/scripts/audit.sh"
scratch=$(mktemp -d "${TMPDIR:-/dev/shm}/sindri-audit-test.XXXXXX") || exit 1
trap 'rm -rf "$scratch"' EXIT

passed=0
failed=0
ok() { printf 'ok   %s\n' "$*"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n' "$*"; failed=$((failed + 1)); }

# The one tree that satisfies every rule; each case copies it and breaks one rule.
clean="$scratch/clean"
mkdir -p "$clean/crates/demo/src" "$clean/crates/demo/tests"
cat >"$clean/crates/demo/src/lib.rs" <<'RS'
//! A demo crate whose opening paragraph carries well over the eight words the
//! audit asks for, and says what the crate is for.

pub mod field; // the patch applied to a record before it is written out
RS
cat >"$clean/crates/demo/src/field.rs" <<'RS'
//! The field is the patch applied to a record before it is written out, and
//! nothing else in the crate knows what a record is.
RS
cat >"$clean/crates/demo/tests/behaviour.rs" <<'RS'
//! Proves the demo behaves, so the audit has a test crate root to look at and
//! a declaration it can follow into a nested directory.

#[test]
fn demo_keeps_a_field() {}

#[test]
fn workspace_reaches_a_declaration() {}

#[test]
fn tests_name_the_behaviour() {}
RS

# run TREE -- runs the audit over TREE, capturing OUT and CODE.
run() {
  OUT=$(bash "$audit" "$1" 2>&1)
  CODE=$?
}
assert_code() { # want desc
  if [[ $CODE == "$1" ]]; then ok "$2: exit $CODE"; else
    bad "$2: exit $CODE, want $1"; printf '  output:\n%s\n' "$OUT"; fi
}
assert_line() { # exact-line desc
  if grep -qxF -- "$1" <<<"$OUT"; then ok "$2: '$1'"; else
    bad "$2: missing line '$1'"; printf '  output:\n%s\n' "$OUT"; fi
}
assert_has() { # substring desc
  if [[ $OUT == *"$1"* ]]; then ok "$2: names $1"; else
    bad "$2: never names $1"; printf '  output:\n%s\n' "$OUT"; fi
}

# 1. Every module opens with a //! paragraph of at least eight words.
case1="$scratch/paragraph"
cp -a "$clean" "$case1"
cat >"$case1/crates/demo/src/thin.rs" <<'RS'
//! Too short.
RS
run "$case1"
assert_code 1 "module-paragraph"
assert_line 'audit: module-paragraph ... FAIL' module-paragraph
assert_has 'crates/demo/src/thin.rs:1:' module-paragraph

# The line is eight words: seven is short, eight is enough.
case1b="$scratch/paragraph-boundary"
cp -a "$clean" "$case1b"
cat >"$case1b/crates/demo/src/seven.rs" <<'RS'
//! one two three four five six seven
RS
cat >"$case1b/crates/demo/src/eight.rs" <<'RS'
//! one two three four five six seven eight
RS
run "$case1b"
assert_has 'crates/demo/src/seven.rs:1:' "module-paragraph, seven words is short"
if [[ $OUT == *eight.rs* ]]; then
  bad "module-paragraph: eight words is enough, and was reported"
else
  ok "module-paragraph: eight words is enough"
fi

# 2. Every `mod` in a crate root is trailed by one clause cut from that module's
#    paragraph: the clause must be there, and it must be cut from that paragraph.
case2="$scratch/mod-clause"
cp -a "$clean" "$case2"
cat >"$case2/crates/demo/src/lib.rs" <<'RS'
//! A demo crate whose opening paragraph carries well over the eight words the
//! audit asks for, and says what the crate is for.

pub mod field;
RS
run "$case2"
assert_code 1 "mod-clause"
assert_line 'audit: mod-clause ... FAIL' mod-clause
assert_has 'crates/demo/src/lib.rs:4:' mod-clause
assert_line 'audit: module-paragraph ... ok' "mod-clause case leaves rule 1 alone"

case2b="$scratch/mod-clause-uncut"
cp -a "$clean" "$case2b"
cat >"$case2b/crates/demo/src/lib.rs" <<'RS'
//! A demo crate whose opening paragraph carries well over the eight words the
//! audit asks for, and says what the crate is for.

pub mod field; // a clause from some other paragraph entirely
RS
run "$case2b"
assert_code 1 "mod-clause, clause not cut"
assert_has 'crates/demo/src/lib.rs:4:' "mod-clause, clause not cut"

# 3. A file under a tests tree that no declaration names is reported where it
#    lies — and a nested file a declaration does reach is not reported, which is
#    the other half of the reading at the top of scripts/audit.sh.
case3="$scratch/undeclared"
cp -a "$clean" "$case3"
mkdir -p "$case3/crates/demo/tests/common"
cat >"$case3/crates/demo/tests/common/helper.rs" <<'RS'
//! A helper that patches a field the way a record would, and nothing more.
RS
run "$case3"
assert_code 1 "undeclared-test-file"
assert_line 'audit: undeclared-test-file ... FAIL' undeclared-test-file
assert_has 'crates/demo/tests/common/helper.rs:1:' undeclared-test-file
assert_line 'audit: module-paragraph ... ok' "undeclared case leaves rule 1 alone"

case3b="$scratch/declared"
cp -a "$clean" "$case3b"
mkdir -p "$case3b/crates/demo/tests/common"
cat >"$case3b/crates/demo/tests/behaviour.rs" <<'RS'
//! Proves the demo behaves, so the audit has a test crate root to look at and
//! a declaration it can follow into a nested directory.

mod common; // the helpers every behaviour test in this crate shares

#[test]
fn demo_keeps_a_field() {}
RS
cat >"$case3b/crates/demo/tests/common/mod.rs" <<'RS'
//! The helpers every behaviour test in this crate shares, in one place so each
//! behaviour test reaches them without keeping a copy of its own.

mod helper;
RS
cat >"$case3b/crates/demo/tests/common/helper.rs" <<'RS'
//! A helper that patches a field the way a record would, and nothing more.
RS
run "$case3b"
assert_code 0 "nested file a declaration reaches"
assert_line 'audit: undeclared-test-file ... ok' "nested file a declaration reaches"

# A file nested one level further is reported too: reach is transitive, and the
# walk stops where the declarations stop.
case3c="$scratch/undeclared-deep"
cp -a "$case3b" "$case3c"
mkdir -p "$case3c/crates/demo/tests/common/deep"
cat >"$case3c/crates/demo/tests/common/deep/helper.rs" <<'RS'
//! A helper two directories down, which `common` never declares, and so is
//! never compiled into any test target this crate builds.
RS
run "$case3c"
assert_code 1 "undeclared-test-file, one level deeper"
assert_has 'crates/demo/tests/common/deep/helper.rs:1:' "undeclared-test-file, one level deeper"

# 4. Every `#[test]` name states the behaviour: no test/it/works/check opener.
for opener in test_ it_ works_ check_; do
  case4="$scratch/test-name-${opener%_}"
  cp -a "$clean" "$case4"
  cat >"$case4/crates/demo/tests/behaviour.rs" <<RS
//! Proves the demo behaves, so the audit has a test crate root to look at and
//! a declaration it can follow into a nested directory.

#[test]
fn ${opener}demo_keeps_a_field() {}
RS
  run "$case4"
  assert_code 1 "test-name, '${opener}' opener"
  assert_line 'audit: test-name ... FAIL' "test-name, '${opener}' opener"
  assert_has 'crates/demo/tests/behaviour.rs:5:' "test-name, '${opener}' opener"
done
assert_line 'audit: undeclared-test-file ... ok' "test-name case leaves rule 3 alone"
if grep -q 'workspace_reaches_a_declaration\|tests_name_the_behaviour' <<<"$OUT"; then
  bad "test-name: a name is banned for its opener, not its first letters"
else
  ok "test-name: 'workspace_…' and 'tests_…' open with no banned opener"
fi

# Four violations, four independent failures: one broken tree, four FAIL lines.
case5="$scratch/all-four"
cp -a "$scratch/undeclared" "$case5"
cp -a "$case2/crates/demo/src/lib.rs" "$case5/crates/demo/src/lib.rs"
cp -a "$scratch/paragraph/crates/demo/src/thin.rs" "$case5/crates/demo/src/thin.rs"
cat >"$case5/crates/demo/tests/behaviour.rs" <<'RS'
//! Proves the demo behaves, so the audit has a test crate root to look at and
//! a declaration it can follow into a nested directory.

#[test]
fn it_keeps_a_field() {}
RS
run "$case5"
assert_code 1 "all four broken"
for rule in module-paragraph mod-clause undeclared-test-file test-name; do
  assert_line "audit: $rule ... FAIL" "all four broken"
done

# The whole tree as it stands must pass every rule, fast.
start=$(date +%s%N)
run "$repo_root"
elapsed_ms=$(( ($(date +%s%N) - start) / 1000000 ))
assert_code 0 "this tree"
assert_line 'audit: module-paragraph ... ok' "this tree"
assert_line 'audit: mod-clause ... ok' "this tree"
assert_line 'audit: undeclared-test-file ... ok' "this tree"
assert_line 'audit: test-name ... ok' "this tree"
if [[ $(grep -c '^audit: ' <<<"$OUT") == 4 ]]; then ok "this tree: one line per rule, four lines"; else
  bad "this tree: $(grep -c '^audit: ' <<<"$OUT") audit lines, want 4"; fi
if (( elapsed_ms < 1000 )); then ok "this tree: ${elapsed_ms}ms, under a second"; else
  bad "this tree: ${elapsed_ms}ms, want under a second"; fi

printf '\n%s passed, %s failed\n' "$passed" "$failed"
(( failed == 0 ))
