#!/usr/bin/env bash
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

target="$scratch/target"
artefact="$target/x86_64-unknown-linux-musl/release/sindri"
mkdir -p "$(dirname "$artefact")"

need_artefact() {
  local desc=$1 out=$2 src=$3
  shift 3
  if rustc --target x86_64-unknown-linux-musl -C panic=abort -O "$@" -o "$out" "$src"; then
    ok "an artefact to hand the probe: $desc"
  else
    bad "rustc could not make the artefact this test needs: $desc"
  fi
}

cat >"$scratch/good.rs" <<'RS'
fn main() {
    println!("sindri 0.0.0 (release-probe-test)");
}
RS
need_artefact 'the good one' "$scratch/good" "$scratch/good.rs"

if cp /bin/true "$scratch/dynamic"; then ok 'a dynamic glibc artefact to refuse'; else
  bad 'no /bin/true to copy a dynamic artefact from'; fi

cat >"$scratch/glibc.rs" <<'RS'
fn main() {
    println!("GNU C Library (GNU libc) stable release version 2.44.");
}
RS
need_artefact 'one that says glibc' "$scratch/glibc" "$scratch/glibc.rs"

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

cp "$scratch/good" "$scratch/openssl-symbol"
if objcopy --add-symbol SSL_CTX_new=.text:0 "$scratch/openssl-symbol" 2>/dev/null; then
  ok 'an artefact carrying an OpenSSL symbol'
else
  bad 'objcopy could not make the OpenSSL artefact this test needs'
fi
cat >"$scratch/openssl-string.rs" <<'RS'
fn main() {
    println!("linking libssl.so.3");
}
RS
need_artefact 'one carrying an OpenSSL library name' \
  "$scratch/openssl-string" "$scratch/openssl-string.rs"

tools="$scratch/machine-no-bwrap"
mkdir -p "$tools"
for tool in bash grep nm strings od head sort tr sed strip basename dirname cat cargo-zigbuild; do
  path=$(command -v "$tool") && ln -s "$path" "$tools/$tool"
done
nozig="$scratch/machine-no-zigbuild"
mkdir -p "$nozig"
for tool in bash grep nm strings od head sort tr sed strip bwrap basename dirname cat; do
  path=$(command -v "$tool") && ln -s "$path" "$nozig/$tool"
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

rustc_path=$(command -v rustc)
for kind in ok broken glibc-c; do mkdir -p "$scratch/cargo-$kind"; done
printf '#!/bin/sh\nexit 0\n' >"$scratch/cargo-ok/cargo"
cat >"$scratch/cargo-broken/cargo" <<'CARGO'
#!/bin/sh
echo 'error[E0425]: cannot find value `answer` in this scope' >&2
exit 101
CARGO
cat >"$scratch/cargo-glibc-c/cargo" <<CARGO
#!/bin/sh
mkdir -p "$scratch/link-defect"
cat >"$scratch/link-defect/defect.rs" <<'RS'
extern "C" {
    fn __isoc23_sscanf(text: *const u8, format: *const u8) -> i32;
}
fn main() {
    let n = unsafe { __isoc23_sscanf(b"1\0".as_ptr(), b"%d\0".as_ptr()) };
    println!("{n}");
}
RS
if said=\$("$rustc_path" --target x86_64-unknown-linux-musl -o "$scratch/link-defect/sindri" \
    "$scratch/link-defect/defect.rs" 2>&1); then
  echo 'release-probe-test: the link defect linked, so this case proves nothing' >&2
else
  printf '%s\n' "\$said" >&2
fi
exit 101
CARGO
chmod +x "$scratch"/cargo-{ok,broken,glibc-c}/cargo

mkdir -p "$scratch/cargo-zig"
cat >"$scratch/cargo-zig/cargo" <<CARGO
#!/bin/sh
printf '%s\n' "\$*" >"$scratch/zigbuild-argv"
printf '%s\n' "\${RUSTFLAGS:-}" >"$scratch/zigbuild-rustflags"
case " \$* " in
  *' zigbuild '*) exit 0 ;;
esac
echo "cargo \$1: the release path builds through cargo-zigbuild" >&2
exit 101
CARGO
chmod +x "$scratch/cargo-zig/cargo"

use_artefact() {
  if ! cp "$1" "$artefact"; then
    bad "no artefact at $1 to hand the probe"
    return 1
  fi
}

run_probe() {
  OUT=$(env PATH="$1" CARGO_TARGET_DIR="$target" "$bash_path" "$probe" 2>&1)
  CODE=$?
}

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
assert_not_failed() { # desc -- the verdicts a stage that failed never prints
  if [[ $CODE == 0 || $CODE == 77 ]]; then ok "$1: exit $CODE"; else
    bad "$1: exit $CODE, want 0 or 77"; printf '  output:\n%s\n' "$OUT"; fi
}
assert_arg() { # want -- the argv the stage recorded names it
  if [[ " ${argv:-} " == *" $1 "* ]]; then ok "the build: asks for $1"; else
    bad "the build: never asks for $1 (argv: ${argv:-none})"; fi
}
refute() { # substring desc -- no line may carry it
  if grep -qF -- "$1" <<<"$OUT"; then
    bad "$2: output carries '$1'"; printf '  output:\n%s\n' "$OUT"; else
    ok "$2: never says '$1'"; fi
}

use_artefact "$scratch/good"
run_probe "$scratch/cargo-ok:$PATH"
can_confine=
if [[ $CODE == 0 ]]; then
  can_confine=yes
  assert_line 'release: ok (6 checks)' 'good artefact'
  assert_has 'artefact is a static ELF' 'good artefact'
  assert_has 'artefact is musl, not glibc' 'good artefact'
  assert_has 'no OpenSSL symbol or string' 'good artefact'
  assert_has 'runs with no devbox, no Nix and no fonts' 'good artefact'
elif [[ $CODE == 77 ]]; then
  can_confine=no
  assert_line 'release: skipped (bwrap unavailable)' 'good artefact, machine that will not unshare'
  assert_has 'artefact is a static ELF' 'good artefact, machine that will not unshare'
  assert_has 'bwrap cannot create an unprivileged sandbox' \
    'good artefact, machine that will not unshare: names what it could not do'
  refute 'release: ok' 'good artefact, machine that will not unshare, no green tick'
else
  bad "good artefact: exit $CODE, want 0 or 77"; printf '  output:\n%s\n' "$OUT"
fi

use_artefact "$scratch/dynamic"
run_probe "$scratch/cargo-ok:$PATH"
assert_code 1 'dynamic artefact'
assert_has 'FAIL release: artefact is not static' 'dynamic artefact'
refute 'release: ok' 'dynamic artefact, no green tick'

use_artefact "$scratch/glibc"
run_probe "$scratch/cargo-ok:$PATH"
assert_code 1 'glibc artefact'
assert_has 'FAIL release: artefact is not musl' 'glibc artefact'
assert_has 'GNU C Library' 'glibc artefact: names what it found'
refute 'release: ok' 'glibc artefact, no green tick'

use_artefact "$scratch/no-symbols"
run_probe "$scratch/cargo-ok:$PATH"
assert_code 1 'artefact with no symbol table'
assert_has 'FAIL release: artefact carries no symbol table' 'artefact with no symbol table'

use_artefact "$scratch/no-stdio-exit"
run_probe "$scratch/cargo-ok:$PATH"
assert_code 1 'artefact without __stdio_exit'
assert_has "FAIL release: artefact is not musl: it does not define musl's __stdio_exit" \
  'artefact without __stdio_exit'

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

rm -f "$artefact"
run_probe "$scratch/cargo-ok:$PATH"
assert_code 1 'a build that left no artefact'
assert_has 'FAIL release: no artefact at' 'a build that left no artefact'

use_artefact "$scratch/good"
run_probe "$scratch/cargo-broken:$PATH"
assert_code 1 'a broken build'
assert_has 'FAIL release: cargo zigbuild --release --target x86_64-unknown-linux-musl' 'a broken build'
refute 'release: skipped' 'a broken build is a failure, not a skip'

use_artefact "$scratch/good"
run_probe "$scratch/cargo-zig:$PATH"
if [[ $can_confine == no ]]; then
  assert_line 'release: skipped (bwrap unavailable)' \
    'artefact built through zigbuild, machine that will not unshare'
else
  assert_code 0 'artefact built through zigbuild'
  assert_line 'release: ok (6 checks)' 'artefact built through zigbuild'
fi
refute 'FAIL release:' 'artefact built through zigbuild'
refute 'cargo-zigbuild is not on PATH' 'artefact built through zigbuild, never a missing tool'
argv=$(cat "$scratch/zigbuild-argv" 2>/dev/null)
for arg in zigbuild --release --target x86_64-unknown-linux-musl --package sindri; do
  assert_arg "$arg"
done
rustflags=$(cat "$scratch/zigbuild-rustflags" 2>/dev/null)
if [[ $rustflags == *strip=none* ]]; then
  ok 'the build: keeps the symbol table musl is read from'
else
  bad "the build: rustflags are '${rustflags:-none}', want -C strip=none among them"
fi

debug_src="$scratch/nix/store/00000000000000000000000000000000-zig-0.17.0/src"
mkdir -p "$debug_src"
cp "$scratch/good.rs" "$debug_src/main.rs"
need_artefact 'one whose debug info names a store path' \
  "$scratch/with-debug" "$debug_src/main.rs" -C debuginfo=2
use_artefact "$scratch/with-debug"
run_probe "$scratch/cargo-ok:$PATH"
assert_not_failed 'artefact carrying debug info'
if grep -qa '/nix/store' "$artefact"; then
  bad 'the artefact still names a store path: the debug info was left in it'
else
  ok 'the artefact names no store path: the debug info was dropped'
fi
if nm -- "$artefact" 2>/dev/null | grep -qw '__stdio_exit'; then
  ok 'the artefact keeps the symbol table the musl assertion reads'
else
  bad 'the artefact lost its symbol table with the debug info'
fi

cat >"$scratch/store-path.rs" <<'RS'
fn main() {
    println!("/nix/store/00000000000000000000000000000000-glibc-2.44/lib/libc.so.6");
}
RS
need_artefact 'one carrying a store path in a section strip keeps' \
  "$scratch/store-path" "$scratch/store-path.rs"
use_artefact "$scratch/store-path"
run_probe "$scratch/cargo-ok:$PATH"
assert_code 1 'artefact carrying a store path'
assert_has 'FAIL release: the artefact names a store path' 'artefact carrying a store path'
refute 'release: ok' 'artefact carrying a store path, no green tick'
use_artefact "$scratch/good"

run_probe "$scratch/cargo-glibc-c:$PATH"
assert_code 1 'a musl link that fails'
assert_line 'release: FAIL (0 checks)' 'a musl link that fails'
assert_has 'undefined reference to' 'a musl link that fails: relays what the linker said'
assert_has '__isoc23_sscanf' 'a musl link that fails: names the symbol it could not find'
refute 'release: skipped' 'a musl link that fails, never the skip it used to be'
refute 'release: ok' 'a musl link that fails, no green tick over an artefact nobody built'

run_probe "$scratch/cargo-ok:$nozig"
assert_code 1 'no cargo-zigbuild'
assert_line 'release: FAIL (0 checks)' 'no cargo-zigbuild'
assert_has 'cargo-zigbuild is not on PATH' 'no cargo-zigbuild'
refute 'release: skipped' 'no cargo-zigbuild, never a skip over an artefact nobody built'
refute 'release: ok' 'no cargo-zigbuild, no green tick over an artefact nobody built'

run_probe "$scratch/cargo-ok:$tools"
assert_code 77 'no bwrap'
assert_line 'release: skipped (bwrap unavailable)' 'no bwrap'
assert_has 'bwrap not on PATH' 'no bwrap'
refute 'release: ok' 'no bwrap, no green tick over uncovered code'

run_probe "$scratch/cargo-ok:$restricted:$PATH"
assert_code 77 'restricted kernel'
assert_line 'release: skipped (bwrap unavailable)' 'restricted kernel'
assert_has 'bwrap cannot create an unprivileged sandbox' 'restricted kernel'

run_stage "$scratch/cargo-ok:$PATH"
assert_code 0 'make release-tier, good artefact'
if [[ $can_confine == no ]]; then
  assert_line 'check: release ... SKIPPED' \
    'make release-tier, good artefact, machine that will not unshare'
  refute 'check: release ... ok' 'make release-tier, good artefact, machine that will not unshare'
else
  assert_line 'check: release ... ok' 'make release-tier, good artefact'
fi

run_stage "$scratch/cargo-ok:$tools"
assert_code 0 'make release-tier, no bwrap'
assert_line 'check: release ... SKIPPED' 'make release-tier, no bwrap'
refute 'check: release ... ok' 'make release-tier, no bwrap'

run_stage "$scratch/cargo-glibc-c:$PATH"
assert_code 2 'make release-tier, a musl link that fails'
assert_line 'check: release ... FAIL' 'make release-tier, a musl link that fails'
refute 'check: release ... SKIPPED' 'make release-tier, a musl link that fails'
refute 'check: release ... ok' 'make release-tier, a musl link that fails'

run_stage "$scratch/cargo-ok:$nozig"
assert_code 2 'make release-tier, no cargo-zigbuild'
assert_line 'check: release ... FAIL' 'make release-tier, no cargo-zigbuild'
refute 'check: release ... SKIPPED' 'make release-tier, no cargo-zigbuild'
refute 'check: release ... ok' 'make release-tier, no cargo-zigbuild'

run_stage "$scratch/cargo-broken:$PATH"
assert_code 2 'make release-tier, broken build'
assert_line 'check: release ... FAIL' 'make release-tier, broken build'
refute 'check: release ... SKIPPED' 'make release-tier, broken build'

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
