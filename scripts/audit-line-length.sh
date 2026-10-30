#!/usr/bin/env bash
# Source files stay at or under 400 lines, with an explicit allowlist.
# Localising a change should not mean reading the world.
set -euo pipefail

limit=400
allowlist="$(dirname "$0")/line-length-allowlist.txt"
failed=0
checked=0

while IFS= read -r file; do
    [ -f "$file" ] || continue
    checked=$((checked + 1))
    lines=$(wc -l <"$file")
    if [ "$lines" -le "$limit" ]; then
        continue
    fi
    if [ -f "$allowlist" ] && grep -qxF "$file" "$allowlist"; then
        continue
    fi
    echo "audit: $file has $lines lines, over the $limit-line limit" >&2
    failed=1
done < <(git ls-files '*.rs')

if [ "$failed" -ne 0 ]; then
    echo "audit: split the file, or add it to scripts/line-length-allowlist.txt with a reason" >&2
    exit 1
fi
echo "audit: $checked files, all at or under $limit lines"
