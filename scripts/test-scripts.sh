#!/usr/bin/env bash
# The release scripts are shell, and shell has its own seam: run the script as
# a process and assert on its exit status, its files and its output. Each
# `scripts/tests/*.sh` file declares `test_*` functions; each runs in a fresh
# temporary directory, so no test can see another's leftovers.
set -uo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
export SINDRI_ROOT="$root"
# shellcheck source=/dev/null
. "$root/scripts/tests/lib.sh"

pass=0
failed=0

for file in "$root"/scripts/tests/*.sh; do
    before="$(declare -F | awk '{print $3}' | sort)"
    # shellcheck source=/dev/null
    . "$file"
    for name in $(comm -13 <(printf '%s\n' "$before") <(declare -F | awk '{print $3}' | sort)); do
        case "$name" in
            test_*) ;;
            *) continue ;;
        esac
        tmp="$(mktemp -d "${TMPDIR:-/tmp}/sindri-scripts.XXXXXX")"
        if (cd "$tmp" && "$name"); then
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
