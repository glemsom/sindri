#!/usr/bin/env bash
# The release stage of `make check` (spec §16, ADR 0001; ticket #62): the release
# path exercised by the gate rather than by hand on release day. It builds the
# artefact the product ships — a static `x86_64-unknown-linux-musl` binary — and
# then asserts four things about the bytes it made:
#
#   1. static      nothing in it names an ELF interpreter, so it needs no loader
#                  from the machine it runs on;
#   2. musl        it defines musl's own `__stdio_exit` and carries no glibc
#                  marker, which is what `static` alone cannot tell you — a
#                  statically linked glibc binary is static and is not this;
#   3. no OpenSSL  neither a symbol nor a string in it names OpenSSL, because a
#                  musl binary cannot use the GNU NSS resolver and TLS therefore
#                  goes through rustls (ADR 0001). `openssl-probe` reads an env
#                  var and is not OpenSSL; `SSL_CTX_new` is;
#   4. it runs     with no devbox, no Nix and no fonts: an empty root, an empty
#                  environment, no PATH, and the artefact as the only file in it,
#                  printing what it prints here. A binary that needs a Nix store
#                  path, a system library or a font to start cannot.
#
# The build reuses CARGO_TARGET_DIR, like every other cargo call in the gate: a
# tree that has already built the artefact pays a freshness check, not a compile.
#
# Three verdicts, and the exit code is part of the contract:
#
#   0   release: ok (N checks)                 the artefact was built and held
#   77  release: skipped (...)                 the machine would not let it run;
#                                              the reason is the line above
#   1   release: FAIL (...)                    it ran and failed
#
# Two reasons to skip, and only two, both facts about the machine rather than the
# tree: no bwrap, or a kernel that will not let bwrap unshare — the same pair
# scripts/sandbox-probe.sh skips on — and no musl C compiler, which is where this
# stage stands today (see `toolchain_gap` below). A skip is printed, never counted
# as a pass, and keeps the run's exit 0.
#
#     devbox run -- make release
#     devbox run -- bash scripts/release-probe.sh
set -uo pipefail

# automake's "skipped" status, chosen so a skip cannot be mistaken for the 0 a
# stage that ran earns or the 1 a failure earns. `make release` reads it.
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

# finish -- the one place the verdict is printed, so there is one wording to grep.
# A failure outranks a skip: a check that ran and failed must not be excused by a
# capability the machine lacked later on.
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

# toolchain_gap BUILD_OUTPUT -- true when the build failed for the one reason this
# stage knows by name. devbox supplies a C compiler for the host and no compiler
# for the musl target, so the C half of rustls's crypto provider (aws-lc-rs) is
# compiled by the host's compiler against the host's C headers; nixpkgs' compiler
# wrapper appends its own hardening and dialect flags after any this stage passes,
# and those headers then name glibc-only stdio entry points that rustc's bundled
# musl does not define. The fix is a musl C compiler in devbox.json, which is the
# toolchain's business and not this script's, so the stage says so and skips
# rather than failing a tree that is not at fault. Any other link or compile
# error is a FAIL: this predicate is the symbols below and nothing else.
toolchain_gap() {
  local said=$1
  local symbol
  for symbol in __isoc23_sscanf __isoc99_sscanf __fprintf_chk __snprintf_chk; do
    if [[ $said == *"undefined reference to \`$symbol'"* ]]; then
      return 0
    fi
  done
  return 1
}

if ! command -v cargo >/dev/null 2>&1; then
  bad 'cargo is not on PATH: run this inside `devbox shell`'
  finish
fi

# 1. The artefact, built the way the product ships it.
if said=$(cargo build --release --target "$target" --package sindri 2>&1); then
  ok "cargo build --release --target $target --package sindri"
elif toolchain_gap "$said"; then
  skip "$(grep -m1 'undefined reference to' <<<"$said" | sed 's/^ *//')"
  skip "the musl target's C is compiled against glibc headers and devbox supplies no musl C compiler"
  printf 'release: skipped (no musl C compiler)\n'
  exit "$SKIPPED_STATUS"
else
  bad "cargo build --release --target $target --package sindri failed:"
  printf '%s\n' "$said"
  finish
fi

if [[ ! -f $artefact ]]; then
  bad "no artefact at $artefact after a successful build"
  finish
fi

# 2. Static: a dynamic ELF carries PT_INTERP, hence its interpreter's name. No
#    readelf needed, and the check is the same one scripts/toolchain-probe.sh
#    makes of a throwaway crate.
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

# 3. musl and not glibc: musl defines `__stdio_exit`, which glibc does not have,
#    and a glibc-linked artefact carries the library's own version string. Both
#    halves, because either alone can pass a binary that is not this one.
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

# 4. No OpenSSL: the symbol table and the strings beside it, one pass over both,
#    because a statically linked libssl would leave its names in either. The
#    markers are names OpenSSL actually uses, matched at a word boundary and not
#    merely as substrings: aws-lc-rs is the C half of rustls's provider and it
#    keeps OpenSSL's *API* names behind its own prefix (`aws_lc_0_47_0_SSL_CTX_new`),
#    which is BoringSSL saying it speaks the same protocol and is not the library
#    ADR 0001 forbids. `openssl-probe` reads `SSL_CERT_FILE`; that is a path, not
#    a symbol, and it is not in this list either.
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

# 5. It runs with nothing around it. bwrap makes the empty root, `--clearenv`
#    empties the environment, and the artefact is the only file mounted into it —
#    so there is no devbox shell, no /nix, no /usr/share/fonts and no dynamic
#    loader for anything to find. The bytes it prints are compared with the bytes
#    the same artefact prints here, which is the strongest claim available without
#    a second machine: same program, same words, nothing to lean on.
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
