#!/usr/bin/env bash
# The no-relink proof of `make check` (spec §16; ticket #61): the binary carries
# the `git describe` of the commit it was built at and that commit's own date,
# never the wall clock, so a tree nobody changed must build to the same bytes
# twice without compiling anything the second time.
#
# It proves that the only way such a claim can be proven — by watching the
# binary rather than by trusting a build log. Each build here is fingerprinted
# three ways, because a hash alone is not evidence: two builds separated by a
# full `cargo clean` would agree on the hash and prove nothing about relinking.
# The fingerprint is the sha256 with the stat beside it, mtime and inode both, so
# "not rebuilt" is a claim about the same file sitting untouched on disk and
# "identical" is a claim about its contents, and neither can stand in for the
# other. The build log is read as well: a `Compiling` line on a rebuild is a
# relink whatever the bytes say.
#
# The tree is watched through four moves, and each of the four Done-when bullets
# of #61 is answered by one of them:
#
#   1. build; build again           an unchanged tree relinks nothing, and the
#                                   binary is byte-identical (hash, mtime, inode)
#   2. touch a doc no compiler sees the file that moved and the binary that did
#                                   not: still no relink, still the same bytes
#   3. re-run the build script      the provenance is a pure function of git's
#                                   state: a later re-run says the same thing,
#                                   which is where a timestamp would show up
#   4. read the version back        `--version` prints the describe and commit
#                                   date git reports, and the same string is
#                                   found embedded in the binary itself, so it
#                                   is a compile-time constant and not something
#                                   a run computes
#
# WHAT THIS DOES NOT CATCH, said here so nobody has to guess. The wall clock is
# excluded by construction — the provenance script names no clock, and the date
# it embeds is the commit's, which move 4 reads back from git — but no proof run
# on one machine can rebuild the same source at two different moments in time. A
# clock read at a granularity of a whole day would only be caught on a day that
# is not the commit's day (move 4 checks exactly that, and says so when today and
# the commit agree, rather than passing on a coincidence). A tree with no git — a
# source tarball — has no describe to compare against; move 4 then checks the
# shape the fallback takes and nothing more.
#
# Two verdicts, and the exit code is part of the contract:
#
#   0   no-relink: ok (N checks)     every move held
#   1   no-relink: FAIL (...)        one did not; the line above says which
#
#     devbox run -- make no-relink
#     devbox run -- bash scripts/no-relink-proof.sh
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$here/.." && pwd) || exit 2
cd "$repo" || exit 2

checks=0
status=0
ok()   { printf 'ok   no-relink: %s\n' "$*"; checks=$((checks + 1)); }
bad()  { printf 'FAIL no-relink: %s\n' "$*"; status=1; }

# finish -- the one place the verdict is printed, so there is one wording to grep.
finish() {
  if ((status)); then
    printf 'no-relink: FAIL (%d checks)\n' "$checks"
    exit 1
  fi
  printf 'no-relink: ok (%d checks)\n' "$checks"
  exit 0
}

# Nothing here can run without a cargo; say which shell supplies one (ADR 0001).
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

# The workspace keeps cargo's default target directory; honour an explicit one
# rather than guess past it.
binary=${CARGO_TARGET_DIR:-$repo/target}/debug/sindri

# fingerprint FILE -- the sha256 of FILE and the stat that moves when cargo
# relinks it, on one line so a difference in either is one comparison.
fingerprint() {
  local hash stat
  hash=$(sha256sum -- "$1") || return 1
  stat=$(stat -c '%y %i' -- "$1") || return 1
  printf 'sha256=%s mtime=%s' "${hash%% *}" "$stat"
}

# build -- one `cargo build`; sets BUILD_SAID to everything it printed so the
# caller can read whether it compiled anything. Returns cargo's own status.
build() {
  BUILD_SAID=$(cargo build --package sindri 2>&1)
}

# compiled_anything -- true when the last build log names a crate it built.
compiled_anything() {
  grep -q '^[[:space:]]*Compiling ' <<<"$BUILD_SAID"
}

# check_same_binary LABEL BEFORE -- build again and hold the binary to BEFORE:
# the same bytes, the same file, and a build log that compiled nothing. The three
# are reported apart, because each is a different way to fail.
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

# 1. The binary as the tree stands, built once so a first run on a cold target
#    directory is not mistaken for a relink.
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

# 2. Build again with nothing touched.
check_same_binary 'an unchanged tree' "$settled"

# 3. Touch a file no compiler ever reads — the glossary — and demand the
#    binary stay exactly where it was. This is the move a build script that
#    watched the whole package, or that stamped the clock, fails.
doc=GLOSSARY.md
if [[ ! -f $doc ]]; then
  bad "there is no $doc to touch, so a non-compiled change cannot be shown"
else
  before_touch=$(fingerprint "$binary")
  touch -- "$doc"
  check_same_binary "a touched $doc" "$before_touch"
fi

# 4. Move the index — one of the few files the provenance script watches — so
#    cargo runs that script again, at a later moment, and see that it says exactly
#    what it said the first time. This is a relink a tree is allowed to have: a
#    watched input moved, so the crate rebuilds; the claim here is about the bytes,
#    not the inode. It is the sharpest thing one machine can say about the clock,
#    because a timestamp of any granularity finer than the day would put different
#    bytes on the disk.
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

# 5. Read the version back out of the binary, and hold it to what git says the
#    checkout is — a source of truth that is not this script and not the crate.
printed=$("$binary" --version 2>&1) || printed=
if [[ -z $printed ]]; then
  bad "$binary --version said nothing"
fi
describe=$(git -C "$repo" describe --tags --always --dirty 2>/dev/null) || describe=
commit_date=$(git -C "$repo" log -1 --format=%cs 2>/dev/null) || commit_date=
if [[ -z $describe || -z $commit_date ]]; then
  # A source tarball: the fallback shape is all there is to hold it to.
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
  # Embedded, not computed at run time: the same bytes are in the binary.
  if grep -qaF -- "$describe ($commit_date)" "$binary"; then
    ok "the binary carries '$describe ($commit_date)' as bytes, not as a lookup"
  else
    bad "the binary does not carry '$describe ($commit_date)' in its bytes"
  fi
  # And never the clock: today's date is not the commit's unless today is the day
  # it was committed, in which case say so rather than pass on a coincidence.
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
