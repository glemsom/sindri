#!/usr/bin/env bash
# Acceptance test for scripts/audit.sh. Each case builds a throwaway tree holding
# one violation and asserts the exit code and the `file:line` it names; fixtures
# live in $TMPDIR because the audit scans every file inside the repo.
set -uo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
audit="$repo_root/scripts/audit.sh"
scratch=$(mktemp -d "${TMPDIR:-/dev/shm}/sindri-audit-test.XXXXXX") || exit 1
trap 'rm -rf "$scratch"' EXIT

passed=0
failed=0
ok() { printf 'ok   %s\n' "$*"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n' "$*"; failed=$((failed + 1)); }

# The one tree that satisfies every rule, carrying no comment at all: each case
# copies it and breaks one rule.
clean="$scratch/clean"
mkdir -p "$clean/crates/demo/src" "$clean/crates/demo/tests"
cat >"$clean/crates/demo/src/lib.rs" <<'RS'
pub mod field;
RS
cat >"$clean/crates/demo/src/field.rs" <<'RS'
pub struct Field;
RS
cat >"$clean/crates/demo/tests/behaviour.rs" <<'RS'
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
assert_no_line() { # exact-line desc
  if grep -qxF -- "$1" <<<"$OUT"; then
    bad "$2: unexpected line '$1'"; printf '  output:\n%s\n' "$OUT"
  else ok "$2: prints no '$1'"; fi
}

# No rule reads a comment: a tree that carries none passes every rule.
run "$clean"
assert_code 0 "a tree with no comments"
assert_line 'audit: undeclared-test-file ... ok' "a tree with no comments"
assert_line 'audit: test-name ... ok' "a tree with no comments"
if [[ $(grep -c '^audit: ' <<<"$OUT") == 2 ]]; then ok "a tree with no comments: two rules, two lines"; else
  bad "a tree with no comments: $(grep -c '^audit: ' <<<"$OUT") audit lines, want 2"; fi

# 1. A file under a tests tree that no declaration names is reported where it
#    lies — and a nested file a declaration does reach is not reported.
case1="$scratch/undeclared"
cp -a "$clean" "$case1"
mkdir -p "$case1/crates/demo/tests/common"
cat >"$case1/crates/demo/tests/common/helper.rs" <<'RS'
pub fn helper() {}
RS
run "$case1"
assert_code 1 "undeclared-test-file"
assert_line 'audit: undeclared-test-file ... FAIL' undeclared-test-file
assert_has 'crates/demo/tests/common/helper.rs:1:' undeclared-test-file
assert_line 'audit: test-name ... ok' "undeclared case leaves the other rule alone"

case1b="$scratch/declared"
cp -a "$clean" "$case1b"
mkdir -p "$case1b/crates/demo/tests/common"
cat >"$case1b/crates/demo/tests/behaviour.rs" <<'RS'
mod common;

#[test]
fn demo_keeps_a_field() {}
RS
cat >"$case1b/crates/demo/tests/common/mod.rs" <<'RS'
mod helper;
RS
cat >"$case1b/crates/demo/tests/common/helper.rs" <<'RS'
pub fn helper() {}
RS
run "$case1b"
assert_code 0 "nested file a declaration reaches"
assert_line 'audit: undeclared-test-file ... ok' "nested file a declaration reaches"

# A file nested one level further is reported too: reach is transitive, and the
# walk stops where the declarations stop.
case1c="$scratch/undeclared-deep"
cp -a "$case1b" "$case1c"
mkdir -p "$case1c/crates/demo/tests/common/deep"
cat >"$case1c/crates/demo/tests/common/deep/helper.rs" <<'RS'
pub fn helper() {}
RS
run "$case1c"
assert_code 1 "undeclared-test-file, one level deeper"
assert_has 'crates/demo/tests/common/deep/helper.rs:1:' "undeclared-test-file, one level deeper"

# 2. Every `#[test]` name states the behaviour: no test/it/works/check opener.
for opener in test_ it_ works_ check_; do
  case2="$scratch/test-name-${opener%_}"
  cp -a "$clean" "$case2"
  cat >"$case2/crates/demo/tests/behaviour.rs" <<RS
#[test]
fn ${opener}demo_keeps_a_field() {}
RS
  run "$case2"
  assert_code 1 "test-name, '${opener}' opener"
  assert_line 'audit: test-name ... FAIL' "test-name, '${opener}' opener"
  assert_has 'crates/demo/tests/behaviour.rs:2:' "test-name, '${opener}' opener"
done
assert_line 'audit: undeclared-test-file ... ok' "test-name case leaves the other rule alone"
if grep -q 'workspace_reaches_a_declaration\|tests_name_the_behaviour' <<<"$OUT"; then
  bad "test-name: a name is banned for its opener, not its first letters"
else
  ok "test-name: 'workspace_…' and 'tests_…' open with no banned opener"
fi

# Two violations, two independent failures: one broken tree, two FAIL lines.
case3="$scratch/both"
cp -a "$case1" "$case3"
cat >"$case3/crates/demo/tests/behaviour.rs" <<'RS'
#[test]
fn it_keeps_a_field() {}
RS
run "$case3"
assert_code 1 "both rules broken"
for rule in undeclared-test-file test-name; do
  assert_line "audit: $rule ... FAIL" "both rules broken"
done

# Neither rule that mandated a comment survives, so neither prints a line.
for rule in module-paragraph mod-clause; do
  assert_no_line "audit: $rule ... ok" "removed rule"
done

# The whole tree as it stands must pass every rule, fast.
start=$(date +%s%N)
run "$repo_root"
elapsed_ms=$(( ($(date +%s%N) - start) / 1000000 ))
assert_code 0 "this tree"
assert_line 'audit: undeclared-test-file ... ok' "this tree"
assert_line 'audit: test-name ... ok' "this tree"
if [[ $(grep -c '^audit: ' <<<"$OUT") == 2 ]]; then ok "this tree: one line per rule, two lines"; else
  bad "this tree: $(grep -c '^audit: ' <<<"$OUT") audit lines, want 2"; fi
if (( elapsed_ms < 1000 )); then ok "this tree: ${elapsed_ms}ms, under a second"; else
  bad "this tree: ${elapsed_ms}ms, want under a second"; fi

printf '\n%s passed, %s failed\n' "$passed" "$failed"
(( failed == 0 ))
