#!/usr/bin/env bash
# Proves the devbox shell supplies the build toolchain pinned by ticket #4
# (see docs/adr/0001-devbox-builds-the-product.md). Every check is a fact the
# spec cares about, so this script is the acceptance test for that ticket and
# is wired into `make check` by ticket #5.
#
# Run it inside the devbox shell:
#     devbox run -- bash scripts/toolchain-probe.sh
# Exits non-zero if any check fails; prints one line per check.
#
# bwrap is deliberately not checked here any more: whether a kernel lets an
# unprivileged process create a user namespace is a fact about the machine rather
# than about the toolchain devbox supplies, and it now has a tier of its own
# (scripts/sandbox-probe.sh) that can skip loudly instead of failing a build.
set -uo pipefail

PINNED_RUST=1.99.0
MUSL_TARGET=x86_64-unknown-linux-musl
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

status=0
ok()   { printf 'ok   %s\n' "$*"; }
bad()  { printf 'FAIL %s\n' "$*"; status=1; }
skip() { printf 'skip %s\n' "$*"; }

# 1. cargo --version reports the pinned Rust.
if got=$(cargo --version 2>/dev/null); then
  case "$got" in
    "cargo $PINNED_RUST "*) ok "cargo --version: $got" ;;
    *)                      bad "cargo --version: '$got', want cargo $PINNED_RUST" ;;
  esac
else
  bad "cargo not on PATH"
fi

# 2. rustc --version reports the pinned Rust.
if got=$(rustc --version 2>/dev/null); then
  case "$got" in
    "rustc $PINNED_RUST "*) ok "rustc --version: $got" ;;
    *)                      bad "rustc --version: '$got', want rustc $PINNED_RUST" ;;
  esac
else
  bad "rustc not on PATH"
fi

# 3. rustfmt and clippy ship with the pinned toolchain. Their own version
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

# 4. The musl target is installed for the pinned toolchain.
if got=$(rustup target list --installed --toolchain "$PINNED_RUST" 2>&1) &&
   grep -qx "$MUSL_TARGET" <<<"$got"; then
  ok "rustup target: $MUSL_TARGET installed for $PINNED_RUST"
else
  bad "rustup target: $MUSL_TARGET not installed for $PINNED_RUST (installed: $(tr '\n' ' ' <<<"${got:-}"))"
fi

# 5. A musl build succeeds and yields a static ELF, proved on a throwaway crate
#    outside the repo so no product source is needed.
crate=$(mktemp -d "${TMPDIR:-/dev/shm}/sindri-toolchain-probe.XXXXXX") || exit 1
trap 'rm -rf "$crate"' EXIT
mkdir -p "$crate/src"
cat >"$crate/Cargo.toml" <<'TOML'
[package]
name = "probe"
version = "0.0.0"
edition = "2021"
TOML
cat >"$crate/src/main.rs" <<'RS'
fn main() {
    println!("musl-ok");
}
RS
if out=$(cargo build --quiet --release --target "$MUSL_TARGET" --manifest-path "$crate/Cargo.toml" 2>&1); then
  ok "cargo build --target $MUSL_TARGET: succeeded"
  bin="$crate/target/$MUSL_TARGET/release/probe"
  # A dynamically linked binary carries PT_INTERP, hence the interpreter's
  # name; a static one carries neither. No readelf or file(1) needed.
  if [[ $(head -c 4 "$bin" | od -An -c | tr -d ' ') == '177ELF' ]] &&
     ! grep -qa 'ld-musl' "$bin" && ! grep -qa 'ld-linux' "$bin"; then
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

# 6. Nothing the product ships may refer to a Nix store path: devbox supplies
#    build-time tools only. Cargo workspace manifests, sources and build
#    scripts are all checked; empty today because ticket #3 creates the crates.
sources=()
for dir in "$repo_root"/crates "$repo_root"/src "$repo_root"/.cargo; do
  [[ -d $dir ]] && sources+=("$dir")
done
while IFS= read -r f; do sources+=("$f"); done < <(
  find "$repo_root" -maxdepth 2 \( -name Cargo.toml -o -name build.rs \) -not -path '*/.git/*' -not -path '*/.worktrees/*' -print
)
if ((${#sources[@]} == 0)); then
  skip "no /nix/store references: no product sources yet (ticket #3 creates them)"
elif hits=$(grep -rIl '/nix/store' "${sources[@]}" 2>/dev/null); then
  bad "/nix/store references in product sources: $(tr '\n' ' ' <<<"$hits")"
else
  ok "no /nix/store references in product sources"
fi

exit "$status"
