#!/usr/bin/env bash
# Proves the gate has teeth (spec §16; ticket #5). `make check` is the single
# definition of done, and a definition nobody has ever seen say no is a
# formality, so this stage makes the gate fail on purpose, three times.
#
# Each case writes one defect into the tree, drives the real `make check` over a
# two-stage subset, and requires three things of it: a non-zero exit, the exact
# line `check: <stage> ... FAIL`, and no line at all from the stage behind it.
# That is the defect caught by the stage that owns it, and the run stopped there
# with that stage named.
#
#   gate_probe_unformatted.rs   an unformatted file, which `fmt` must catch
#   gate_probe_clippy.rs        a warning, which `clippy -- -D warnings` must
#   gate_probe_panics.rs        a panicking `#[test]`, which `test` must
#
# NO RECURSION. A stage of `make check` that called `make check` would call
# itself for ever. The loop is driven through a stage subset instead —
# `make check STAGES='fmt clippy'`, the switch the Makefile documents — so
# `gate` is never in the list. The subset is also what proves the stopping: a
# stage that failed and a run that went on anyway print the same FAIL line, and
# only the silence behind it tells the two apart.
#
# NOTHING IS LEFT BEHIND. The defects are files this script writes and takes
# back, never edits to yours: three other tickets are landing in crates/ beside
# this one, and a probe that rewrote a source file would collide with them and
# could throw away work nobody had committed. `withdraw` runs from a trap on
# EXIT, INT, TERM and HUP, so a killed run takes its files with it, and the tree
# is compared against the `git status --porcelain` taken before the first file
# was written — compared, rather than required to be empty, because a
# developer's own uncommitted work is none of this stage's business. Every byte
# is restored by never writing over one.
#
# CHEAP. The stages reuse CARGO_TARGET_DIR like every other cargo call in the
# gate, so a defect costs the compile of one test target plus the suite `make
# check` was going to run anyway, not a build of the workspace. Measured on this
# machine, warm: 8 s for all three cases, against the 4 s `make test` already
# spends.
#
# WHAT THIS DOES NOT CATCH, said here so nobody has to guess. It reads the
# verdict and where the run stopped, not why the stage said no: a `fmt` that
# failed for an unrelated reason satisfies it. Nor can it prove the defect was
# the only thing wrong — a stage that was already failing for its own reasons
# reads exactly like one that caught the defect, which is why `make check` runs
# `gate` after `fmt`, `clippy` and `test` have already passed on this tree.
#
# Three verdicts, and the exit code is part of the contract:
#
#   0   gate: ok (N checks)                 every defect was caught by its stage
#   77  gate: skipped (<tool> unavailable)  this machine cannot drive the gate:
#                                           no make, cargo or git. The reason is
#                                           the line above. `make gate` relays it
#                                           as `check: gate ... SKIPPED` and keeps
#                                           exit 0, like the sandbox tier.
#   1   gate: FAIL (...)                    a defect went uncaught, or the tree
#                                           was not left as it was found
#
#     devbox run -- bash scripts/check-probe.sh
#
# scripts/check-probe-test.sh is the contract test for those three verdicts, and
# drives this script against a stand-in `make` so none of them needs a compile.
set -uo pipefail

# automake's "skipped" status, as scripts/sandbox-probe.sh uses it.
SKIPPED_STATUS=77

# The probe calls no external tool it can avoid, because two of its three
# verdicts are about tools that are not on PATH: a `dirname` borrowed from the
# machine would turn the skip this script exists to print into a crash.
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

# not_on_path TOOL -- this machine cannot drive the gate. The verdict line is
# printed here and nowhere else, so there is one wording to grep for.
not_on_path() {
  skip "$1 not on PATH"
  printf 'gate: skipped (%s unavailable)\n' "$1"
  exit "$SKIPPED_STATUS"
}

make_bin=$(command -v make) || not_on_path make
# cargo and git are not used to build anything here, but without them the stages
# could not run and the tree could not be read back, and a probe that guessed at
# either would be printing a verdict it did not earn.
command -v cargo >/dev/null 2>&1 || not_on_path cargo
command -v git >/dev/null 2>&1 || not_on_path git

# The defects go where cargo auto-discovers them: a file sitting directly under a
# tests tree is a test target of its own, with no declaration to wire up and no
# crate root touched. scripts/audit.sh says the same thing in its own words, and
# the audit's other three rules are satisfied by the paragraphs below, so a
# defect that outlived a signal would still be a tidy file.
tests_dir=crates/sindri-core/tests
defects=(gate_probe_unformatted.rs gate_probe_clippy.rs gate_probe_panics.rs)
# The name a defect is written under before it is published, and the file that
# must exist for the first time only after nothing else can complain about it.
# Named here, above the traps, because a trap may run at any moment after it is
# set and `set -u` would turn an unset name in one into a second failure.
tmp_name=.gate_probe.tmp

# withdraw -- take every defect back out, and the half-written one too.
withdraw() {
  local name
  for name in "${defects[@]}"; do rm -f "$tests_dir/$name"; done
  rm -f "$tests_dir/$tmp_name"
}
trap withdraw EXIT
# A killed run must leave nothing behind, and must not print the green line it
# did not earn. The signal trap exits, and the EXIT trap withdraws the defect.
trap 'printf "gate: FAIL (interrupted)\n"; exit 1' INT TERM HUP

# A defect is published with a rename rather than written in place, so the one
# kill a trap cannot answer — SIGKILL, which this script gets no chance to see —
# cannot leave a defect-named file that is empty or half written. This tree is
# shared with whoever else is building in it, and a file cargo would compile and
# then complain about is a worse thing to leave than the dotfile above.
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

# caught STAGE CANARY WHAT -- the stage must have failed over WHAT, and the run
# must not have reached CANARY. One ok line or one bad line, never both.
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

# prove STAGE CANARY WRITE WHAT -- write the defect, drive the loop, judge it,
# and take the defect back out whatever the verdict was.
prove() {
  "$3"
  out=$("$make_bin" --no-print-directory check STAGES="$1 $2" 2>&1)
  code=$?
  caught "$1" "$2" "$4"
  withdraw
}

# The tree as it was found, read before the first defect is written, so the last
# check compares like with like on a tree somebody may be working in.
before=$(git status --porcelain)

prove fmt clippy write_unformatted 'a deliberately unformatted file'
prove clippy test write_clippy 'a warning'
prove test bench write_panics 'a panicking test'

# Two readings of "left as it was found", because they fail differently: the
# names say which file survived, the whole-tree comparison catches anything else
# — a mark left in a directory, an index touched by accident.
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
