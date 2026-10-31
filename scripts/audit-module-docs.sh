#!/usr/bin/env bash
# A module opens with `//!`: one paragraph on what it is for, so a reader — or an
# agent holding a narrow slice — can place the file without reading its body.
# Inner attributes go after it, because the first line is the line that gets
# scanned; boilerplate there costs exactly the read the rule exists to save.
set -euo pipefail

failed=0
checked=0

while IFS= read -r file; do
    [ -f "$file" ] || continue
    checked=$((checked + 1))
    first=$(grep -m1 -v '^[[:space:]]*$' "$file" || true)
    case "$first" in
        '//!'*) continue ;;
        '') what="is empty" ;;
        *) what="opens with \`${first:0:60}\`" ;;
    esac
    printf 'audit: %s %s, not a //! paragraph\n' "$file" "$what" >&2
    failed=1
done < <(git ls-files '*.rs')

if [ "$failed" -ne 0 ]; then
    echo "audit: open the module with a //! paragraph saying what it is for, above any inner attribute" >&2
    exit 1
fi
echo "audit: $checked files, every module opens with its own paragraph"
