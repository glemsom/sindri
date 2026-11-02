#!/usr/bin/env bash
# A module opens with `//!`: one paragraph on what it is for, so a reader — or an
# agent holding a narrow slice — can place the file without reading its body.
# The paragraph is held to words, not to shape: `//!` on its own is a placeholder
# that satisfies the scan and says nothing. Inner attributes go after it, because
# the first line is the line that gets scanned; boilerplate there costs exactly
# the read the rule exists to save.
#
# The set it walks is what `git status` sees: an unstaged file is checked
# too, and a gitignored tree stays out.
set -euo pipefail

min_words=8
failed=0
checked=0

# The words in the `//!` paragraph the file opens with: the markers are dropped
# and the wrapped lines are counted together, because a paragraph is the unit the
# rule asks for, not its first line. The run ends where the first non-`//!` line
# begins, and it is read from column 0, which is where the `case` matches.
paragraph_words() { # file
    awk '
        /^\/\/!/ { counted = 1; $0 = substr($0, 4); words += NF; next }
        counted { exit }
        { next }
        END { print words + 0 }
    ' "$1"
}

while IFS= read -r file; do
    [ -f "$file" ] || continue
    checked=$((checked + 1))
    first=$(grep -m1 -v '^[[:space:]]*$' "$file" || true)
    case "$first" in
        '//!'*)
            if [ "$(paragraph_words "$file")" -ge "$min_words" ]; then
                continue
            fi
            what="says //! and $min_words words are not there"
            ;;
        '') what="is empty, not a //! paragraph" ;;
        *) what="opens with \`${first:0:60}\`, not a //! paragraph" ;;
    esac
    printf 'audit: %s %s\n' "$file" "$what" >&2
    failed=1
done < <({ git ls-files; git ls-files --others --exclude-standard; } | grep '\.rs$')

if [ "$failed" -ne 0 ]; then
    echo "audit: open the module with a //! paragraph saying what it is for, above any inner attribute" >&2
    exit 1
fi
echo "audit: $checked files, every module opens with its own paragraph"
