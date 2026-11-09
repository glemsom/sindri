#!/usr/bin/env bash
set -uo pipefail

SKIPPED_STATUS=77

case ${BASH_SOURCE[0]} in
  */*) here=${BASH_SOURCE[0]%/*} ;;
  *)   here=. ;;
esac
repo=$(cd "$here/.." && pwd) || exit 2
cd "$repo" || exit 2

checks=0
status=0
out=''
code=0
ok()   { printf 'ok   gate: %s\n' "$*"; checks=$((checks + 1)); }
skip() { printf 'skip gate: %s\n' "$*"; }
bad()  { printf 'FAIL gate: %s\n' "$*"; status=1; }
show() { printf '  output:\n%s\n' "$out"; }

not_on_path() {
  skip "$1 not on PATH"
  printf 'gate: skipped (%s unavailable)\n' "$1"
  exit "$SKIPPED_STATUS"
}

make_bin=$(command -v make) || not_on_path make
command -v cargo >/dev/null 2>&1 || not_on_path cargo
command -v git >/dev/null 2>&1 || not_on_path git

tests_dir=crates/sindri-core/tests
defects=(gate_probe_unformatted.rs gate_probe_clippy.rs gate_probe_panics.rs)
tmp_name=.gate_probe.tmp

withdraw() {
  local name
  for name in "${defects[@]}"; do rm -f "$tests_dir/$name"; done
  rm -f "$tests_dir/$tmp_name"
}
trap withdraw EXIT
trap 'printf "gate: FAIL (interrupted)\n"; exit 1' INT TERM HUP

write_defect() { # NAME -- the bytes arrive on stdin
  cat >"$tests_dir/$tmp_name" && mv -f "$tests_dir/$tmp_name" "$tests_dir/$1"
}

write_unformatted() {
  write_defect gate_probe_unformatted.rs <<'RS'
//! A deliberately unformatted file, written and then withdrawn by
//! scripts/check-probe.sh to prove that the fmt stage can fail.

pub fn spacing(left:i32,right:i32)->i32{
    left+right
}
RS
}

write_clippy() {
  write_defect gate_probe_clippy.rs <<'RS'
//! A file that compiles and a clippy warning, written and then withdrawn by
//! scripts/check-probe.sh to prove that the clippy stage can fail.

#[test]
fn an_empty_vector_has_no_length() {
    let values = Vec::<u8>::new();
    assert!(values.len() == 0);
}
RS
}

write_panics() {
  write_defect gate_probe_panics.rs <<'RS'
//! A deliberately failing test, written and then withdrawn by
//! scripts/check-probe.sh to prove that the test stage can fail.

#[test]
fn a_defect_makes_the_run_stop() {
    panic!("check-probe: deliberate failure");
}
RS
}

caught() {
  if ((code == 0)); then
    bad "$1: exit 0, want non-zero over $3"; show; return
  fi
  if [[ $out != *"check: $1 ... FAIL"* ]]; then
    bad "$1: wants 'check: $1 ... FAIL', got exit $code and no such line"; show; return
  fi
  if [[ $out == *"check: $1 ... ok"* ]]; then
    bad "$1: printed 'check: $1 ... ok' over $3"; show; return
  fi
  if [[ $out == *"check: $2"* ]]; then
    bad "$1: the run went on to $2 over $3"; show; return
  fi
  ok "$1 caught $3"
}

prove() {
  "$3"
  out=$("$make_bin" --no-print-directory check STAGES="$1 $2" 2>&1)
  code=$?
  caught "$1" "$2" "$4"
  withdraw
}

before=$(git status --porcelain)

prove fmt clippy write_unformatted 'a deliberately unformatted file'
prove clippy test write_clippy 'a warning'
prove test bench write_panics 'a panicking test'

left=()
for name in "${defects[@]}"; do
  [[ -e "$tests_dir/$name" ]] && left+=("$name")
done
if [[ ${#left[@]} -eq 0 ]]; then
  ok 'no defect file survives the run'
else
  bad "the tree still holds ${left[*]}"
fi

after=$(git status --porcelain)
if [[ $before == "$after" ]]; then
  ok 'the tree is left as it was found'
else
  bad "the tree changed: '${before//$'\n'/ }' -> '${after//$'\n'/ }'"
fi

if ((status)); then
  printf 'gate: FAIL (%s checks)\n' "$checks"
  exit 1
fi
printf 'gate: ok (%s checks)\n' "$checks"
exit 0
