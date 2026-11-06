#!/usr/bin/env bash
# Contract test for the release stage of `make check` (spec §16, ADR 0001; ticket
# #62): scripts/release-probe.sh, and the stage line the Makefile relays it as.
#
# The release path must be exercised by the gate rather than by hand on release
# day, which means two things this test proves and nothing else can:
#
#   1. A broken release script turns the stage line into `check: release ... FAIL`
#      and stops the run with a non-zero exit. That is the bullet in the ticket,
#      and it is proven by handing the stage a `cargo` that refuses to build.
#   2. Every assertion the probe makes is one that can fail. An artefact that is
#      not static, one that is glibc rather than musl, and one carrying an OpenSSL
#      symbol are each caught by name — and a machine with no bwrap, or a build
#      that cannot link because devbox supplies no musl C compiler, skips loudly
#      at 77 instead of printing the green line a stage that ran has earned.
#
# The artefacts are made here rather than shipped: rustc builds a static musl
# binary in the layout a real CARGO_TARGET_DIR has, copying `/bin/true` supplies
# the dynamic one, `cc -static` the glibc one, and appending one symbol to a good
# artefact makes the OpenSSL case. The machine is faked the way
# scripts/sandbox-probe-test.sh fakes it — a stand-in `bwrap` first on PATH, and a
# stand-in `cargo` that answers the build without building — so every verdict is
# provable on a machine whose real tools are fine.
#
#     bash scripts/release-probe-test.sh
set -uo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
probe="$repo_root/scripts/release-probe.sh"
bash_path=$(command -v bash)
make_path=$(command -v make)
scratch=$(mktemp -d "${TMPDIR:-/dev/shm}/sindri-release-probe-test.XXXXXX") || exit 1
trap 'rm -rf "$scratch"' EXIT

passed=0
failed=0
ok() { printf 'ok   %s\n' "$*"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n' "$*"; failed=$((failed + 1)); }

# The target directory every case hands the stage, laid out the way cargo lays a
# musl release build out: <CARGO_TARGET_DIR>/x86_64-unknown-linux-musl/release.
target="$scratch/target"
artefact="$target/x86_64-unknown-linux-musl/release/sindri"
mkdir -p "$(dirname "$artefact")"

# A static musl binary for the probe to pass, built from four lines with no
# dependency at all, so this test pays a compile rather than a release build.
cat >"$scratch/good.rs" <<'RS'
fn main() {
    println!("sindri 0.0.0 (release-probe-test)");
}
RS
if rustc --target x86_64-unknown-linux-musl -C panic=abort -O -o "$scratch/good" "$scratch/good.rs"; then
  ok 'a static musl artefact to hand the probe'
else
  bad 'rustc could not build the static musl artefact this test needs'
fi

# The artefact the probe must refuse: dynamic, so it carries the interpreter a
# static one does not.
if cp /bin/true "$scratch/dynamic"; then ok 'a dynamic glibc artefact to refuse'; else
  bad 'no /bin/true to copy a dynamic artefact from'; fi

# An artefact that is static and carries glibc's own version string, which is the
# case only the glibc marker catches: staticness alone would pass it, so the probe
# may not use staticness as its evidence of musl. Appending the string is enough,
# because a statically linked libc leaves exactly that behind.
cp "$scratch/good" "$scratch/glibc"
printf 'GNU C Library (GNU libc) stable release version 2.44.\n' >>"$scratch/glibc"

# And two artefacts that carry no musl to find: one with no symbol table at all,
# one with musl's own `__stdio_exit` taken out of it.
cp "$scratch/good" "$scratch/no-symbols"
if strip --strip-all "$scratch/no-symbols" 2>/dev/null; then
  ok 'a stripped artefact with no symbol table to read'
else
  bad 'strip could not make the artefact this test needs'
fi
cp "$scratch/good" "$scratch/no-stdio-exit"
if objcopy --strip-symbol=__stdio_exit "$scratch/no-stdio-exit" 2>/dev/null; then
  ok "a static artefact that does not define musl's __stdio_exit"
else
  bad 'objcopy could not make the artefact this test needs'
fi

# Two artefacts carrying OpenSSL, in each of the two shapes it could arrive: a
# symbol in the table (what a statically linked libssl leaves behind) and a
# library name in the strings (what a dynamically linked one leaves behind).
cp "$scratch/good" "$scratch/openssl-symbol"
if objcopy --add-symbol SSL_CTX_new=.text:0 "$scratch/openssl-symbol" 2>/dev/null; then
  ok 'an artefact carrying an OpenSSL symbol'
else
  bad 'objcopy could not make the OpenSSL artefact this test needs'
fi
cp "$scratch/good" "$scratch/openssl-string"
printf 'libssl.so.3' >>"$scratch/openssl-string"

# The machines, by PATH. A bwrap that reports a version and a kernel that will not
# let it unshare is the machine the skip exists for; a directory holding only the
# tools the probe needs is the machine with no bwrap at all.
tools="$scratch/machine-no-bwrap"
mkdir -p "$tools"
for tool in bash grep nm strings od head sort tr sed basename dirname cat; do
  path=$(command -v "$tool") && ln -s "$path" "$tools/$tool"
done
restricted="$scratch/machine-restricted"
mkdir -p "$restricted"
cat >"$restricted/bwrap" <<'BW'
#!/bin/sh
if [ "${1:-}" = --version ]; then
  echo 'bubblewrap 0.12.0 (fake)'
  exit 0
fi
echo 'bwrap: setting up uid map: Permission denied' >&2
exit 1
BW
chmod +x "$restricted/bwrap"

# The stand-in cargos, first on PATH so the stage's own `cargo build` is answered
# without a build: one succeeds, one fails the way a broken tree fails, and one
# fails the way devbox's missing musl C compiler fails.
for kind in ok broken no-musl-cc; do mkdir -p "$scratch/cargo-$kind"; done
printf '#!/bin/sh\nexit 0\n' >"$scratch/cargo-ok/cargo"
cat >"$scratch/cargo-broken/cargo" <<'CARGO'
#!/bin/sh
echo 'error[E0425]: cannot find value `answer` in this scope' >&2
exit 101
CARGO
cat >"$scratch/cargo-no-musl-cc/cargo" <<'CARGO'
#!/bin/sh
echo "bcm.c:(.text.aws_lc_0_47_0_handle_cpu_env+0x54): undefined reference to \`__isoc23_sscanf'" >&2
exit 101
CARGO
chmod +x "$scratch"/cargo-{ok,broken,no-musl-cc}/cargo

# use_artefact FILE -- put FILE where the probe looks, so the probe needs no
# argument and no case can forget to say which artefact it means.
use_artefact() {
  if ! cp "$1" "$artefact"; then
    bad "no artefact at $1 to hand the probe"
    return 1
  fi
}

# run_probe PATH -- captures OUT and CODE from the probe alone.
run_probe() {
  OUT=$(env PATH="$1" CARGO_TARGET_DIR="$target" "$bash_path" "$probe" 2>&1)
  CODE=$?
}

# run_stage PATH -- captures OUT and CODE from `make release-tier`, the stage line
# `make check` prints its verdict through. The tier rather than `make release`,
# because that one runs this test, and a test that ran itself would never end.
run_stage() {
  OUT=$(env PATH="$1" CARGO_TARGET_DIR="$target" "$make_path" --no-print-directory \
    -C "$repo_root" release-tier 2>&1)
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

# 1. A good artefact on this machine. The last check is the one that needs a
#    kernel that will unshare, so a machine that refuses skips it — loudly, at 77,
#    which is the probe's own convention and never a green line.
use_artefact "$scratch/good"
run_probe "$scratch/cargo-ok:$PATH"
if [[ $CODE == 0 ]]; then
  assert_line 'release: ok (5 checks)' 'good artefact'
  assert_has 'artefact is a static ELF' 'good artefact'
  assert_has 'artefact is musl, not glibc' 'good artefact'
  assert_has 'no OpenSSL symbol or string' 'good artefact'
  assert_has 'runs with no devbox, no Nix and no fonts' 'good artefact'
elif [[ $CODE == 77 ]]; then
  assert_line 'release: skipped (bwrap unavailable)' 'good artefact, machine that will not unshare'
  assert_line 'release: FAIL' 'good artefact, machine that will not unshare'
  assert_has 'artefact is a static ELF' 'good artefact, machine that will not unshare'
else
  bad "good artefact: exit $CODE, want 0 or 77"; printf '  output:\n%s\n' "$OUT"
fi

# 2. The artefact that is not static: a glibc binary with an interpreter in it.
use_artefact "$scratch/dynamic"
run_probe "$scratch/cargo-ok:$PATH"
assert_code 1 'dynamic artefact'
assert_has 'FAIL release: artefact is not static' 'dynamic artefact'
refute 'release: ok' 'dynamic artefact, no green tick'

# 3. A static artefact that says glibc: staticness alone would pass it, so the
#    marker is what catches it.
use_artefact "$scratch/glibc"
run_probe "$scratch/cargo-ok:$PATH"
assert_code 1 'glibc artefact'
assert_has 'FAIL release: artefact is not musl' 'glibc artefact'
assert_has 'GNU C Library' 'glibc artefact: names what it found'
refute 'release: ok' 'glibc artefact, no green tick'

# 3b. The other two ways an artefact can carry no musl: no symbol table to read,
#     and no `__stdio_exit` in the one it has.
use_artefact "$scratch/no-symbols"
run_probe "$scratch/cargo-ok:$PATH"
assert_code 1 'artefact with no symbol table'
assert_has 'FAIL release: artefact carries no symbol table' 'artefact with no symbol table'

use_artefact "$scratch/no-stdio-exit"
run_probe "$scratch/cargo-ok:$PATH"
assert_code 1 'artefact without __stdio_exit'
assert_has "FAIL release: artefact is not musl: it does not define musl's __stdio_exit" \
  'artefact without __stdio_exit'

# 4. An artefact carrying OpenSSL, in a symbol and in a library name.
use_artefact "$scratch/openssl-symbol"
run_probe "$scratch/cargo-ok:$PATH"
assert_code 1 'openssl symbol'
assert_has 'FAIL release: the artefact carries OpenSSL' 'openssl symbol'
assert_has 'SSL_CTX_new' 'openssl symbol: names the symbol it found'
refute 'release: ok' 'openssl symbol, no green tick'

use_artefact "$scratch/openssl-string"
run_probe "$scratch/cargo-ok:$PATH"
assert_code 1 'openssl library name'
assert_has 'FAIL release: the artefact carries OpenSSL' 'openssl library name'
assert_has 'libssl.so.3' 'openssl library name: names the library it found'
refute 'release: ok' 'openssl library name, no green tick'

# 5. A build that says it succeeded and left nothing to assert is a failure, not
#    a pass.
rm -f "$artefact"
run_probe "$scratch/cargo-ok:$PATH"
assert_code 1 'a build that left no artefact'
assert_has 'FAIL release: no artefact at' 'a build that left no artefact'

# 6. A build that fails for any reason but the named one fails the stage.
use_artefact "$scratch/good"
run_probe "$scratch/cargo-broken:$PATH"
assert_code 1 'a broken build'
assert_has 'FAIL release: cargo build --release --target x86_64-unknown-linux-musl' 'a broken build'
refute 'release: skipped' 'a broken build is a failure, not a skip'

# 7. The one gap this stage knows by name: no musl C compiler, so the artefact
#    cannot link. The reason is printed and the verdict skips — it is neither a
#    pass nor the tree's failure.
run_probe "$scratch/cargo-no-musl-cc:$PATH"
assert_code 77 'no musl C compiler'
assert_line 'release: skipped (no musl C compiler)' 'no musl C compiler'
assert_has '__isoc23_sscanf' 'no musl C compiler: names the symbol that could not be found'
refute 'release: ok' 'no musl C compiler, no green tick over an artefact nobody built'

# 8. A machine with no bwrap: the probe says which capability is missing and
#    exits 77, with the reason above the verdict.
run_probe "$scratch/cargo-ok:$tools"
assert_code 77 'no bwrap'
assert_line 'release: skipped (bwrap unavailable)' 'no bwrap'
assert_has 'bwrap not on PATH' 'no bwrap'
refute 'release: ok' 'no bwrap, no green tick over uncovered code'

# 9. A kernel that will not let bwrap unshare: the same skip, said the same way.
run_probe "$scratch/cargo-ok:$restricted:$PATH"
assert_code 77 'restricted kernel'
assert_line 'release: skipped (bwrap unavailable)' 'restricted kernel'
assert_has 'bwrap cannot create an unprivileged sandbox' 'restricted kernel'

# 10. The stage line, over a machine whose cargo answers without a build: each
#     verdict relayed, and the broken script stopping the run where it stands.
run_stage "$scratch/cargo-ok:$PATH"
assert_code 0 'make release-tier, good artefact'
assert_line 'check: release ... ok' 'make release-tier, good artefact'

run_stage "$scratch/cargo-ok:$tools"
assert_code 0 'make release-tier, no bwrap'
assert_line 'check: release ... SKIPPED' 'make release-tier, no bwrap'
refute 'check: release ... ok' 'make release-tier, no bwrap'

run_stage "$scratch/cargo-no-musl-cc:$PATH"
assert_code 0 'make release-tier, no musl C compiler'
assert_line 'check: release ... SKIPPED' 'make release-tier, no musl C compiler'
refute 'check: release ... ok' 'make release-tier, no musl C compiler'

run_stage "$scratch/cargo-broken:$PATH"
assert_code 2 'make release-tier, broken build'
assert_line 'check: release ... FAIL' 'make release-tier, broken build'
refute 'check: release ... SKIPPED' 'make release-tier, broken build'

# 11. The default really is to run the release path: `make check` lists the stage,
#     the loop reaches it, and `make release` runs the contract test the same way.
OUT=$("$make_path" --no-print-directory -C "$repo_root" -n -p 2>/dev/null | sed -n 's/^STAGES := //p')
if [[ $OUT == *release* ]]; then
  ok 'make check: STAGES lists release'
else
  bad "make check: STAGES is '$OUT', want release among them"
fi
OUT=$(cd "$repo_root" && "$make_path" --no-print-directory -n check 2>&1)
if grep -q 'release-probe.sh' <<<"$OUT"; then
  ok 'make check: the loop reaches the release stage'
else
  bad 'make check: the loop never reaches the release stage'; printf '  output:\n%s\n' "$OUT"
fi
OUT=$(cd "$repo_root" && "$make_path" --no-print-directory -n release 2>&1)
if grep -q 'release-probe-test.sh' <<<"$OUT" && grep -q 'release-probe.sh' <<<"$OUT"; then
  ok 'make release: runs the contract test and then the probe'
else
  bad 'make release: does not reach both scripts'; printf '  output:\n%s\n' "$OUT"
fi

printf '\n%s passed, %s failed\n' "$passed" "$failed"
(( failed == 0 ))
