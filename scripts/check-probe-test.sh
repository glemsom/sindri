#!/usr/bin/env bash
set -uo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
probe="$repo_root/scripts/check-probe.sh"
bash_path=$(command -v bash)
make_path=$(command -v make)
scratch=$(mktemp -d "${TMPDIR:-/dev/shm}/sindri-check-probe-test.XXXXXX") || exit 1
trap 'rm -rf "$scratch"' EXIT

passed=0
failed=0
ok() { printf 'ok   %s\n' "$*"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n' "$*"; failed=$((failed + 1)); }

export CHECK_PROBE_RECORD="$scratch/make-saw.status"

tree_before=$(cd "$repo_root" && git status --porcelain)

for machine in blind mute runon honest hang; do
  mkdir -p "$scratch/machine-$machine"
done

cat >"$scratch/machine-blind/make" <<'SH'
#!/bin/sh
for arg in "$@"; do case $arg in STAGES=*) stages=${arg#STAGES=} ;; esac; done
set -- $stages
git status --porcelain >>"${CHECK_PROBE_RECORD:-/dev/null}" 2>/dev/null
printf 'check: %s ... ok\n' "${1:-fmt}"
exit 0
SH

cat >"$scratch/machine-mute/make" <<'SH'
#!/bin/sh
exit 1
SH

cat >"$scratch/machine-runon/make" <<'SH'
#!/bin/sh
for arg in "$@"; do case $arg in STAGES=*) stages=${arg#STAGES=} ;; esac; done
set -- $stages
printf 'check: %s ... FAIL\n' "${1:-fmt}"
[ -n "${2:-}" ] && printf 'check: %s ... ok\n' "$2"
exit 2
SH

cat >"$scratch/machine-honest/make" <<'SH'
#!/bin/sh
for arg in "$@"; do case $arg in STAGES=*) stages=${arg#STAGES=} ;; esac; done
set -- $stages
printf 'check: %s ... FAIL\n' "${1:-fmt}"
exit 2
SH

cat >"$scratch/machine-hang/make" <<'SH'
#!/bin/sh
for arg in "$@"; do case $arg in STAGES=*) stages=${arg#STAGES=} ;; esac; done
git status --porcelain >>"${CHECK_PROBE_RECORD:-/dev/null}" 2>/dev/null
printf '%s\n' "$$" >"${CHECK_PROBE_PIDFILE:-/dev/null}"
exec sleep 120
SH
chmod +x "$scratch"/machine-{blind,mute,runon,honest,hang}/make

no_make="$scratch/machine-no-make"
no_cargo="$scratch/machine-no-cargo"
mkdir -p "$no_make" "$no_cargo"
for tool in bash git cargo; do
  path=$(command -v "$tool") && ln -s "$path" "$no_make/$tool"
done
for tool in bash git make; do
  path=$(command -v "$tool") && ln -s "$path" "$no_cargo/$tool"
done

run_probe() {
  OUT=$(env PATH="$1" "$bash_path" "$probe" 2>&1)
  CODE=$?
}

run_make() {
  OUT=$(env PATH="$1" "$make_path" --no-print-directory -C "$repo_root" gate-tier 2>&1)
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
refute() { # substring desc -- no line may carry it
  if grep -qF -- "$1" <<<"$OUT"; then
    bad "$2: output carries '$1'"; printf '  output:\n%s\n' "$OUT"; else
    ok "$2: never says '$1'"; fi
}

run_probe "$no_make"
assert_code 77 "no make"
assert_line 'gate: skipped (make unavailable)' "no make"
refute 'gate: ok' "no make, no green line over a gate nobody rang"

run_probe "$no_cargo"
assert_code 77 "no cargo"
assert_line 'gate: skipped (cargo unavailable)' "no cargo"
refute 'gate: ok' "no cargo, no green line over a gate nobody rang"

run_probe "$scratch/machine-blind:$PATH"
assert_code 1 "blind make"
assert_has 'gate: FAIL (' "blind make"
assert_has 'fmt: exit 0, want non-zero' "blind make"
refute 'gate: ok' "blind make"
if grep -q 'gate_probe_' "$CHECK_PROBE_RECORD" 2>/dev/null; then
  ok "blind make: the probe had put the defect in the tree"
else
  bad "blind make: no defect was ever written into the tree"
fi

run_probe "$scratch/machine-mute:$PATH"
assert_code 1 "mute make"
assert_has "wants 'check: fmt ... FAIL'" "mute make"
refute 'gate: ok' "mute make"

run_probe "$scratch/machine-runon:$PATH"
assert_code 1 "run-on make"
assert_has 'the run went on to clippy' "run-on make"
refute 'gate: ok' "run-on make"

run_probe "$scratch/machine-honest:$PATH"
assert_code 0 "honest make"
assert_line 'gate: ok (5 checks)' "honest make"
assert_has 'ok   gate: fmt caught a deliberately unformatted file' "honest make"
assert_has 'ok   gate: clippy caught a warning' "honest make"
assert_has 'ok   gate: test caught a panicking test' "honest make"
assert_has 'ok   gate: the tree is left as it was found' "honest make"
assert_has 'ok   gate: no defect file survives the run' "honest make"
refute 'gate: FAIL' "honest make"

run_make "$scratch/machine-honest:$PATH"
assert_code 0 "make gate-tier, honest make"
assert_line 'check: gate ... ok' "make gate-tier, honest make"

run_make "$scratch/machine-blind:$PATH"
assert_code 2 "make gate-tier, blind make"
assert_line 'check: gate ... FAIL' "make gate-tier, blind make"
refute 'check: gate ... ok' "make gate-tier, blind make"

CHECK_PROBE_RECORD="$scratch/killed.status" CHECK_PROBE_PIDFILE="$scratch/killed.pid" \
  PATH="$scratch/machine-hang:$PATH" "$bash_path" "$probe" >"$scratch/killed.out" 2>&1 &
probe_pid=$!
for _ in $(seq 1 400); do [[ -s "$scratch/killed.pid" ]] && break; sleep 0.05; done
if [[ -s "$scratch/killed.pid" ]]; then
  ok "killed mid-run: the probe reached the stage"
else
  bad "killed mid-run: the probe never reached the stage"; cat "$scratch/killed.out"; fi
if grep -q 'gate_probe_unformatted.rs' "$scratch/killed.status" 2>/dev/null; then
  ok "killed mid-run: the first defect was in the tree when the signal landed"
else
  bad "killed mid-run: no defect was in the tree to be restored"
fi
kill -TERM "$(cat "$scratch/killed.pid" 2>/dev/null)" "$probe_pid" 2>/dev/null
wait "$probe_pid"
CODE=$?
if ((CODE != 0)); then ok "killed mid-run: exit $CODE, never 0"; else
  bad "killed mid-run: exit 0, want non-zero for a run that was killed"; fi
left=$(ls "$repo_root"/crates/sindri-core/tests/gate_probe_*.rs 2>/dev/null)
if [[ -z $left ]]; then ok "killed mid-run: no defect file survives"; else
  bad "killed mid-run: '$left' was left behind"; fi
if [[ $(cd "$repo_root" && git status --porcelain) == "$tree_before" ]]; then
  ok "killed mid-run: git status --porcelain reads as it did before"
else
  bad "killed mid-run: the tree changed"; (cd "$repo_root" && git status --porcelain) | diff - <(printf '%s\n' "$tree_before") || true
fi

OUT=$("$make_path" --no-print-directory -C "$repo_root" -n -p 2>/dev/null |
  sed -n 's/^STAGES := //p' | head -1)
if [[ $OUT == *gate* ]]; then
  ok "make check: STAGES lists gate"
else
  bad "make check: STAGES is '$OUT', want gate among them"
fi
OUT=$(cd "$repo_root" && "$make_path" --no-print-directory -n check 2>&1)
if [[ $OUT == *gate* ]]; then
  ok "make check: the loop reaches the gate stage"
else
  bad "make check: the loop never reaches the gate stage"
fi
OUT=$(cd "$repo_root" && "$make_path" --no-print-directory -n gate-tier 2>&1)
if [[ $OUT == *check-probe.sh* ]]; then
  ok "make gate-tier: runs the probe"
else
  bad "make gate-tier: does not reach the probe"
fi
if grep -q 'STAGES=' "$probe" 2>/dev/null; then
  ok "the probe drives the loop through a stage subset"
else
  bad "the probe never hands make a stage subset"
fi

printf '\n%s passed, %s failed\n' "$passed" "$failed"
(( failed == 0 ))
