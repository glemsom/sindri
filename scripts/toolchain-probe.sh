#!/usr/bin/env bash
set -uo pipefail

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

finish() {
  if ((status)); then
    printf 'toolchain tier: FAIL (%d checks)\n' "$checks"
    exit 1
  fi
  printf 'toolchain tier: ok (%d checks)\n' "$checks"
  exit 0
}

unavailable() {
  skip "$1"
  printf 'toolchain tier: skipped (no devbox shell)\n'
  exit "$SKIPPED_STATUS"
}

is_static_elf() {
  [[ $(head -c 4 -- "$1" | od -An -c | tr -d ' ') == '177ELF' ]] &&
    ! grep -qa 'ld-musl' -- "$1" && ! grep -qa 'ld-linux' -- "$1"
}

if ! command -v cargo >/dev/null 2>&1; then
  unavailable 'cargo is not on PATH: run this inside `devbox shell`'
fi

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

if got=$(cargo --version 2>/dev/null); then
  case "$got" in
    "cargo $PINNED_RUST "*) ok "cargo --version: $got" ;;
    *)                      bad "cargo --version: '$got', want cargo $PINNED_RUST" ;;
  esac
else
  bad 'cargo not on PATH'
fi

if got=$(rustc --version 2>/dev/null); then
  case "$got" in
    "rustc $PINNED_RUST "*) ok "rustc --version: $got" ;;
    *)                      bad "rustc --version: '$got', want rustc $PINNED_RUST" ;;
  esac
else
  bad 'rustc not on PATH'
fi

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

if got=$(rustup target list --installed --toolchain "$PINNED_RUST" 2>&1) &&
   grep -qx "$MUSL_TARGET" <<<"$got"; then
  ok "rustup target: $MUSL_TARGET installed for $PINNED_RUST"
else
  bad "rustup target: $MUSL_TARGET not installed for $PINNED_RUST (installed: $(tr '\n' ' ' <<<"${got:-}"))"
fi

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

c_built=0
if [[ -n $cc_value ]]; then
  if said=$("${cc_cmd[@]}" -static -o "$work/musl-c" "$work/musl-c.c" 2>&1); then
    c_built=1
  else
    bad "$CC_VAR is '$cc_value', and cannot build a C program for $MUSL_TARGET: $said"
  fi
fi
if ((c_built)); then
  if is_static_elf "$work/musl-c"; then
    ok "a C program it builds for $MUSL_TARGET is a static ELF"
  else
    bad "a C program built for $MUSL_TARGET is not a static ELF"
  fi
  if grep -qa '__stdio_exit' -- "$work/musl-c" && ! grep -qa 'GNU C Library' -- "$work/musl-c"; then
    ok "the C it builds is musl's, not glibc's: __stdio_exit and no GNU C Library"
  else
    bad "the C built for $MUSL_TARGET is not musl's: no __stdio_exit or a glibc marker"
  fi
fi

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
if out=$(CARGO_TARGET_DIR="$work/target" cargo build --quiet --release --target "$MUSL_TARGET" \
  --manifest-path "$work/Cargo.toml" 2>&1); then
  ok "cargo build --target $MUSL_TARGET: succeeded"
  bin="$work/target/$MUSL_TARGET/release/probe"
  if is_static_elf "$bin"; then
    ok "musl artefact is a static ELF: $(basename "$bin")"
  else
    bad "musl artefact at $bin is not a static ELF"
  fi
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
