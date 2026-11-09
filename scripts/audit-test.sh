#!/usr/bin/env bash
set -uo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
audit="$repo_root/scripts/audit.sh"
scratch=$(mktemp -d "${TMPDIR:-/dev/shm}/sindri-audit-test.XXXXXX") || exit 1
trap 'rm -rf "$scratch"' EXIT

passed=0
failed=0
ok() { printf 'ok   %s\n' "$*"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n' "$*"; failed=$((failed + 1)); }

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
assert_rules() { # desc rule...
  local desc=$1 got
  shift
  got=$(sed -n 's/^audit: \(.*\) \.\.\. .*/\1/p' <<<"$OUT" | tr '\n' ' ')
  if [[ $got == "$* " ]]; then ok "$desc: rules $*"; else
    bad "$desc: rules '$got', want '$* '"; printf '  output:\n%s\n' "$OUT"; fi
}

run "$clean"
assert_code 0 "a tree with no comments"
assert_rules "a tree with no comments" undeclared-test-file test-name

undeclared="$scratch/undeclared"
cp -a "$clean" "$undeclared"
mkdir -p "$undeclared/crates/demo/tests/common"
cat >"$undeclared/crates/demo/tests/common/helper.rs" <<'RS'
pub fn helper() {}
RS
run "$undeclared"
assert_code 1 "undeclared-test-file"
assert_line 'audit: undeclared-test-file ... FAIL' undeclared-test-file
assert_has 'crates/demo/tests/common/helper.rs:1:' undeclared-test-file
assert_line 'audit: test-name ... ok' "undeclared case leaves the other rule alone"

declared="$scratch/declared"
cp -a "$clean" "$declared"
mkdir -p "$declared/crates/demo/tests/common"
cat >"$declared/crates/demo/tests/behaviour.rs" <<'RS'
mod common;

#[test]
fn demo_keeps_a_field() {}
RS
cat >"$declared/crates/demo/tests/common/mod.rs" <<'RS'
mod helper;
RS
cat >"$declared/crates/demo/tests/common/helper.rs" <<'RS'
pub fn helper() {}
RS
run "$declared"
assert_code 0 "nested file a declaration reaches"
assert_line 'audit: undeclared-test-file ... ok' "nested file a declaration reaches"

deep="$scratch/undeclared-deep"
cp -a "$declared" "$deep"
mkdir -p "$deep/crates/demo/tests/common/deep"
cat >"$deep/crates/demo/tests/common/deep/helper.rs" <<'RS'
pub fn helper() {}
RS
run "$deep"
assert_code 1 "undeclared-test-file, one level deeper"
assert_has 'crates/demo/tests/common/deep/helper.rs:1:' "undeclared-test-file, one level deeper"

for opener in test_ it_ works_ check_; do
  named="$scratch/test-name-${opener%_}"
  cp -a "$clean" "$named"
  cat >"$named/crates/demo/tests/behaviour.rs" <<RS
#[test]
fn ${opener}demo_keeps_a_field() {}
RS
  run "$named"
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

both="$scratch/both"
cp -a "$undeclared" "$both"
cat >"$both/crates/demo/tests/behaviour.rs" <<'RS'
#[test]
fn it_keeps_a_field() {}
RS
run "$both"
assert_code 1 "both rules broken"
for rule in undeclared-test-file test-name; do
  assert_line "audit: $rule ... FAIL" "both rules broken"
done

start=$(date +%s%N)
run "$repo_root"
elapsed_ms=$(( ($(date +%s%N) - start) / 1000000 ))
assert_code 0 "this tree"
assert_rules "this tree" undeclared-test-file test-name
if (( elapsed_ms < 1000 )); then ok "this tree: ${elapsed_ms}ms, under a second"; else
  bad "this tree: ${elapsed_ms}ms, want under a second"; fi

printf '\n%s passed, %s failed\n' "$passed" "$failed"
(( failed == 0 ))
