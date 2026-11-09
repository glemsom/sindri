#!/usr/bin/env bash
set -uo pipefail

SKIPPED_STATUS=77

checks=0
status=0
ok()   { printf 'ok   sandbox: %s\n' "$*"; checks=$((checks + 1)); }
skip() { printf 'skip sandbox: %s\n' "$*"; }
bad()  { printf 'FAIL sandbox: %s\n' "$*"; status=1; }

unavailable() {
  skip "$1"
  printf 'sandbox tier: skipped (bwrap unavailable)\n'
  exit "$SKIPPED_STATUS"
}

if ! command -v bwrap >/dev/null 2>&1; then
  unavailable 'bwrap not on PATH'
fi

if ! got=$(bwrap --version 2>&1); then
  bad "bwrap --version: ${got:-no output}"
  printf 'sandbox tier: FAIL (bwrap on PATH but unusable)\n'
  exit 1
fi
ok "bwrap --version: $got"

sandbox=(--unshare-all --ro-bind / / --dev /dev --proc /proc)

if ! out=$(bwrap "${sandbox[@]}" -- true 2>&1); then
  unavailable "bwrap cannot create an unprivileged sandbox: ${out:-no output}"
fi
ok 'bwrap creates an unprivileged sandbox'

here_namespace=$(readlink /proc/self/ns/pid 2>&1)
if out=$(bwrap "${sandbox[@]}" -- sh -c 'readlink /proc/self/ns/pid' 2>&1) &&
   [[ -n $out && $out != "$here_namespace" ]]; then
  ok "bwrap unshares a pid namespace: $here_namespace -> $out"
else
  bad "bwrap did not unshare a pid namespace: '${out:-no output}', this shell's is '$here_namespace'"
fi

if ((status)); then
  printf 'sandbox tier: FAIL (bwrap present, but the sandbox it made is not the one promised)\n'
  exit 1
fi
printf 'sandbox tier: ok (%s checks)\n' "$checks"
exit 0
