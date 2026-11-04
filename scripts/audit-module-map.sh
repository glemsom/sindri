#!/usr/bin/env bash
# The map: a crate root declares its modules and says, on the same line, what
# each one is for — a reader holding a narrow slice starts the search there, and
# a bare column of declarations sends them opening files until one matches.
# The clause is cut from the module's own `//!` paragraph and never written
# twice: a second description is the one that drifts, so the audit reads that
# paragraph and holds the clause to words it carries, in the order it carries
# them. A clause no paragraph holds is reported with the word that was not found.
#
# It walks the crate roots and nothing else: only a `crates/*/src/lib.rs`, because a
# module below the root is found by opening the root.
set -euo pipefail

# The tree it walks, from the one place the four audits agree on it.
. "${BASH_SOURCE[0]%/*}/tree-listing.sh"

min_words=3
failed=0
checked=0

# One declaration in a crate root: a visibility, the name, and the clause that
# trails it. Read from a variable, because `)` in the bracket expression is a
# parse error when the pattern sits inline in `[[`.
declaration='^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?mod[[:space:]]+([a-z_0-9]+);[[:space:]]*(//(.*))?$'

normalize() { # text -> one word per line, lowercase, punctuation dropped
    tr '[:upper:]' '[:lower:]' | tr -cs '[:alnum:]' '\n' | sed '/^$/d'
}

paragraph() { # file -> the words of the `//!` paragraph it opens with
    awk '
        /^\/\/!/ { $0 = substr($0, 4); print; next }
        { exit }
    ' "$1"
}

missing_word() { # module file, then the clause's words
    local file=$1
    shift
    local -a words=()
    mapfile -t words < <(paragraph "$file" | normalize)
    local at=0 word
    for word in "$@"; do
        while [ "$at" -lt "${#words[@]}" ] && [ "${words[$at]}" != "$word" ]; do
            at=$((at + 1))
        done
        if [ "$at" -ge "${#words[@]}" ]; then
            printf '%s' "$word"
            return 1
        fi
        at=$((at + 1))
    done
}

listing=$(tree_listing '^crates/[^/]+/src/lib\.rs$')

while IFS= read -r root; do
    [ -f "$root" ] || continue
    dir=$(dirname "$root")
    line_number=0
    while IFS= read -r line; do
        line_number=$((line_number + 1))
        if [[ ! $line =~ $declaration ]]; then
            continue
        fi
        name=${BASH_REMATCH[3]}
        clause=$(printf '%s' "${BASH_REMATCH[5]:-}" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')
        checked=$((checked + 1))
        if [ -z "$clause" ]; then
            printf 'audit: %s:%d places %s with no clause\n' "$root" "$line_number" "$name" >&2
            failed=1
            continue
        fi
        module_file=''
        if [ -f "$dir/$name.rs" ]; then
            module_file="$dir/$name.rs"
        elif [ -f "$dir/$name/mod.rs" ]; then
            module_file="$dir/$name/mod.rs"
        fi
        if [ -z "$module_file" ]; then
            printf 'audit: %s:%d declares %s, which has no file\n' "$root" "$line_number" "$name" >&2
            failed=1
            continue
        fi
        mapfile -t clause_words < <(printf '%s\n' "$clause" | normalize)
        if [ "${#clause_words[@]}" -lt "$min_words" ]; then
            printf 'audit: %s:%d places %s with a label, not a clause\n' \
                "$root" "$line_number" "$name" >&2
            failed=1
            continue
        fi
        if ! word=$(missing_word "$module_file" "${clause_words[@]}"); then
            printf "audit: %s does not say '%s'\n" "$module_file" "$word" >&2
            failed=1
        fi
    done <"$root"
done <<<"$listing"

if [ "$failed" -ne 0 ]; then
    echo "audit: trail each declaration with a clause cut from the module's own paragraph" >&2
    exit 1
fi
echo "audit: $checked declarations, every module is placed where the search starts"
