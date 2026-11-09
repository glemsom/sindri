#!/usr/bin/env bash
set -uo pipefail

SKIPPED_STATUS=77

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$here/.." && pwd) || exit 2
target=x86_64-unknown-linux-musl
target_dir=${CARGO_TARGET_DIR:-$repo/target}
artefact=$target_dir/$target/release/sindri

checks=0
status=0
bwrap_why=
ok()   { printf 'ok   release: %s\n' "$*"; checks=$((checks + 1)); }
skip() { printf 'skip release: %s\n' "$*"; }
bad()  { printf 'FAIL release: %s\n' "$*"; status=1; }

finish() {
  if ((status)); then
    printf 'release: FAIL (%d checks)\n' "$checks"
    exit 1
  fi
  if [[ -n $bwrap_why ]]; then
    printf 'release: skipped (bwrap unavailable)\n'
    exit "$SKIPPED_STATUS"
  fi
  printf 'release: ok (%d checks)\n' "$checks"
  exit 0
}

if ! command -v cargo >/dev/null 2>&1; then
  bad 'cargo is not on PATH: run this inside `devbox shell`'
  finish
fi

if ! command -v cargo-zigbuild >/dev/null 2>&1; then
  bad 'cargo-zigbuild is not on PATH: run this inside `devbox shell`'
  finish
fi

if said=$(RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C strip=none" \
    cargo zigbuild --release --target "$target" --package sindri 2>&1); then
  ok "cargo zigbuild --release --target $target --package sindri"
else
  bad "cargo zigbuild --release --target $target --package sindri failed:"
  printf '%s\n' "$said"
  finish
fi

if [[ ! -f $artefact ]]; then
  bad "no artefact at $artefact after a successful build"
  finish
fi

if ! said=$(strip --strip-debug -- "$artefact" 2>&1); then
  bad "strip --strip-debug could not drop the debug info from $(basename -- "$artefact"): $said"
  finish
fi

interpreters=
for name in ld-musl ld-linux; do
  if grep -qa -- "$name" "$artefact"; then
    interpreters+="$name "
  fi
done
if [[ -n $interpreters ]]; then
  bad "artefact is not static: it names ${interpreters% }"
elif [[ $(head -c 4 -- "$artefact" | od -An -c | tr -d ' ') != '177ELF' ]]; then
  bad "artefact is not an ELF at all: $(basename -- "$artefact")"
else
  ok "artefact is a static ELF: $(basename -- "$artefact")"
fi

if [[ ! -s $artefact ]]; then
  bad "artefact is empty: $artefact"
elif ! symbols=$(nm -- "$artefact" 2>/dev/null) || [[ -z $symbols ]]; then
  bad "artefact carries no symbol table, so musl cannot be shown: $(basename -- "$artefact")"
elif ! grep -qw '__stdio_exit' <<<"$symbols"; then
  bad "artefact is not musl: it does not define musl's __stdio_exit"
elif glibc=$(grep -oa 'GNU C Library[^\"]*' -- "$artefact" | head -1) && [[ -n $glibc ]]; then
  bad "artefact is not musl: it says '$glibc'"
else
  ok "artefact is musl, not glibc: it defines __stdio_exit and names no GNU C Library"
fi

openssl_hits=$(
  {
    nm -- "$artefact" 2>/dev/null
    strings -a -- "$artefact" 2>/dev/null
  } | grep -oaE '(^|[^_[:alnum:]])(SSL_CTX_new|SSL_library_init|SSL_get_verify_result|OPENSSL_init_ssl)|libssl\.so[.0-9]*|libcrypto\.so[.0-9]*|OpenSSL [0-9]+\.[0-9]+' | sort -u
)
if [[ -n $openssl_hits ]]; then
  bad "the artefact carries OpenSSL: $(tr '\n' ' ' <<<"$openssl_hits")"
else
  ok 'no OpenSSL symbol or string in the artefact'
fi

if store=$(grep -oa '/nix/store/[^[:space:]]*' -- "$artefact" | head -1 | cut -c1-120) &&
   [[ -n $store ]]; then
  bad "the artefact names a store path: $store"
else
  ok 'no Nix store path in the artefact'
fi

if ! command -v bwrap >/dev/null 2>&1; then
  skip 'bwrap not on PATH'
  bwrap_why='bwrap not on PATH'
elif ! here_said=$("$artefact" --version 2>&1); then
  bad "the artefact does not run here: $here_said"
else
  sandbox=(--unshare-all --die-with-parent --clearenv --ro-bind "$artefact" /sindri)
  if ! there_said=$(bwrap "${sandbox[@]}" /sindri --version 2>&1); then
    skip "bwrap cannot create an unprivileged sandbox: ${there_said:-no output}"
    bwrap_why='bwrap cannot create an unprivileged sandbox'
  elif [[ $there_said != "$here_said" ]]; then
    bad "the artefact says '$there_said' alone and '$here_said' here"
  else
    ok "runs with no devbox, no Nix and no fonts: printed '$there_said'"
  fi
fi

finish
