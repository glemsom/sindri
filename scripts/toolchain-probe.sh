#!/usr/bin/env bash
# The toolchain tier of `make check`: proves the devbox shell supplies everything
# the release needs to build, and that nothing it supplies is floating.
#
# Five sets of facts:
#
#   the tree     every package in `devbox.json` names a version or a commit, and
#                the init hook's `rustup default` is the same Rust as `Cargo.toml`'s
#                `rust-version`. A gate that measures a compiler nobody pinned
#                proves nothing about the compiler, and a package that floats
#                changes the sandbox the tests run in without changing the tree.
#   the Rust     cargo, rustc, rustfmt and clippy answer from the pinned toolchain,
#                and the musl target is installed for it.
#   the C        `CC_x86_64_unknown_linux_musl` names a compiler that answers,
#                names the musl target when asked, and whose output is a static ELF
#                carrying musl's own `__stdio_exit` — and
#                `AR_x86_64_unknown_linux_musl` archives what that compiler makes.
#                The host's `cc` is a glibc compiler, and the C half of rustls's
#                provider built by it is a musl link that dies on what glibc's
#                headers name: naming the compiler for its target is what keeps a
#                build from reaching `cc` by accident. No C++ pair is pinned
#                because nothing in the tree asks for one — aws-lc-sys's `.cc`
#                files are its tests, and the CMake path that would compile them is
#                unreachable in a shell with no cmake.
#   the C library  `pkg-config` is supplied and it is the shell's own, and what it
#                answers is "no system AWS-LC or OpenSSL". It has exactly one
#                question here — whether aws-lc-sys links a system C library
#                instead of the vendored one, its third candidate — and the
#                artefact is forbidden a system OpenSSL. Dropping it looks like it
#                would make that a property of the shell and does the opposite:
#                devbox does not isolate /usr/bin, so the question would go to
#                whatever pkg-config the host installed, and one that resolves
#                `openssl` would answer for the host. The shell's own pkg-config
#                searches the shell's profile only, where no OpenSSL lives.
#   the absences `cmake` and `nasm` are not supplied, and `devbox.json` is held to
#                that: aws-lc-sys builds its C with the `cc` crate when CMake is
#                unmet, and ships its own assembler. Both would make the C half of
#                the artefact depend on the machine that built it.
#   the build    a musl build of a throwaway crate is a static ELF with no store
#                path in it that runs standalone.
#
# Three verdicts, and the exit code is part of the contract:
#
#   0   toolchain tier: ok (N checks)          the tier ran and passed
#   77  toolchain tier: skipped (no devbox shell)
#                                              it did not run; the reason is the
#                                              line above
#   1   toolchain tier: FAIL (N checks)        it ran and failed
#
# `make toolchain-tier` relays the three as `check: toolchain ... ok`, `... SKIPPED`
# and `... FAIL`, keeping exit 0 for the skip alone. Outside a devbox shell the
# tree has no toolchain to measure and saying so is the honest answer; inside one,
# every gap is the tree's or the shell's own and is a FAIL, including a compiler
# for the wrong libc — the output that excuses it is the output a broken shell and
# a broken tree print alike.
#
# bwrap is deliberately not checked here: whether a kernel lets an unprivileged
# process create a user namespace is a fact about the machine rather than about the
# toolchain devbox supplies, and it has a tier of its own
# (scripts/sandbox-probe.sh) that can skip loudly instead of failing a build.
#
#     devbox run -- bash scripts/toolchain-probe.sh
set -uo pipefail

# automake's "skipped" status, chosen so a skip cannot be mistaken for the 0 a
# tier that ran earns or the 1 a failure earns. `make toolchain-tier` reads it.
SKIPPED_STATUS=77

MUSL_TARGET=x86_64-unknown-linux-musl
CC_VAR=CC_x86_64_unknown_linux_musl
AR_VAR=AR_x86_64_unknown_linux_musl
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

checks=0
status=0
ok()   { printf 'ok   toolchain: %s\n' "$*"; checks=$((checks + 1)); }
bad()  { printf 'FAIL toolchain: %s\n' "$*"; status=1; }
skip() { printf 'skip toolchain: %s\n' "$*"; }

# finish -- the one place the verdict is printed, so there is one wording to grep.
finish() {
  if ((status)); then
    printf 'toolchain tier: FAIL (%d checks)\n' "$checks"
    exit 1
  fi
  printf 'toolchain tier: ok (%d checks)\n' "$checks"
  exit 0
}

# unavailable REASON -- this shell cannot run the tier. The verdict line is printed
# here and nowhere else, so there is one wording to grep for.
unavailable() {
  skip "$1"
  printf 'toolchain tier: skipped (no devbox shell)\n'
  exit "$SKIPPED_STATUS"
}

# is_static_elf FILE -- ELF magic and no interpreter's name in it. A dynamically
# linked binary carries PT_INTERP, hence the interpreter's name; a static one
# carries neither. No readelf and no file(1) needed, and the same reading
# scripts/release-probe.sh makes of the artefact.
is_static_elf() {
  [[ $(head -c 4 -- "$1" | od -An -c | tr -d ' ') == '177ELF' ]] &&
    ! grep -qa 'ld-musl' -- "$1" && ! grep -qa 'ld-linux' -- "$1"
}

# Not being in a devbox shell is the one thing this tier cannot measure around: the
# toolchain is what the shell supplies, and a tree with no cargo on PATH has none to
# read. It is printed, never counted as a pass, and keeps the run's exit 0.
if ! command -v cargo >/dev/null 2>&1; then
  unavailable 'cargo is not on PATH: run this inside `devbox shell`'
fi

# 1. Every package names a version or a commit: `name@<version>` where the version
#    starts with a digit, or a flake reference pinned to a 40-hex revision. A
#    floating package moves the sandbox the tests run in without moving the tree.
packages=$(sed -n '/"packages"[[:space:]]*:[[:space:]]*\[/,/^[[:space:]]*\]/p' "$repo_root/devbox.json" |
  sed -n 's/^[[:space:]]*"\([^"]*\)".*/\1/p' | grep -vx 'packages')
floating=()
counted=0
for package in $packages; do
  counted=$((counted + 1))
  version=${package##*@}
  if [[ $version != "$package" && $version =~ ^[0-9] ]]; then
    continue
  fi
  if [[ $package =~ /[0-9a-f]{40}(#|$) ]]; then
    continue
  fi
  floating+=("$package")
done
if ((counted == 0)); then
  bad 'devbox.json names no packages, so nothing is pinned'
elif ((${#floating[@]})); then
  bad "devbox.json lets packages float: ${floating[*]}"
else
  ok "every package in devbox.json names a version or a commit: $counted packages"
fi

# 2. The two tools the tree decided against, held to by the file that would supply
#    them: aws-lc-sys builds its C with the `cc` crate when CMake is unmet and
#    ships its own assembler, so a cmake or a nasm here would make the C half of
#    the artefact a fact about the machine. What the shell supplies is what
#    devbox.json names — the host's own tools are reachable on PATH and are not the
#    shell's, which is why the compiler below is named for its target rather than
#    assumed absent.
unwanted=()
for package in $packages; do
  case "$package" in
    cmake@* | nasm@* | *'#cmake' | *'#nasm') unwanted+=("$package") ;;
  esac
done
if ((${#unwanted[@]})); then
  bad "devbox.json names what the release builds without: ${unwanted[*]}"
else
  ok 'devbox.json names neither cmake nor nasm: the cc crate builds the C, and the assembler ships with it'
fi

# 3. The pin the shell installs and the pin the tree builds against are one fact.
#    `rust-version = "1.99"` is a minor, `rustup default 1.99.0` a patch, so the
#    tree's is a prefix of the shell's.
rust_version=$(sed -n 's/^rust-version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$repo_root/Cargo.toml" | head -1)
hook_rust=$(sed -n 's/.*rustup default[[:space:]]\{1,\}\([0-9][0-9.]*\).*/\1/p' "$repo_root/devbox.json" | head -1)
if [[ -z $rust_version ]]; then
  bad 'Cargo.toml pins no rust-version, so the shell has nothing to agree with'
elif [[ -z $hook_rust ]]; then
  bad 'devbox.json init hook never sets `rustup default`, so no Rust is pinned'
elif [[ $hook_rust != "$rust_version" && $hook_rust != "$rust_version".* ]]; then
  bad "devbox.json's rustup default is $hook_rust and Cargo.toml's rust-version is $rust_version: the gate would measure a compiler nobody pinned"
else
  ok "the shell's rustup default $hook_rust agrees with Cargo.toml's rust-version $rust_version"
fi
PINNED_RUST=${hook_rust:-$rust_version}

# 4. cargo --version reports the pinned Rust.
if got=$(cargo --version 2>/dev/null); then
  case "$got" in
    "cargo $PINNED_RUST "*) ok "cargo --version: $got" ;;
    *)                      bad "cargo --version: '$got', want cargo $PINNED_RUST" ;;
  esac
else
  bad 'cargo not on PATH'
fi

# 5. rustc --version reports the pinned Rust.
if got=$(rustc --version 2>/dev/null); then
  case "$got" in
    "rustc $PINNED_RUST "*) ok "rustc --version: $got" ;;
    *)                      bad "rustc --version: '$got', want rustc $PINNED_RUST" ;;
  esac
else
  bad 'rustc not on PATH'
fi

# 6. rustfmt and clippy ship with the pinned toolchain. Their own version
#    numbers are not the Rust version (rustfmt 1.x, clippy 0.1.<minor>), so ask
#    rustup which components the pinned toolchain actually has.
if got=$(rustup component list --installed --toolchain "$PINNED_RUST" 2>&1); then
  minor=$(cut -d. -f2 <<<"$PINNED_RUST")
  for component in rustfmt clippy; do
    if grep -qx "$component-x86_64-unknown-linux-gnu" <<<"$got"; then
      ok "rustup component: $component installed for $PINNED_RUST"
    else
      bad "rustup component: $component missing for $PINNED_RUST"
    fi
  done
  if got=$(cargo fmt --version 2>&1); then
    ok "cargo fmt --version: $got"
  else
    bad "cargo fmt: $got"
  fi
  if got=$(cargo clippy --version 2>&1); then
    case "$got" in
      "clippy 0.1.$minor "*) ok "cargo clippy --version: $got" ;;
      *)                     bad "cargo clippy --version: '$got', want clippy 0.1.$minor" ;;
    esac
  else
    bad "cargo clippy: $got"
  fi
else
  bad "rustup component list: $got"
fi

# 7. The musl target is installed for the pinned toolchain.
if got=$(rustup target list --installed --toolchain "$PINNED_RUST" 2>&1) &&
   grep -qx "$MUSL_TARGET" <<<"$got"; then
  ok "rustup target: $MUSL_TARGET installed for $PINNED_RUST"
else
  bad "rustup target: $MUSL_TARGET not installed for $PINNED_RUST (installed: $(tr '\n' ' ' <<<"${got:-}"))"
fi

# 8. The C compiler is named for its target. The value may carry arguments —
#    `zig cc -target x86_64-linux-musl` is a program and a flag — which is how the
#    `cc` crate reads it too.
work=$(mktemp -d "${TMPDIR:-/dev/shm}/sindri-toolchain-probe.XXXXXX") ||
  { bad 'no temporary directory to build in'; finish; }
trap 'rm -rf "$work"' EXIT
cat >"$work/musl-c.c" <<'C'
#include <stdio.h>

int main(void) {
    puts("musl-ok");
    return 0;
}
C
cc_cmd=()
ar_cmd=()
cc_value=
ar_value=
if [[ -v $CC_VAR ]]; then cc_value=${!CC_VAR}; fi
if [[ -v $AR_VAR ]]; then ar_value=${!AR_VAR}; fi
if [[ -n $cc_value ]]; then read -r -a cc_cmd <<<"$cc_value"; fi
if [[ -n $ar_value ]]; then read -r -a ar_cmd <<<"$ar_value"; fi

if [[ -z $cc_value ]]; then
  bad "$CC_VAR is not set, so no compiler is named for $MUSL_TARGET and a build reaches the host's cc"
else
  if said=$("${cc_cmd[@]}" -dumpmachine 2>&1); then
    case "$said" in
      x86_64-*musl*) ok "$CC_VAR names the musl target: $said" ;;
      *)
        bad "$CC_VAR is '$cc_value', and for $MUSL_TARGET it names '$said': that is a compiler for another libc"
        ;;
    esac
  else
    bad "$CC_VAR is '$cc_value', and it does not answer: $said"
  fi
fi

# 9. What that compiler builds is musl and not glibc, read off the binary the way
#    scripts/release-probe.sh reads the artefact: musl defines `__stdio_exit`,
#    glibc does not, and a static binary needs no interpreter from either. The
#    host's cc produces neither a static ELF here nor that symbol.
c_built=0
if [[ -n $cc_value ]]; then
  if said=$("${cc_cmd[@]}" -static -o "$work/musl-c" "$work/musl-c.c" 2>&1); then
    c_built=1
  else
    bad "$CC_VAR is '$cc_value', and cannot build a C program for $MUSL_TARGET: $said"
  fi
fi
# Nothing is read off a program that was never built: the failure above says why,
# and a second line about a file that does not exist would blame the wrong fact.
if ((c_built)); then
  if is_static_elf "$work/musl-c"; then
    ok "a C program it builds for $MUSL_TARGET is a static ELF"
  else
    bad "a C program built for $MUSL_TARGET is not a static ELF"
  fi
  # `__stdio_exit` lives in the symbol table, so the name is in the bytes; glibc
  # does not define it and says who it is instead. Both read with grep, because a
  # toolchain tier may not borrow a fact from the host's binutils.
  if grep -qa '__stdio_exit' -- "$work/musl-c" && ! grep -qa 'GNU C Library' -- "$work/musl-c"; then
    ok "the C it builds is musl's, not glibc's: __stdio_exit and no GNU C Library"
  else
    bad "the C built for $MUSL_TARGET is not musl's: no __stdio_exit or a glibc marker"
  fi
fi

# 10. The archiver named for the target archives an object that compiler made. An
#     `ar` archive carries its members' names as plain strings, so the member is
#     found without calling the host's `ar` to list it.
if [[ -z $ar_value ]]; then
  bad "$AR_VAR is not set, so no archiver is named for $MUSL_TARGET"
elif ((c_built)); then
  if said=$("${cc_cmd[@]}" -c -o "$work/musl-c.o" "$work/musl-c.c" 2>&1) &&
     said=$("${ar_cmd[@]}" rcs "$work/libmusl-c.a" "$work/musl-c.o" 2>&1) &&
     grep -qa 'musl-c.o' -- "$work/libmusl-c.a"; then
    ok "$AR_VAR archives what the musl compiler builds: $ar_value"
  else
    bad "$AR_VAR is '$ar_value', and it does not archive a musl object: $said"
  fi
fi

# 11. The pkg-config question, answered by the shell's own pkg-config: it is the
#     one thing that makes "no system OpenSSL in the artefact" a property of the
#     toolchain. `readlink -f` because the profile's entry is a link into the store.
pkg_config_path=$(command -v pkg-config 2>/dev/null) || pkg_config_path=
resolved=
if [[ -n $pkg_config_path ]]; then
  resolved=$(readlink -f "$pkg_config_path" 2>/dev/null) || resolved=
fi
if [[ $resolved == /nix/store/* ]]; then
  ok "the pkg-config on PATH is the shell's own: $resolved"
else
  bad "the pkg-config on PATH is '${resolved:-none}', which the shell does not supply: a system AWS-LC or OpenSSL would be the answer it gives"
fi
if [[ -z $pkg_config_path ]]; then
  bad 'no pkg-config on PATH, so nothing can be said about a system AWS-LC or OpenSSL'
else
  hits=()
  for module in openssl libcrypto aws-lc; do
    if pkg-config --exists "$module" 2>/dev/null; then
      hits+=("$module at $(pkg-config --variable=prefix "$module" 2>/dev/null)")
    fi
  done
  if ((${#hits[@]})); then
    bad "pkg-config resolves a system C library the artefact may not link: ${hits[*]}"
  else
    ok 'pkg-config finds no system AWS-LC or OpenSSL: the vendored C is the only candidate'
  fi
fi

# 12. A musl build succeeds and yields a static ELF, proved on a throwaway crate
#     outside the repo so no product source is needed.
mkdir -p "$work/src"
cat >"$work/Cargo.toml" <<'TOML'
[package]
name = "probe"
version = "0.0.0"
edition = "2021"
TOML
cat >"$work/src/main.rs" <<'RS'
fn main() {
    println!("musl-ok");
}
RS
# The throwaway crate builds into its own target directory, named here rather than
# inherited: an exported CARGO_TARGET_DIR — which is how a worktree shares a target
# directory with the rest of the tree — would otherwise put the artefact somewhere
# this probe does not look, and a passing toolchain would be reported as broken.
if out=$(CARGO_TARGET_DIR="$work/target" cargo build --quiet --release --target "$MUSL_TARGET" \
  --manifest-path "$work/Cargo.toml" 2>&1); then
  ok "cargo build --target $MUSL_TARGET: succeeded"
  bin="$work/target/$MUSL_TARGET/release/probe"
  if is_static_elf "$bin"; then
    ok "musl artefact is a static ELF: $(basename "$bin")"
  else
    bad "musl artefact at $bin is not a static ELF"
  fi
  # devbox supplies build-time tools only, so the compiler's own Nix store path
  # must not survive into the artefact (ADR 0001).
  if grep -qa '/nix/store' "$bin"; then
    bad "musl artefact carries /nix/store paths: $(grep -ao '/nix/store[^ ]*' "$bin" | head -3 | tr '\n' ' ')"
  else
    ok "musl artefact carries no /nix/store paths"
  fi
  if got=$("$bin" 2>&1) && [[ $got == "musl-ok" ]]; then
    ok "musl artefact runs standalone: printed '$got'"
  else
    bad "musl artefact does not run standalone: '$got'"
  fi
else
  bad "cargo build --target $MUSL_TARGET failed: $out"
fi

# 13. Nothing the product ships may refer to a Nix store path: devbox supplies
#     build-time tools only. Cargo workspace manifests, sources and build scripts
#     are all checked.
sources=()
for dir in "$repo_root"/crates "$repo_root"/src "$repo_root"/.cargo; do
  [[ -d $dir ]] && sources+=("$dir")
done
while IFS= read -r f; do sources+=("$f"); done < <(
  find "$repo_root" -maxdepth 2 \( -name Cargo.toml -o -name build.rs \) -not -path '*/.git/*' -not -path '*/.worktrees/*' -print
)
if ((${#sources[@]} == 0)); then
  skip 'no /nix/store references: no product sources yet (ticket #3 creates them)'
else
  grep_status=0
  hits=$(grep -rIl '/nix/store' "${sources[@]}" 2>/dev/null) || grep_status=$?
  if ((grep_status == 0)); then
    bad "/nix/store references in product sources: $(tr '\n' ' ' <<<"$hits")"
  elif ((grep_status == 1)); then
    ok 'no /nix/store references in product sources'
  else
    bad "the product sources could not be read: grep exited $grep_status"
  fi
fi

finish
