#!/usr/bin/env bash
# A test's name states the behaviour it proves, because the name is the spec
# written down: it is what a failing run prints and what a reader searches for.
# A name under four words states nothing, and a `test_`, `it_`, `works` or
# `check_` first word names the fact that it is a test — which the `#[test]`
# above the function has already said. The fix is to sentence the case:
# `sentence_name_the_case`, not to shorten the sentence to fit the function.
#
# A test is read as the `#[test]` / `#[tokio::test]` attribute and the item it
# annotates, wherever rustfmt put them: the attribute may stand on its own line,
# share one with the `fn`, or have blank lines, comments and further attributes
# below it. Every one of those gaps is stepped over, because a test the scan
# loses is a name nothing reports.
#
set -euo pipefail

# The tree it walks, from the one place the four audits agree on it.
. "${BASH_SOURCE[0]%/*}/tree-listing.sh"

min_words=4
failed=0
files=0
tests=0

# The tests in one file as `name:words`, in the order they are declared.
test_names() { # file
    awk '
        # The attribute, and anything the `fn` below may carry: a visibility, an
        # `async`, a `const`. A `fn` that is none of these is not the item the
        # attribute annotated, so the scan gives up on the line rather than
        # pairing the next `fn` in the file with a stale `#[test]`.
        /^[[:space:]]*#\[(tokio::)?test(\]|\()/ {
            annotated = 1
            sub(/^[[:space:]]*#\[(tokio::)?test[^]]*\][[:space:]]*/, "", $0)
            if ($0 == "") next
        }
        annotated && /^[[:space:]]*(#\[|\/\/|$)/ { next }
        annotated && /^[[:space:]]*([a-z]+(\([a-z:]+\))?[[:space:]]+)*fn[[:space:]]+[A-Za-z0-9_]/ {
            name = $0
            sub(/^.*fn[[:space:]]+/, "", name)
            sub(/[^A-Za-z0-9_].*$/, "", name)
            # Words are runs of letters, not fields between underscores: a run of
            # them is still one word, and `_` alone is no word at all.
            words = 0
            rest = name
            while (match(rest, /[A-Za-z0-9]+/)) {
                words++
                rest = substr(rest, RSTART + RLENGTH)
            }
            print name ":" words
            annotated = 0
            next
        }
        { annotated = 0 }
    ' "$1"
}

listing=$(tree_listing '\.rs$')

while IFS= read -r file; do
    [ -f "$file" ] || continue
    files=$((files + 1))
    while IFS= read -r entry; do
        [ -n "$entry" ] || continue
        tests=$((tests + 1))
        name=${entry%:*}
        words=${entry##*:}
        case "${name%%_*}" in
            test | it | works | check)
                what="opens with \`${name%%_*}\`, which the #[test] above it already said"
                ;;
            *)
                [ "$words" -lt "$min_words" ] || continue
                what="is $words words long and states no behaviour"
                ;;
        esac
        printf 'audit: %s `%s` %s\n' "$file" "$name" "$what" >&2
        failed=1
    done < <(test_names "$file")
done <<<"$listing"

if [ "$failed" -ne 0 ]; then
    echo "audit: name the behaviour the test proves rather than the function, so the fix reads as sentence_name_the_case" >&2
    exit 1
fi
echo "audit: $files files, $tests tests, every name states the behaviour it proves"
