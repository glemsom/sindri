#!/usr/bin/env bash
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$here/.." && pwd) || exit 2
cd "$repo" || exit 2

checks=0
status=0
ok()   { printf 'ok   no-relink: %s\n' "$*"; checks=$((checks + 1)); }
bad()  { printf 'FAIL no-relink: %s\n' "$*"; status=1; }

finish() {
  if ((status)); then
    printf 'no-relink: FAIL (%d checks)\n' "$checks"
    exit 1
  fi
  printf 'no-relink: ok (%d checks)\n' "$checks"
  exit 0
}

if ! command -v cargo >/dev/null 2>&1; then
  bad 'cargo is not on PATH: run this inside `devbox shell`'
  finish
fi
if ! command -v git >/dev/null 2>&1; then
  bad 'git is not on PATH: this proof reads the commit it is proving'
  finish
fi
if ! git -C "$repo" rev-parse --git-dir >/dev/null 2>&1; then
  bad 'this is not a git checkout: a source tarball has no commit to describe'
  finish
fi

binary=${CARGO_TARGET_DIR:-$repo/target}/debug/sindri

fingerprint() {
  local hash stat
  hash=$(sha256sum -- "$1") || return 1
  stat=$(stat -c '%y %i' -- "$1") || return 1
  printf 'sha256=%s mtime=%s' "${hash%% *}" "$stat"
}

build() {
  BUILD_SAID=$(cargo build --package sindri 2>&1)
}

compiled_anything() {
  grep -q '^[[:space:]]*Compiling ' <<<"$BUILD_SAID"
}

check_same_binary() {
  local label=$1 before=$2 after
  if ! build; then
    bad "$label: cargo build failed"
    printf '%s\n' "$BUILD_SAID"
    return
  fi
  after=$(fingerprint "$binary") || {
    bad "$label: no binary at $binary to fingerprint"
    return
  }
  if [[ $after == "$before" ]]; then
    ok "$label: the same binary, $after"
  else
    bad "$label: the binary moved — was [$before], now [$after]"
  fi
  if compiled_anything; then
    bad "$label: cargo compiled $(tr '\n' ' ' <<<"$(grep '^[[:space:]]*Compiling ' <<<"$BUILD_SAID")")"
  else
    ok "$label: cargo compiled nothing"
  fi
}

if ! build; then
  bad 'the first build failed'
  printf '%s\n' "$BUILD_SAID"
  finish
fi
if [[ ! -x $binary ]]; then
  bad "the build left no binary at $binary"
  finish
fi
settled=$(fingerprint "$binary") || {
  bad "cannot fingerprint $binary"
  finish
}
ok "built the tree: $settled"

check_same_binary 'an unchanged tree' "$settled"

doc=GLOSSARY.md
if [[ ! -f $doc ]]; then
  bad "there is no $doc to touch, so a non-compiled change cannot be shown"
else
  before_touch=$(fingerprint "$binary")
  touch -- "$doc"
  check_same_binary "a touched $doc" "$before_touch"
fi

index=$(git -C "$repo" rev-parse --git-path index 2>/dev/null)
[[ -n ${index:-} && $index != /* ]] && index=$repo/$index
if [[ -z ${index:-} || ! -f $index ]]; then
  bad 'no git index to move, so the build script cannot be made to say it twice'
else
  before_rerun=$(fingerprint "$binary")
  touch -- "$index"
  if ! build; then
    bad 'the build after moving the index failed'
    printf '%s\n' "$BUILD_SAID"
  else
    after_rerun=$(fingerprint "$binary")
    if [[ ${after_rerun%% mtime=*} == "${before_rerun%% mtime=*}" ]]; then
      ok "a re-run build script: the same bytes at a later moment, ${after_rerun%% mtime=*}"
    else
      bad "a re-run build script changed the bytes: was [${before_rerun%% mtime=*}], now [${after_rerun%% mtime=*}]"
    fi
  fi
fi

printed=$("$binary" --version 2>&1) || printed=
if [[ -z $printed ]]; then
  bad "$binary --version said nothing"
fi
describe=$(git -C "$repo" describe --tags --always --dirty 2>/dev/null) || describe=
commit_date=$(git -C "$repo" log -1 --format=%cs 2>/dev/null) || commit_date=
if [[ -z $describe || -z $commit_date ]]; then
  if [[ $printed =~ ^sindri\ [^[:space:]]+\ \(.*\)$ ]]; then
    ok "no commit to describe; the version falls back to '$printed'"
  else
    bad "no commit to describe and '$printed' is not the fallback shape"
  fi
else
  wanted="sindri $describe ($commit_date)"
  if [[ $printed == "$wanted" ]]; then
    ok "the binary says '$printed', which is git describe and the commit date"
  else
    bad "the binary says '$printed', want '$wanted'"
  fi
  if grep -qaF -- "$describe ($commit_date)" "$binary"; then
    ok "the binary carries '$describe ($commit_date)' as bytes, not as a lookup"
  else
    bad "the binary does not carry '$describe ($commit_date)' in its bytes"
  fi
  today=$(date -u +%F 2>/dev/null) || today=
  if [[ -z $today ]]; then
    bad 'no date(1) to read the wall clock with'
  elif [[ $today == "$commit_date" ]]; then
    printf 'note no-relink: the commit was made today (%s), so the clock and the commit agree\n' "$today"
  elif [[ $printed == *"($today)"* ]]; then
    bad "the binary says '($today)', which is the day it was built, not the day it was committed"
  else
    ok "no $today anywhere in the version: the date is the commit's, not the clock's"
  fi
fi

finish
