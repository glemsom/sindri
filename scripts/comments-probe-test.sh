#!/usr/bin/env bash
# Acceptance test for scripts/comments-probe.sh, the comment gate of `make check`
# (ADR-0018, ticket #72).
#
# Each case builds a throwaway tree under $TMPDIR holding one thing the gate is
# meant to judge, and asserts both the verdict and the `file:line` the gate
# names. Fixtures live in a temp dir on purpose: the gate reads every `.rs` file
# it is given, so a deliberately-broken fixture inside the repo would be a real
# violation.
#
#     bash scripts/comments-probe-test.sh
set -uo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
probe="$repo_root/scripts/comments-probe.sh"
make_path=$(command -v make)
scratch=$(mktemp -d "${TMPDIR:-/dev/shm}/sindri-comments-probe-test.XXXXXX") || exit 1
trap 'rm -rf "$scratch"' EXIT

passed=0
failed=0
ok() { printf 'ok   %s\n' "$*"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n' "$*"; failed=$((failed + 1)); }

# The one tree that satisfies the gate; each case copies it, adds one file or
# one line, and requires the gate to name it.
clean="$scratch/clean"
mkdir -p "$clean/src"
cat >"$clean/src/lib.rs" <<'RS'
//! The demo crate, whose opening paragraph the gate does not read, because a
//! `//!` is a module's account of itself and not a note on the code below it.

// The loop below must not spin: the caller holds a lock across every turn.
pub fn sample(held: bool) -> bool {
    held
}
RS

run() { # TREE -- captures OUT and CODE from the gate alone
  OUT=$(bash "$probe" "$1" </dev/null 2>&1)
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

# break_case NAME LINE... -- copies the clean tree to $scratch/NAME and appends
# every LINE to its lib.rs. Copies rather than edits, so no case can leak into the
# next one.
break_case() {
  local name=$1
  shift
  cp -r "$clean" "$scratch/$name"
  printf '%s\n' "$@" >>"$scratch/$name/src/lib.rs"
}

# 1. The tree the earlier batches cleaned: nothing to report.
run "$clean"
assert_code 0 "clean tree"
assert_has 'comments gate: ok (' "clean tree"
refute 'comments gate: FAIL' "clean tree"

# 2. A `//` comment naming a spec section. The line named is the one the comment
#    sits on, so a reader is sent to the citation itself.
break_case spec '// §16 makes this the definition of done, which is why it is here.'
run "$scratch/spec"
assert_code 1 "a spec section in a // comment"
assert_line '  src/lib.rs:8: cites a spec section (§16)' "a spec section in a // comment"
assert_line 'comments gate: FAIL (1 violation)' "a spec section in a // comment"

# 3. An ADR, and an issue number, in the spellings this tree writes them.
break_case adr '// The mask is decided here, as ADR-0011 asks.'
run "$scratch/adr"
assert_code 1 "an ADR in a // comment"
assert_line '  src/lib.rs:8: cites an ADR (ADR-0011)' "an ADR in a // comment"

break_case issue '// The `open` tool landed once this was written (#34).'
run "$scratch/issue"
assert_code 1 "an issue number in a // comment"
assert_line '  src/lib.rs:8: cites an issue number (#34)' "an issue number in a // comment"

break_case ticket '// ticket 71 emptied the test trees of this kind of line.'
run "$scratch/ticket"
assert_code 1 "a bare ticket number in a // comment"
assert_line '  src/lib.rs:8: cites an issue number (ticket 71)' "a bare ticket number in a // comment"

# 4. `///` and `//!` are out of scope: rustdoc answers a caller's question, and
#    ADR-0018 leaves it for a pass of its own.
break_case doc '/// Not here: §16, ADR-0018 and #72 all stay in a `///` comment.'
run "$scratch/doc"
assert_code 0 "a citation in a /// comment"
refute 'comments gate: FAIL' "a citation in a /// comment"

# 5. A tree with no Rust in it reads nothing and cites nothing: the gate says so
#    rather than hand awk no file at all and wait on the shell's stdin.
empty="$scratch/empty"
mkdir -p "$empty"
run "$empty"
assert_code 0 "a tree with no Rust in it"
assert_line 'comments gate: ok (0 files, 0 // comments)' "a tree with no Rust in it"

# 6. A comment block longer than the three lines ADR-0018 allows, blank `//`
#    lines in the run included: the paragraph that argues a design comes back
#    one line at a time, so the count starts at its first line.
break_case block4 '// One: the lock is taken before the turn.' \
  '// Two: the caller holds it across every write.' \
  '// Three: nothing else may take it in the meantime.' \
  '// Four: which is why the signature carries it.'
run "$scratch/block4"
assert_code 1 "a block of four // lines"
assert_line '  src/lib.rs:8: a comment block runs 4 lines, want at most 3' "a block of four // lines"
assert_line 'comments gate: FAIL (1 violation)' "a block of four // lines"

# 7. Three lines is the bar, and a blank line splits a run in two: neither is a
#    paragraph, so neither is reported.
break_case block3 '// One: the lock is taken before the turn.' \
  '//' \
  '// Three: which is why the signature carries it.'
run "$scratch/block3"
assert_code 0 "a block of three // lines, one of them blank"
refute 'comments gate: FAIL' "a block of three // lines, one of them blank"

# A blank `//` line is inside the run, not a break in it, so a fourth line under
# one is a paragraph the same way.
break_case block4_blank '// One: the lock is taken before the turn.' \
  '// Two: the caller holds it across every write.' \
  '//' \
  '// Four: which is why the signature carries it.'
run "$scratch/block4_blank"
assert_code 1 "a blank // line inside a block of four"
assert_line '  src/lib.rs:8: a comment block runs 4 lines, want at most 3' "a blank // line inside a block of four"

break_case split '// One: the lock is taken before the turn.' \
  '// Two: the caller holds it across every write.' \
  '' \
  '// Three: nothing else may take it in the meantime.' \
  '// Four: which is why the signature carries it.'
run "$scratch/split"
assert_code 0 "two blocks split by a blank line"
refute 'comments gate: FAIL' "two blocks split by a blank line"

# 8. A run of trailing comments is not a block: each is the clause of the line
#    it sits on, and the mod-clause convention writes them that way.
break_case trailing_block 'pub mod a; // the first clause' \
  'pub mod b; // the second clause' \
  'pub mod c; // the third clause' \
  'pub mod d; // the fourth clause'
run "$scratch/trailing_block"
assert_code 0 "four trailing // comments in a row"
refute 'comments gate: FAIL' "four trailing // comments in a row"

# 9. A trailing comment is a `//` comment too, so a citation in one is reported
#    where it sits.
break_case trailing_cite 'pub fn tailed() -> u8 { 1 } // §16 says this is done.'
run "$scratch/trailing_cite"
assert_code 1 "a citation in a trailing // comment"
assert_line '  src/lib.rs:8: cites a spec section (§16)' "a citation in a trailing // comment"

# 10. A `//` inside a string literal is code, not a comment: this tree's fixtures
#     carry URLs with fragments and Rust that strips a `//` prefix, and a gate that
#     read either as a comment would fail on the tree the batches cleaned.
break_case url 'const DOC: &str = "https://example.test/spec#12";'
run "$scratch/url"
assert_code 0 "a URL with a fragment in a string"
refute 'comments gate: FAIL' "a URL with a fragment in a string"

break_case strip 'let authority = match rest.strip_prefix("//") {'
run "$scratch/strip"
assert_code 0 "a // inside a string literal"
refute 'comments gate: FAIL' "a // inside a string literal"

break_case hidden 'let note = "// §16 and ADR-0018 are in this string, not a comment";'
run "$scratch/hidden"
assert_code 0 "a citation inside a string literal"
refute 'comments gate: FAIL' "a citation inside a string literal"

# A `#` that runs on into a word is a colour, not an issue number: a gate that
# read `#1a2b3c` as #1 would fail a comment on a palette.
break_case colour '// The chosen ink is #1a2b3c, and nothing here cites anything.'
run "$scratch/colour"
assert_code 0 "a hex colour in a // comment"
refute 'comments gate: FAIL' "a hex colour in a // comment"

# 11. A machine that cannot read the tree says so instead of earning the green
#     line a gate that ran has earned — and it says which tool is missing, so the
#     gap is fixable rather than mysterious. Each case is handed a PATH holding
#     the shell and the tools named for it, and nothing else.
machine() { # NAME TOOL... -- a PATH holding a shell and every TOOL, symlinked
  local name=$1 tool
  shift
  mkdir -p "$scratch/machine-$name"
  ln -s "$(command -v bash)" "$scratch/machine-$name/bash"
  for tool in "$@"; do ln -s "$(command -v "$tool")" "$scratch/machine-$name/$tool"; done
  printf '%s' "$scratch/machine-$name"
}

none=$(machine none)
OUT=$(env PATH="$none" "$none/bash" "$probe" "$clean" </dev/null 2>&1)
CODE=$?
assert_code 77 "no awk"
assert_line 'skip comments: awk not on PATH' "no awk"
assert_line 'comments gate: skipped (awk unavailable)' "no awk"
refute 'comments gate: ok' "no awk, no green line over a tree nothing read"
# The skip is the verdict, not a crash: a machine this bare has no `dirname` or
# `git` either, so the probe may borrow nothing from the machine to say it.
refute 'command not found' "no awk, no borrowed tool on PATH"

# A tool missing further down the pass is a skip too: a machine that holds awk and
# nothing else would otherwise walk no file and call the empty list a clean tree.
for tool in sort find; do
  if [[ $tool == find ]]; then
    path=$(machine "no-$tool" awk sort)
  else
    path=$(machine "no-$tool" awk)
  fi
  OUT=$(env PATH="$path" "$path/bash" "$probe" "$clean" </dev/null 2>&1)
  CODE=$?
  assert_code 77 "no $tool"
  assert_line "skip comments: $tool not on PATH" "no $tool"
  assert_line "comments gate: skipped ($tool unavailable)" "no $tool"
  refute 'comments gate: ok' "no $tool, no green line over a tree nothing read"
done

# 12. The stage line: the verdicts relayed the way every neighbouring stage
#     relays them, and only the skip keeps exit 0 with the gate unrun.
run_make() { # DIR PATH -- captures OUT and CODE from `make comments-tier`
  OUT=$(env PATH="$2" "$make_path" --no-print-directory -C "$1" \
    -f "$repo_root/Makefile" comments-tier 2>&1)
  CODE=$?
}

run_make "$repo_root" "$PATH"
assert_code 0 "make comments-tier, this tree"
assert_line 'check: comments ... ok' "make comments-tier, this tree"

# The relay reads the verdict from the exit code, so a skipped gate must never
# borrow the green line: `make` is on this machine and only the gate is missing,
# which is the shape a runner without awk has.
run_make "$repo_root" "$none"
assert_code 0 "make comments-tier, no awk"
assert_line 'check: comments ... SKIPPED' "make comments-tier, no awk"
assert_line 'comments gate: skipped (awk unavailable)' "make comments-tier, no awk"
refute 'check: comments ... ok' "make comments-tier, no awk"

# 13. The stage fails a build over a tree that cites, printing `check: comments
#     ... FAIL` and exiting non-zero. It is this repo's own Makefile and this
#     repo's own probe, run with the fixture as the working directory, so what is
#     proved is the wiring `make check` runs — not a copy of it that could drift
#     from the file. The probe finds the tree it is given by its working
#     directory, which is why the symlink below is enough.
dirty="$scratch/dirty"
mkdir -p "$dirty/src"
printf '// The mask is decided here, as ADR-0011 asks.\n' >"$dirty/src/lib.rs"
ln -s "$repo_root/scripts" "$dirty/scripts"
run_make "$dirty" "$PATH"
assert_code 2 "make comments-tier, a tree that cites"
assert_line 'check: comments ... FAIL' "make comments-tier, a tree that cites"
refute 'check: comments ... ok' "make comments-tier, a tree that cites"

# 14. The default really is to run the gate: `make check` lists it among its
#     stages, so nothing but a machine with no awk can turn it off. Read from
#     make itself, so a stage added to STAGES but never wired, or wired but never
#     listed, is caught either way.
OUT=$("$make_path" --no-print-directory -C "$repo_root" -n -p 2>/dev/null | sed -n 's/^STAGES := //p' | head -1)
if [[ $OUT == *comments* ]]; then
  ok "make check: STAGES lists comments"
else
  bad "make check: STAGES is '$OUT', want comments among them"
fi
OUT=$(cd "$repo_root" && "$make_path" --no-print-directory -n check 2>&1)
if grep -q 'scripts/comments-probe.sh' <<<"$OUT"; then
  ok "make check: the loop reaches the comments stage"
else
  bad "make check: the loop never reaches the comments stage"; printf '  output:\n%s\n' "$OUT"
fi

printf '\n%s passed, %s failed\n' "$passed" "$failed"
(( failed == 0 ))
