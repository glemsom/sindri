#!/usr/bin/env bash
# The sandbox tier of `make check` (spec §16, ADR 0001; wired by ticket #7):
# proves this machine can confine a command the way the product will, which means
# bwrap creating an unprivileged user namespace and a sandbox inside it.
#
# It is a script of its own, apart from scripts/toolchain-probe.sh, for one
# reason: whether a kernel lets an unprivileged process create a user namespace
# is a fact about the machine, not about the tree, and a hosted CI runner may say
# no (Ubuntu 23.10 and later restrict it through AppArmor). A tier that cannot
# run has to say so in a line of its own — and must never print the green line a
# tier that really ran has earned.
#
# Three verdicts, and the exit code is part of the contract:
#
#   0   sandbox tier: ok (N checks)                the tier ran and passed
#   77  sandbox tier: skipped (bwrap unavailable)  the tier could not run here;
#                                                  the reason is the line above
#   1   sandbox tier: FAIL (...)                   the tier ran and failed
#
# `make sandbox-tier` relays all three as `check: sandbox ... ok`, `... SKIPPED`
# and `... FAIL`, and keeps exit 0 for the skip alone: the gap is printed where a
# reader cannot miss it, never counted as a pass.
#
# "Unavailable" covers both shapes of a machine that cannot do this: no bwrap on
# PATH, and a bwrap the kernel will not let unshare. A bwrap on PATH that cannot
# even report a version is a FAIL instead, because that is a broken install
# someone can fix rather than an absent capability.
#
#     devbox run -- bash scripts/sandbox-probe.sh
#
# To see the skip on a machine whose bwrap works, hand it a PATH with no bwrap on
# it. scripts/sandbox-probe-test.sh does exactly that, for all three verdicts.
set -uo pipefail

# automake's "skipped" status, chosen so a skip cannot be mistaken for the 0 a
# tier that ran earns or the 1 a failure earns. `make sandbox-tier` reads it.
SKIPPED_STATUS=77

checks=0
status=0
ok()   { printf 'ok   sandbox: %s\n' "$*"; checks=$((checks + 1)); }
skip() { printf 'skip sandbox: %s\n' "$*"; }
bad()  { printf 'FAIL sandbox: %s\n' "$*"; status=1; }

# unavailable REASON -- this machine cannot run the tier. The verdict line is
# printed here and nowhere else, so there is one wording to grep for.
unavailable() {
  skip "$1"
  printf 'sandbox tier: skipped (bwrap unavailable)\n'
  exit "$SKIPPED_STATUS"
}

if ! command -v bwrap >/dev/null 2>&1; then
  unavailable 'bwrap not on PATH'
fi

# 1. bwrap runs at all. A binary that cannot print its own version is broken, so
#    it fails rather than excusing the machine.
if ! got=$(bwrap --version 2>&1); then
  bad "bwrap --version: ${got:-no output}"
  printf 'sandbox tier: FAIL (bwrap on PATH but unusable)\n'
  exit 1
fi
ok "bwrap --version: $got"

# The invocation every check below repeats: every namespace unshared, the root
# filesystem read-only, and the two mounts that make it look like a machine.
sandbox=(--unshare-all --ro-bind / / --dev /dev --proc /proc)

# 2. A real unprivileged sandbox is created. This is the check a hosted runner
#    may forbid, and the only reason this tier can skip.
if ! out=$(bwrap "${sandbox[@]}" -- true 2>&1); then
  unavailable "bwrap cannot create an unprivileged sandbox: ${out:-no output}"
fi
ok 'bwrap creates an unprivileged sandbox'

# 3. And it is a sandbox: the command starts in a pid namespace of its own, not
#    in this shell's wearing a sandbox's clothes. bwrap keeps pid 1 for reaping,
#    so the sandboxed process's own pid is not the fact to check — the namespace
#    it sees is.
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
