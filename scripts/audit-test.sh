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

plant() { # root path line...
  local root=$1 path=$2
  shift 2
  if [[ $path == */* ]]; then mkdir -p "$root/${path%/*}"; fi
  printf '%s\n' "$@" >"$root/$path"
}
write_comment_block() { # root path marker count
  local root=$1 path=$2 marker=$3 count=$4 i
  local -a lines=()
  for ((i = 1; i <= count; i++)); do lines+=("$marker block line $i"); done
  plant "$root" "$path" "${lines[@]}"
}
write_citation() { # root path marker text
  plant "$1" "$2" "$3 $4"
}

clean="$scratch/clean"
mkdir -p "$clean/crates/demo/src" "$clean/crates/demo/tests"
cat >"$clean/crates/demo/src/lib.rs" <<'RS'
pub mod field;
RS
write_comment_block "$clean" crates/demo/src/field.rs '//' 3
printf 'pub struct Field;\n' >>"$clean/crates/demo/src/field.rs"
write_comment_block "$clean" Makefile '#' 3
write_comment_block "$clean" Cargo.toml '#' 2
write_comment_block "$clean" .github/workflows/ci.yml '#' 2
write_comment_block "$clean" scripts/demo.sh '#' 2
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
assert_code 0 "a tree whose blocks stop at three lines"
assert_rules "a tree whose blocks stop at three lines" \
  undeclared-test-file test-name comment-block comment-citation

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

for entry in \
  'crates/demo/src/field.rs|//' \
  'crates/demo/src/doc.rs|///' \
  'crates/demo/src/inner.rs|//!' \
  'scripts/demo.sh|#' \
  'Makefile|#' \
  'Cargo.toml|#' \
  '.github/workflows/ci.yml|#'; do
  IFS='|' read -r path marker <<<"$entry"
  block="$scratch/comment-block-${path//\//-}"
  cp -a "$clean" "$block"
  write_comment_block "$block" "$path" "$marker" 4
  run "$block"
  assert_code 1 "comment-block, $marker in $path"
  assert_line 'audit: comment-block ... FAIL' "comment-block, $marker in $path"
  assert_has "$path:1: a comment block runs 4 lines" "comment-block, $marker in $path"
  assert_line 'audit: comment-citation ... ok' "comment-block, $marker in $path leaves the citation rule alone"
done

citations=(
  'ADR-0020|cites an ADR'
  'adr/0020-the-artefact-is-one-static-musl-binary.md|cites an ADR'
  'ticket #72|cites an issue number'
  'issue #88|cites an issue number'
  '(#124)|cites an issue number'
  'spec §16|cites a spec section'
)
for path in crates/demo/src/field.rs scripts/demo.sh Makefile Cargo.toml .github/workflows/ci.yml; do
  marker='#'
  [[ $path == *.rs ]] && marker='//'
  for entry in "${citations[@]}"; do
    IFS='|' read -r text want <<<"$entry"
    cited="$scratch/cite-${path//\//-}-${text//[^A-Za-z0-9]/_}"
    cp -a "$clean" "$cited"
    write_citation "$cited" "$path" "$marker" "$text"
    run "$cited"
    assert_code 1 "comment-citation, '$text' in $path"
    assert_line 'audit: comment-citation ... FAIL' "comment-citation, '$text' in $path"
    assert_has "$path:1: $want" "comment-citation, '$text' in $path"
    assert_line 'audit: comment-block ... ok' "comment-citation, '$text' in $path leaves the block rule alone"
  done
done

for marker in '///' '//!'; do
  cited="$scratch/cite-${marker//[^A-Za-z0-9]/_}"
  cp -a "$clean" "$cited"
  write_citation "$cited" crates/demo/src/doc.rs "$marker" 'see ADR-0020 for why'
  run "$cited"
  assert_code 1 "comment-citation, $marker"
  assert_has 'crates/demo/src/doc.rs:1: cites an ADR' "comment-citation, $marker"
done

tight="$scratch/comment-citation-tight"
cp -a "$clean" "$tight"
printf '%s\n' '## ---' '#see ADR-0020 for why' >"$tight/Makefile"
run "$tight"
assert_code 1 "comment-citation, a comment with no space after the #"
assert_has 'Makefile:2: cites an ADR' "comment-citation, a comment with no space after the #"

for entry in \
  'scripts/demo.sh|cargo build # see ADR-0020 for why' \
  'Makefile|check: # ticket #72 repeats it' \
  'Cargo.toml|version = "0.1.0" # issue #88 asked' \
  '.github/workflows/ci.yml|runs-on: ubuntu-24.04 # (#124) copied it'; do
  IFS='|' read -r path line <<<"$entry"
  trailed="$scratch/trailed-${path//\//-}"
  cp -a "$clean" "$trailed"
  plant "$trailed" "$path" "$line"
  run "$trailed"
  assert_code 1 "comment-citation, trailing # in $path"
  assert_has "$path:1: cites" "comment-citation, trailing # in $path"
done

quoted="$scratch/comment-citation-quoted"
cp -a "$clean" "$quoted"
printf '%s\n' \
  'url="https://example.test/spec#12"' \
  'dir=${HOME#/home/}' \
  'printf "%s\\n" "$url" "$dir" # the names they are' \
  '# the spec#12 fragment names no issue' >"$quoted/scripts/demo.sh"
run "$quoted"
assert_code 0 "comment-citation: a # inside a string or a substitution is code, not a comment"

trailing="$scratch/comment-citation-trailing"
cp -a "$clean" "$trailing"
cat >"$trailing/crates/demo/src/field.rs" <<'RS'
pub const URL: &str = "https://example.test/spec#12";

pub fn field() {} // see ADR-0020 for why
RS
run "$trailing"
assert_code 1 "comment-citation, a trailing comment"
assert_has 'crates/demo/src/field.rs:3: cites an ADR' "comment-citation, a trailing comment"
if grep -q 'crates/demo/src/field.rs:1:' <<<"$OUT"; then
  bad "comment-citation: a // inside a string literal is code, not a comment"
else
  ok "comment-citation: a // inside a string literal is code, not a comment"
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
assert_rules "this tree" undeclared-test-file test-name comment-block comment-citation
if (( elapsed_ms < 1000 )); then ok "this tree: ${elapsed_ms}ms, under a second"; else
  bad "this tree: ${elapsed_ms}ms, want under a second"; fi

printf '\n%s passed, %s failed\n' "$passed" "$failed"
(( failed == 0 ))
