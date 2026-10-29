#!/usr/bin/env bash
# The release scripts are shell, and shell has its own seam: run the script as
# a process and assert on its exit status, its files and its output. Each
# `scripts/tests/*.sh` file declares `test_*` functions; each one runs in a
# fresh temporary directory and a fresh shell, so no test can see another's
# leftovers and no test file can silently shadow another's names.
set -uo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
lib="$root/scripts/tests/lib.sh"
export SINDRI_ROOT="$root"

pass=0
failed=0

# The test names in one file, read by a shell that has loaded that file and
# nothing else. A file that does not parse fails here rather than reporting an
# empty, passing list.
names_in() { # file
    bash -c '. "$1" && declare -F | awk "{print \$3}" | grep "^test_" | sort' _ "$1"
}

for file in "$root"/scripts/tests/*.sh; do
    [ "$file" = "$lib" ] && continue
    if ! names=$(names_in "$file"); then
        printf 'FAIL %s (does not parse)\n' "$(basename "$file")"
        failed=$((failed + 1))
        continue
    fi
    if [ -z "$names" ]; then
        printf 'FAIL %s (declares no test_* function)\n' "$(basename "$file")"
        failed=$((failed + 1))
        continue
    fi
    for name in $names; do
        tmp="$(mktemp -d "${TMPDIR:-/tmp}/sindri-scripts.XXXXXX")"
        if bash -c 'set -euo pipefail; . "$1"; . "$2"; cd "$3"; "$4"' \
            _ "$lib" "$file" "$tmp" "$name"; then
            printf 'ok   %s\n' "${name#test_}"
            pass=$((pass + 1))
        else
            printf 'FAIL %s\n' "${name#test_}"
            failed=$((failed + 1))
        fi
        rm -rf "$tmp"
    done
done

printf 'scripts: %d passed, %d failed\n' "$pass" "$failed"
[ "$failed" -eq 0 ]
