#!/usr/bin/env bash
# Rust modules stay at or under 800 lines. Localising a change should not mean
# reading the world, and a file that outgrows the limit is split — there is no
# allowlist to reach for. Shell scripts and docs are outside it: they are read
# whole or searched, never navigated, so `GLOSSARY.md` is a stated choice.
#
set -euo pipefail

# The tree it walks, from the one place the four audits agree on it.
. "${BASH_SOURCE[0]%/*}/tree-listing.sh"

limit=800
failed=0
checked=0

listing=$(tree_listing '\.rs$')

while IFS= read -r file; do
    [ -f "$file" ] || continue
    checked=$((checked + 1))
    # `NR` counts lines, where `wc -l` counts newlines and so measures a
    # file whose last line carries no terminator one short.
    lines=$(awk 'END { print NR }' "$file")
    if [ "$lines" -le "$limit" ]; then
        continue
    fi
    echo "audit: $file has $lines lines, over the $limit-line limit" >&2
    failed=1
done <<<"$listing"

if [ "$failed" -ne 0 ]; then
    echo "audit: split the file" >&2
    exit 1
fi
echo "audit: $checked files, all at or under $limit lines"
