#!/usr/bin/env bash
# The comment gate of `make check` (ADR-0018, ticket #72): a `//` comment in a
# `.rs` file carries what the code cannot show, and carries neither a citation
# nor a paragraph.
#
#   citation      no `//` comment names a spec section (`§16`), a decision
#                 (`ADR-0011`, `ADR 0011`) or an issue (`#72`, `ticket 71`). What
#                 replaces one is nothing: the capability is named by the test
#                 that proves it, and git carries the rest.
#   block-length  a run of consecutive `//` lines is at most three, blank `//`
#                 lines in the run included, because a fourth line is the
#                 paragraph that argues a design and the argument is the pull
#                 request's. A run past the bar is reported at its first line.
#
# `///` and `//!` are out of scope: rustdoc is delivered by `cargo doc`, so a doc
# comment answers what a caller must know, which is a different and larger
# question than whether an internal note is worth its lines.
#
#     bash scripts/comments-probe.sh [REPO_ROOT]     (default: the script's parent)
#
# The verdict line is the last thing printed, and the exit code is part of the
# contract:
#
#   0   comments gate: ok (N files, M // comments)   the tree the cleanup batches
#                                                    left, and nothing since
#   77  comments gate: skipped (awk unavailable)     this machine cannot read the
#                                                    tree; the reason is the line
#                                                    above, and `make check`
#                                                    relays it as SKIPPED, never
#                                                    as the ok a gate that ran has
#                                                    earned
#   1   comments gate: FAIL (N violations)           a comment cites, or a block
#                                                    runs past three lines
#
# HOW IT IS READ, so nobody has to guess. A `//` comment is a line whose first
# characters are `//` but not `///` or `//!`, or the text after the first `//` on
# a line of code. A `//` preceded by a colon is not a comment: `https://` is a URL
# scheme inside a string, and this tree's fixtures are full of them. That is the
# whole of the string handling, so a string literal followed by `//` further
# along the line reads as a comment — no line in this tree does that. Not read at
# all: a `/* */` block comment, and a `//` on a line inside a string that spans
# several. One violation per comment: the first citation it carries is the one
# named.
set -uo pipefail

SKIPPED_STATUS=77
US=$'\x1f'
# The three lines ADR-0018 allows a kept comment block, and the number the
# violation is reported with.
MAX_BLOCK=3

# Nothing is borrowed from the machine to find the tree or to say the tree
# cannot be read: the two verdicts below turn on what is not on PATH, and a
# `dirname` from it would turn the skip into a crash.
case ${BASH_SOURCE[0]} in
  */*) here=${BASH_SOURCE[0]%/*} ;;
  *) here=. ;;
esac
repo_root=$(cd "${1:-$here/..}" && pwd) || {
  printf 'comments: cannot enter %s\n' "${1:-$here/..}" >&2
  exit 2
}
cd "$repo_root" || exit 2

if ! command -v awk >/dev/null 2>&1; then
  printf 'skip comments: awk not on PATH\n'
  printf 'comments gate: skipped (awk unavailable)\n'
  exit "$SKIPPED_STATUS"
fi

files=()
collect() {
  local path
  while IFS= read -r path; do
    path=${path#./}
    [[ -f $path ]] && files+=("$path")
  done < <("$@")
}
if git rev-parse --git-dir >/dev/null 2>&1; then
  collect git ls-files --cached --others --exclude-standard -- '*.rs'
else
  collect find . \( -name .git -o -name target -o -name .worktrees -o -name node_modules -o -name vendor \) -prune \
    -o -type f -name '*.rs' -print
fi
if ((${#files[@]})); then
  mapfile -t files < <(printf '%s\n' "${files[@]}" | LC_ALL=C sort)
fi

violations=()
comments=0

# With no file to read, awk would read this shell's stdin instead and wait for a
# tree nobody wrote, so the pass is only made when there is one.
if ((${#files[@]})); then
  while IFS=$US read -r kind file line text; do
    case $kind in
      count) comments=$file ;;
      cite | block) violations+=("  $file:$line: $text") ;;
    esac
  done < <(awk -v US="$US" -v MAX="$MAX_BLOCK" '
    # comment_start LINE -- the column the line’s comment starts at, or 0. A `//`
    # preceded by a colon is a URL scheme inside a string, not a comment.
    function comment_start(line,   i, n, prev) {
      n = length(line)
      for (i = 1; i < n; i++) {
        if (substr(line, i, 2) != "//") continue
        prev = (i > 1) ? substr(line, i - 1, 1) : " "
        if (prev == ":") { i++; continue }
        return i
      }
      return 0
    }

    # flush_block -- one violation when the run that just ended is past the bar.
    # It is reported at the first line of the run, so the reader lands on the head
    # of the paragraph rather than on whatever ended it.
    function flush_block() {
      if (run > MAX)
        print "block" US file US start US "a comment block runs " run " lines, want at most " MAX
      run = 0
    }

    # classify TEXT -- what the comment cites, or nothing. Sets WHAT to the words
    # the violation is reported with and CITED to the citation as it was written.
    function classify(text) {
      WHAT = ""; CITED = ""
      if (match(text, /§[[:space:]]*[0-9]+/)) {
        WHAT = "cites a spec section"; CITED = substr(text, RSTART, RLENGTH)
        gsub(/[[:space:]]+/, "", CITED)
        return
      }
      if (match(text, /ADR[- ]?[0-9]+/)) {
        WHAT = "cites an ADR"; CITED = substr(text, RSTART, RLENGTH)
        return
      }
      if (match(text, /[Tt]icket[[:space:]]*#?[[:space:]]*[0-9]+/)) {
        WHAT = "cites an issue number"; CITED = substr(text, RSTART, RLENGTH)
        return
      }
      if (match(text, /#[0-9]+/)) {
        WHAT = "cites an issue number"; CITED = substr(text, RSTART, RLENGTH)
      }
    }

    FNR == 1 { flush_block() }

    {
      sub(/\r$/, "")
      if ($0 ~ /^[[:space:]]*\/\//) {
        if ($0 ~ /^[[:space:]]*\/\/[\/!]/) { flush_block(); next }
        text = $0
        if (!run) { start = FNR; file = FILENAME }
        run++
      } else {
        flush_block()
        col = comment_start($0)
        if (!col) next
        text = substr($0, col)
      }
      sub(/^[[:space:]]*/, "", text)
      sub(/^\/\/+/, "", text)
      classify(text)
      if (WHAT != "") print "cite" US FILENAME US FNR US WHAT " (" CITED ")"
      comments++
    }

    END { flush_block(); print "count" US comments }
  ' "${files[@]}")
fi

# plural N WORD -- WORD, with an `s` unless N is 1, so a verdict never reads
# `1 violations`.
plural() {
  if (($1 == 1)); then printf '%s' "$2"; else printf '%s' "$2s"; fi
}

if ((${#violations[@]})); then
  printf '%s\n' "${violations[@]}"
  printf 'comments gate: FAIL (%s %s)\n' "${#violations[@]}" "$(plural "${#violations[@]}" violation)"
  exit 1
fi
printf 'comments gate: ok (%s %s, %s // %s)\n' \
  "${#files[@]}" "$(plural "${#files[@]}" file)" "$comments" "$(plural "$comments" comment)"
