#!/usr/bin/env bash
# Rust modules stay at or under 800 lines. Localising a change should not mean
# reading the world, and a file that outgrows the limit is split — there is no
# allowlist to reach for. Shell scripts and docs are outside it: they are read
# whole or searched, never navigated, so `GLOSSARY.md` is a stated choice.
set -euo pipefail

limit=800
failed=0
checked=0

while IFS= read -r file; do
    [ -f "$file" ] || continue
    checked=$((checked + 1))
    lines=$(wc -l <"$file")
    if [ "$lines" -le "$limit" ]; then
        continue
    fi
    echo "audit: $file has $lines lines, over the $limit-line limit" >&2
    failed=1
done < <(git ls-files '*.rs')

if [ "$failed" -ne 0 ]; then
    echo "audit: split the file" >&2
    exit 1
fi
echo "audit: $checked files, all at or under $limit lines"
