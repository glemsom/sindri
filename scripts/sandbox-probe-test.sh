#!/usr/bin/env bash
set -uo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
probe="$repo_root/scripts/sandbox-probe.sh"
bash_path=$(command -v bash)
make_path=$(command -v make)
scratch=$(mktemp -d "${TMPDIR:-/dev/shm}/sindri-sandbox-probe-test.XXXXXX") || exit 1
trap 'rm -rf "$scratch"' EXIT

passed=0
failed=0
ok() { printf 'ok   %s\n' "$*"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n' "$*"; failed=$((failed + 1)); }

none="$scratch/machine-none"
mkdir -p "$none"
ln -s "$bash_path" "$none/bash"

for machine in broken restricted working; do
  mkdir -p "$scratch/machine-$machine"
done
broken="$scratch/machine-broken:/usr/bin:/bin"
restricted="$scratch/machine-restricted:/usr/bin:/bin"
working="$scratch/machine-working:/usr/bin:/bin"

cat >"$scratch/machine-broken/bwrap" <<'SH'
#!/bin/sh
echo 'bwrap: cannot execute: Exec format error' >&2
exit 1
SH

cat >"$scratch/machine-restricted/bwrap" <<'SH'
#!/bin/sh
if [ "${1:-}" = --version ]; then
  echo 'bubblewrap 0.12.0 (fake)'
  exit 0
fi
echo 'bwrap: setting up uid map: Permission denied' >&2
exit 1
SH

cat >"$scratch/machine-working/bwrap" <<'SH'
#!/bin/sh
if [ "${1:-}" = --version ]; then
  echo 'bubblewrap 0.12.0 (fake)'
  exit 0
fi
case "$*" in *ns/pid*) echo 'pid:[4026531000]' ;; esac
exit 0
SH
chmod +x "$scratch"/machine-{broken,restricted,working}/bwrap

run_probe() {
  OUT=$(env PATH="$1" "$bash_path" "$probe" 2>&1)
  CODE=$?
}

run_make() {
  OUT=$(env PATH="$1" "$make_path" --no-print-directory -C "$repo_root" sandbox-tier 2>&1)
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

run_probe "$none"
assert_code 77 "no bwrap"
assert_line 'sandbox tier: skipped (bwrap unavailable)' "no bwrap"
refute 'sandbox tier: ok' "no bwrap, no green tick over uncovered code"

run_probe "$broken"
assert_code 1 "broken bwrap"
assert_line 'sandbox tier: FAIL (bwrap on PATH but unusable)' "broken bwrap"
refute 'sandbox tier: skipped' "broken bwrap is a failure, not a skip"

run_probe "$restricted"
assert_code 77 "restricted kernel"
assert_line 'sandbox tier: skipped (bwrap unavailable)' "restricted kernel"
assert_has 'bwrap cannot create an unprivileged sandbox' "restricted kernel"
refute 'sandbox tier: ok' "restricted kernel, no green tick over uncovered code"

run_probe "$working"
assert_code 0 "working bwrap"
assert_line 'sandbox tier: ok (3 checks)' "working bwrap"
assert_has 'ok   sandbox: bwrap --version: bubblewrap 0.12.0 (fake)' "working bwrap"
assert_has 'bwrap unshares a pid namespace' "working bwrap"
refute 'sandbox tier: skipped' "working bwrap is not a skip"

run_make "$working"
assert_code 0 "make sandbox-tier, working bwrap"
assert_line 'check: sandbox ... ok' "make sandbox-tier, working bwrap"

run_make "$none"
assert_code 0 "make sandbox-tier, no bwrap"
assert_line 'check: sandbox ... SKIPPED' "make sandbox-tier, no bwrap"
refute 'check: sandbox ... ok' "make sandbox-tier, no bwrap"

run_make "$restricted"
assert_code 0 "make sandbox-tier, restricted kernel"
assert_line 'check: sandbox ... SKIPPED' "make sandbox-tier, restricted kernel"
assert_line 'sandbox tier: skipped (bwrap unavailable)' "make sandbox-tier, restricted kernel"

run_make "$broken"
assert_code 2 "make sandbox-tier, broken bwrap"
assert_line 'check: sandbox ... FAIL' "make sandbox-tier, broken bwrap"
refute 'check: sandbox ... SKIPPED' "make sandbox-tier, broken bwrap"

run_make "$PATH"
assert_code 0 "this machine"
if grep -q '^sandbox tier: ' <<<"$OUT"; then
  ok "this machine: prints a verdict line"
else
  bad "this machine: prints no verdict line"; printf '  output:\n%s\n' "$OUT"
fi
refute 'check: sandbox ... FAIL' "this machine"

OUT=$(cd "$repo_root" && "$make_path" --no-print-directory -n sandbox 2>&1)
if grep -q 'sandbox-probe.sh' <<<"$OUT"; then
  ok "make sandbox: runs the probe by default"
else
  bad "make sandbox: does not reach the probe"; printf '  output:\n%s\n' "$OUT"
fi
OUT=$("$make_path" --no-print-directory -C "$repo_root" -n -p 2>/dev/null | sed -n 's/^STAGES := //p')
if [[ $OUT == *sandbox* ]]; then
  ok "make check: STAGES lists sandbox"
else
  bad "make check: STAGES is '$OUT', want sandbox among them"
fi
OUT=$(cd "$repo_root" && "$make_path" --no-print-directory -n check 2>&1)
if grep -q 'sandbox' <<<"$OUT"; then
  ok "make check: the loop reaches the sandbox stage"
else
  bad "make check: the loop never reaches the sandbox stage"; printf '  output:\n%s\n' "$OUT"
fi

printf '\n%s passed, %s failed\n' "$passed" "$failed"
(( failed == 0 ))
