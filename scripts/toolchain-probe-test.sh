#!/usr/bin/env bash
# Contract test for the toolchain tier of `make check`: the assertions
# scripts/toolchain-probe.sh makes, and the stage line the Makefile relays.
#
# The tier proves the devbox shell supplies the build toolchain the release needs.
# This test drives its three verdicts and the facts ticket #101 added to their
# failures; the passing case is the tree and the shell this test runs in, so the
# facts that were already there are held by that run rather than by a fake:
#
#   * a machine with no cargo on PATH is not a devbox shell: that is a printed
#     SKIPPED at 77, relayed as `check: toolchain ... SKIPPED` with the run's exit
#     still 0 — the one verdict that is never the green line a passing tier earned;
#   * a `devbox.json` that lets a package float fails the tier, naming the package,
#     and one with no `rustup default` fails naming the pin it wanted;
#   * a `Cargo.toml` whose `rust-version` disagrees with that pin fails naming both
#     versions, because a gate that measures a compiler nobody pinned says nothing;
#   * a shell that names no musl C compiler, one that names a compiler for the
#     host's glibc, and the host's own `cc` are each caught by name: a build
#     reaching a glibc compiler is the defect that made the musl artefact, and the
#     `__isoc99_sscanf` link error, a coincidence of what the shell happened to
#     put on PATH;
#   * a `pkg-config` that is not the shell's own, and one that resolves a system
#     OpenSSL, each fail the tier: the shell's pkg-config is what makes "the
#     artefact links the vendored C" a property of the toolchain rather than of
#     the host;
#   * a `devbox.json` that names cmake or nasm fails the tier, because each is a
#     decision the probe writes down and asserts rather than a comment nobody can
#     check;
#   * a good machine passes, and passes on the tree's own facts.
#
# Outside a devbox shell there is no toolchain to measure and no machine this test
# could drive, so it skips at 77 with the tier it tests, and `make toolchain` prints
# SKIPPED for both rather than a green line over nothing.
#
# The machines are faked by PATH, the way scripts/sandbox-probe-test.sh fakes one,
# and each tree by copying this repo's `Cargo.toml` and `devbox.json` into a
# scratch directory with one fact changed. The C is compiled for real: a compiler
# asked what it targets answers in its own words, which is the whole point of
# asking it.
#
#     bash scripts/toolchain-probe-test.sh
set -uo pipefail

if ! command -v cargo >/dev/null 2>&1; then
  printf 'skip toolchain test: cargo is not on PATH: run this inside `devbox shell`\n'
  printf 'toolchain test: skipped (no devbox shell)\n'
  exit 77
fi

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
probe="$repo_root/scripts/toolchain-probe.sh"
bash_path=$(command -v bash)
make_path=$(command -v make)
scratch=$(mktemp -d "${TMPDIR:-/dev/shm}/sindri-toolchain-probe-test.XXXXXX") || exit 1
trap 'rm -rf "$scratch"' EXIT

passed=0
failed=0
ok() { printf 'ok   %s\n' "$*"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n' "$*"; failed=$((failed + 1)); }

# new_tree NAME -- a tree the probe can be pointed at, holding this repo's
# Cargo.toml and devbox.json and its own copy of the probe, so one case can break
# one fact without touching the repo the test is running against.
new_tree() {
  local dir="$scratch/tree-$1"
  if ! mkdir -p "$dir/scripts" ||
     ! cp "$probe" "$dir/scripts/toolchain-probe.sh" ||
     ! cp "$repo_root/Cargo.toml" "$repo_root/devbox.json" "$dir/"; then
    bad "could not make the tree '$1' this case needs"
    return 1
  fi
  printf '%s' "$dir"
}

# A machine with nothing but a shell: not a devbox shell, and the case the skip is
# for. bash is there because `make` needs it to run the probe.
shell_only="$scratch/machine-no-shell"
mkdir -p "$shell_only"
ln -s "$bash_path" "$shell_only/bash"

# A pkg-config that is not the shell's: it answers for the host, which is the
# machine the artefact is not built for, and it resolves an OpenSSL there.
foreign_pc="$scratch/machine-foreign-pkg-config"
mkdir -p "$foreign_pc"
cat >"$foreign_pc/pkg-config" <<'SH'
#!/bin/sh
case " $* " in
  *' --exists '*) exit 0 ;;
  *' --variable=prefix '*) echo /usr ;;
esac
exit 1
SH
chmod +x "$foreign_pc/pkg-config"

# The compiler pair the devbox shell this test runs in is supposed to name. Every
# case states its own rather than inheriting the shell's, so the good case is the
# one that proves the hook exports them at all.
pinned_cc=${CC_x86_64_unknown_linux_musl:-}
pinned_ar=${AR_x86_64_unknown_linux_musl:-}
if [[ -n $pinned_cc && -n $pinned_ar ]]; then
  ok "the shell the test runs in names the musl compiler: $pinned_cc"
else
  bad 'the shell the test runs in names no musl compiler: run this inside `devbox shell`'
fi

# run_probe PATH TREE [VAR=VALUE...] -- captures OUT and CODE from the probe,
# with the compiler variables cleared first so a case says which pair it means.
run_probe() {
  local path=$1 tree=$2
  shift 2
  OUT=$(env -u CC_x86_64_unknown_linux_musl -u CXX_x86_64_unknown_linux_musl \
    -u AR_x86_64_unknown_linux_musl PATH="$path" "$@" \
    "$bash_path" "$tree/scripts/toolchain-probe.sh" 2>&1)
  CODE=$?
}

# run_target TARGET PATH [VAR=VALUE...] -- captures OUT and CODE from one make
# target, which is how the stage lines `make check` prints its verdicts through are
# read. The tier rather than `make toolchain` for most cases, because that one runs
# this test, and a test that ran itself would never end.
run_target() {
  local target=$1 path=$2
  shift 2
  OUT=$(env -u CC_x86_64_unknown_linux_musl -u CXX_x86_64_unknown_linux_musl \
    -u AR_x86_64_unknown_linux_musl PATH="$path" "$@" \
    "$make_path" --no-print-directory -C "$repo_root" "$target" 2>&1)
  CODE=$?
}

run_stage() { run_target toolchain-tier "$@"; }

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

# 1. A good machine, and the tree this test was run against: every fact the tier
#    claims, including the pins, the named compiler and the three absences.
run_probe "$PATH" "$repo_root" \
  CC_x86_64_unknown_linux_musl="$pinned_cc" AR_x86_64_unknown_linux_musl="$pinned_ar"
assert_code 0 'a good machine'
assert_line 'toolchain tier: ok (21 checks)' 'a good machine'
assert_has 'every package in devbox.json names a version' 'a good machine'
assert_has "agrees with Cargo.toml's rust-version" 'a good machine'
assert_has 'names the musl target' 'a good machine'
assert_has "the pkg-config on PATH is the shell's own" 'a good machine'
assert_has 'finds no system AWS-LC or OpenSSL' 'a good machine'
assert_has 'devbox.json names neither cmake nor nasm' 'a good machine'
assert_has 'runs standalone' 'a good machine'
refute 'FAIL toolchain:' 'a good machine'

# 2. No cargo on PATH: no devbox shell, so the tier says so and skips at 77 —
#    printed, never counted as the pass a tier that ran earned.
run_probe "$shell_only" "$repo_root"
assert_code 77 'no devbox shell'
assert_line 'toolchain tier: skipped (no devbox shell)' 'no devbox shell'
assert_has 'cargo is not on PATH' 'no devbox shell: says why'
refute 'toolchain tier: ok' 'no devbox shell, no green line'
refute 'FAIL toolchain:' 'no devbox shell is a skip, not a failure'

# 3. A package that floats: the pin is the tree's, so a shell that could change
#    under the tree is a defect in the tree.
float_tree=$(new_tree floating-package)
sed -i 's|^\( *\)"bubblewrap@[0-9.]*"|\1"bubblewrap@latest"|' "$float_tree/devbox.json"
if grep -q 'bubblewrap@latest' "$float_tree/devbox.json"; then
  ok 'a tree whose bubblewrap floats, to refuse'
else
  bad 'could not write the floating package this test needs'
fi
run_probe "$PATH" "$float_tree" \
  CC_x86_64_unknown_linux_musl="$pinned_cc" AR_x86_64_unknown_linux_musl="$pinned_ar"
assert_code 1 'a floating package'
assert_has 'FAIL toolchain: devbox.json lets packages float' 'a floating package'
assert_has 'bubblewrap@latest' 'a floating package: names it'
refute 'toolchain tier: ok' 'a floating package, no green line'

# 3b. A hook that pins no compiler at all: nothing to measure against, so the tier
#     fails rather than measuring the one the machine happened to have.
unpinned_tree=$(new_tree unpinned-rust)
sed -i 's|rustup default [0-9.]*|rustup show active-toolchain|' "$unpinned_tree/devbox.json"
run_probe "$PATH" "$unpinned_tree" \
  CC_x86_64_unknown_linux_musl="$pinned_cc" AR_x86_64_unknown_linux_musl="$pinned_ar"
assert_code 1 'a hook that pins no Rust'
assert_has 'FAIL toolchain: devbox.json' 'a hook that pins no Rust'
assert_has 'rustup default' 'a hook that pins no Rust: names the pin it wanted'

# 4. A Cargo.toml that pins an older Rust than the shell installs: the gate would
#    measure a compiler nobody pinned, so the two are checked against each other.
drift_tree=$(new_tree rust-version-drift)
sed -i 's|^rust-version = "[0-9.]*"|rust-version = "1.98"|' "$drift_tree/Cargo.toml"
if grep -qx 'rust-version = "1.98"' "$drift_tree/Cargo.toml"; then
  ok 'a tree whose rust-version disagrees with the hook, to refuse'
else
  bad 'could not write the rust-version drift this test needs'
fi
run_probe "$PATH" "$drift_tree" \
  CC_x86_64_unknown_linux_musl="$pinned_cc" AR_x86_64_unknown_linux_musl="$pinned_ar"
assert_code 1 'a rust-version that disagrees'
assert_has 'FAIL toolchain:' 'a rust-version that disagrees'
assert_has "rust-version is 1.98" 'a rust-version that disagrees: names both versions'
refute 'toolchain tier: ok' 'a rust-version that disagrees, no green line'

# 5. A shell that names no musl C compiler: the hole this tier closes, because a
#    build with no compiler named for its target reaches whatever is on PATH.
run_probe "$PATH" "$repo_root"
assert_code 1 'no musl C compiler named'
assert_has 'FAIL toolchain: CC_x86_64_unknown_linux_musl is not set' 'no musl C compiler named'

# 6. A compiler named for the target that is a compiler for the host: `zig cc`
#    without a target builds for glibc, and says so when asked.
run_probe "$PATH" "$repo_root" \
  CC_x86_64_unknown_linux_musl='zig cc' AR_x86_64_unknown_linux_musl="$pinned_ar"
assert_code 1 'a compiler for the host libc'
assert_has 'FAIL toolchain: CC_x86_64_unknown_linux_musl' 'a compiler for the host libc'
assert_has "it names 'x86_64-unknown-linux-gnu'" 'a compiler for the host libc: relays what it said'
refute 'toolchain tier: ok' 'a compiler for the host libc, no green line'

# 7. The host's own `cc`: the compiler the artefact defect came in on.
run_probe "$PATH" "$repo_root" \
  CC_x86_64_unknown_linux_musl='cc' AR_x86_64_unknown_linux_musl="$pinned_ar"
assert_code 1 'the host cc named for the musl target'
assert_has "FAIL toolchain: CC_x86_64_unknown_linux_musl is 'cc'" \
  'the host cc named for the musl target'
refute 'toolchain tier: ok' 'the host cc named for the musl target, no green line'

# 8. No archiver: the C compiler alone cannot make the archive a C build needs.
run_probe "$PATH" "$repo_root" CC_x86_64_unknown_linux_musl="$pinned_cc"
assert_code 1 'no archiver named'
assert_has 'FAIL toolchain: AR_x86_64_unknown_linux_musl is not set' 'no archiver named'

# 9. A pkg-config that is not the shell's: it answers for the host, and this host
#    resolves an OpenSSL, so both halves of the decision fail by name. devbox does
#    not isolate /usr/bin, which is why the package is pinned rather than dropped.
run_probe "$foreign_pc:$PATH" "$repo_root" \
  CC_x86_64_unknown_linux_musl="$pinned_cc" AR_x86_64_unknown_linux_musl="$pinned_ar"
assert_code 1 "a pkg-config that is not the shell's own"
assert_has 'FAIL toolchain: the pkg-config on PATH is' "a pkg-config that is not the shell's own"
assert_has 'FAIL toolchain: pkg-config resolves a system C library' "a pkg-config that is not the shell's own"
assert_has 'openssl at /usr' "a pkg-config that is not the shell's own: names what it resolved"
refute 'toolchain tier: ok' "a pkg-config that is not the shell's own, no green line"

# 9b. A tree that adds cmake: the absence is a decision, so the presence is a
#     change nobody decided, and the C half of the artefact would follow the machine.
cmake_tree=$(new_tree cmake-supplied)
sed -i 's|^\( *\)"packages": \[|\1"packages": [\n\1  "cmake@3.31.0",|' "$cmake_tree/devbox.json"
if grep -q 'cmake@3.31.0' "$cmake_tree/devbox.json"; then
  ok 'a tree that names cmake, to refuse'
else
  bad 'could not write the cmake package this test needs'
fi
run_probe "$PATH" "$cmake_tree" \
  CC_x86_64_unknown_linux_musl="$pinned_cc" AR_x86_64_unknown_linux_musl="$pinned_ar"
assert_code 1 'a tree that names cmake'
assert_has 'FAIL toolchain: devbox.json names what the release builds without' 'a tree that names cmake'
assert_has 'cmake@3.31.0' 'a tree that names cmake: names the package'
refute 'toolchain tier: ok' 'a tree that names cmake, no green line'

# 10. The stage line: every verdict relayed, and only the skip keeping exit 0.
run_stage "$PATH" \
  CC_x86_64_unknown_linux_musl="$pinned_cc" AR_x86_64_unknown_linux_musl="$pinned_ar"
assert_code 0 'make toolchain-tier, good machine'
assert_line 'check: toolchain ... ok' 'make toolchain-tier, good machine'

run_stage "$shell_only"
assert_code 0 'make toolchain-tier, no devbox shell'
assert_line 'check: toolchain ... SKIPPED' 'make toolchain-tier, no devbox shell'
refute 'check: toolchain ... ok' 'make toolchain-tier, no devbox shell, no green line'

run_stage "$PATH"
assert_code 2 'make toolchain-tier, no musl C compiler'
assert_line 'check: toolchain ... FAIL' 'make toolchain-tier, no musl C compiler'
refute 'check: toolchain ... SKIPPED' 'make toolchain-tier, no musl C compiler'
refute 'check: toolchain ... ok' 'make toolchain-tier, no musl C compiler, no green line'

# 10b. `make toolchain` outside a devbox shell: the contract test has no machine to
#      drive and the tier has no toolchain to measure, so both print SKIPPED with
#      the reason above them and the run keeps its exit 0.
run_target toolchain "$shell_only"
assert_code 0 'make toolchain, no devbox shell'
assert_line 'check: toolchain-test ... SKIPPED' 'make toolchain, no devbox shell'
assert_line 'check: toolchain ... SKIPPED' 'make toolchain, no devbox shell'
refute 'check: toolchain ... ok' 'make toolchain, no devbox shell, no green line'

# 11. The default really is to run the tier: `make check` lists it, the loop
#     reaches it, and `make toolchain` runs this contract test before the probe.
OUT=$("$make_path" --no-print-directory -C "$repo_root" -n -p 2>/dev/null | sed -n 's/^STAGES := //p')
if [[ $OUT == *toolchain* ]]; then
  ok 'make check: STAGES lists toolchain'
else
  bad "make check: STAGES is '$OUT', want toolchain among them"
fi
OUT=$(cd "$repo_root" && "$make_path" --no-print-directory -n toolchain 2>&1)
if grep -q 'toolchain-probe-test.sh' <<<"$OUT" && grep -q 'toolchain-probe.sh' <<<"$OUT"; then
  ok 'make toolchain: runs the contract test and then the probe'
else
  bad 'make toolchain: does not reach both scripts'; printf '  output:\n%s\n' "$OUT"
fi

printf '\n%s passed, %s failed\n' "$passed" "$failed"
(( failed == 0 ))
