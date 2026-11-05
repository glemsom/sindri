#!/usr/bin/env bash
# The map: a crate root declares its modules and says, on the same line, what
# each one is for — a reader holding a narrow slice starts the search there, and
# a bare column of declarations sends them opening files until one matches.
# The clause is cut from the module's own `//!` paragraph and never written
# twice: a second description is the one that drifts, so the audit reads that
# paragraph and holds the clause to words it carries, in the order it carries
# them. A clause no paragraph holds is reported with the word that was not found.
#
# It walks the crate roots for that clause and nothing else: only a
# `crates/*/src/lib.rs`, because a module below the root is found by opening the
# root.
#
# The map reads the other way too, and nothing else does. A declaration is what
# compiles a file, so a `.rs` file under a crate's tests tree that no declaration
# names is a module nothing opens: not compiled, not linted, not run, its tests
# proving nothing while the name audit still counts them. Cargo names the test
# binaries itself — a `tests/foo.rs` and a `tests/foo/main.rs` are entry points —
# and every other file there is named by a declaration in the module that owns
# it, a `#[path]` attribute included, which is how a test binary reaches the
# harness it shares.
set -euo pipefail

# The tree it walks, from the one place the four audits agree on it.
. "${BASH_SOURCE[0]%/*}/tree-listing.sh"

min_words=3
failed=0
checked=0
undeclared=0
misplaced=0

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

# Every file the declarations in the crates point at, read from all of them at
# once and printed normalized so one file has one name. A `mod` names a file in
# two shapes: a module's own files live in the directory named after it, while a
# `lib.rs`, a `main.rs` and a `mod.rs` hold their children beside themselves. A
# `#[path]` attribute names whatever it points at, read from the directory the
# declaring file's own modules live in, which is how a test binary reaches the
# harness it shares.
declared() { # the files to read -> the paths their declarations name
    awk '
        function dir_of(path,   dir) {
            dir = path
            if (dir !~ /\//) return "."
            sub(/\/[^\/]*$/, "", dir)
            return dir
        }
        # A crate root holds its children beside itself whatever it is
        # called: `lib.rs`, `main.rs`, or a test binary `foo.rs`. Only a
        # module below one takes its files into its own directory.
        function home(path,   stem) {
            if (path ~ /^crates\/[^\/]+\/tests\/[^\/]+\.rs$/ ||
                path ~ /^crates\/[^\/]+\/src\/bin\/[^\/]+\.rs$/) return dir_of(path)
            stem = path
            sub(/^.*\//, "", stem)
            sub(/\.rs$/, "", stem)
            if (stem == "lib" || stem == "main" || stem == "mod") return dir_of(path)
            return dir_of(path) "/" stem
        }
        function join(base, rel,   parts, count, at, out, walk, path) {
            count = split(base "//" rel, parts, "/")
            out = 0
            for (at = 1; at <= count; at++) {
                if (parts[at] == "" || parts[at] == ".") continue
                if (parts[at] == "..") { if (out > 0) out--; continue }
                path[++out] = parts[at]
            }
            walk = ""
            for (at = 1; at <= out; at++) walk = walk (at > 1 ? "/" : "") path[at]
            return walk
        }
        {
            line = $0
            if (match(line, /#[[:space:]]*\[path[[:space:]]*=[[:space:]]*"[^"]*"[[:space:]]*\]/)) {
                attribute = substr(line, RSTART, RLENGTH)
                sub(/^.*=[[:space:]]*"/, "", attribute)
                sub(/".*$/, "", attribute)
                line = substr(line, RSTART + RLENGTH)
            }
            if (line ~ /^([[:space:]]*#\[[^]]*\])*[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?mod[[:space:]]+[A-Za-z_0-9]+[[:space:]]*;/) {
                if (attribute != "") {
                    print join(home(FILENAME), attribute)
                } else {
                    name = line
                    sub(/^.*mod[[:space:]]+/, "", name)
                    sub(/[^A-Za-z_0-9].*$/, "", name)
                    print join(home(FILENAME), name ".rs")
                    print join(home(FILENAME), name "/mod.rs")
                }
                attribute = ""
                next
            }
            # An attribute carries to the declaration below it and no
            # further: a `#[path]` with no `mod` under it names no file.
            if (attribute != "" && line ~ /[^[:space:]]/ && line !~ /^[[:space:]]*(#\[|\/\/)/) {
                attribute = ""
            }
        }
        ' "$@"
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
            misplaced=1
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
            misplaced=1
            continue
        fi
        mapfile -t clause_words < <(printf '%s\n' "$clause" | normalize)
        if [ "${#clause_words[@]}" -lt "$min_words" ]; then
            printf 'audit: %s:%d places %s with a label, not a clause\n' \
                "$root" "$line_number" "$name" >&2
            failed=1
            misplaced=1
            continue
        fi
        if ! word=$(missing_word "$module_file" "${clause_words[@]}"); then
            printf "audit: %s does not say '%s'\n" "$module_file" "$word" >&2
            failed=1
            misplaced=1
        fi
    done <"$root"
done <<<"$listing"

# The files a declaration may sit in, read once: a tree with no Rust in it asks
# no tool for anything, so the listing stays the only call the audit makes.
sources_listing=$(tree_listing '^crates/[^/]+/.+\.rs$')
sources=()
while IFS= read -r file; do
    [ -n "$file" ] && sources+=("$file")
done <<<"$sources_listing"

declare -A named=()
if [ "${#sources[@]}" -gt 0 ]; then
    while IFS= read -r path; do
        [ -n "$path" ] && named[$path]=1
    done <<<"$(declared "${sources[@]}")"
fi

# A test binary is named by Cargo and never by a declaration: `tests/foo.rs` and
# `tests/foo/main.rs` are the two shapes an entry point takes.
test_binary='^crates/[^/]+/tests/([^/]+\.rs|[^/]+/main\.rs)$'

test_files=$(tree_listing '^crates/[^/]+/tests/.+\.rs$')
named_tests=0
while IFS= read -r file; do
    [ -f "$file" ] || continue
    if [[ $file =~ $test_binary ]]; then
        continue
    fi
    named_tests=$((named_tests + 1))
    if [ -z "${named[$file]:-}" ]; then
        printf 'audit: %s is a module nothing declares: declare it in the module that owns it, or delete it\n' \
            "$file" >&2
        undeclared=1
        failed=1
    fi
done <<<"$test_files"

if [ "$failed" -ne 0 ]; then
    if [ "$misplaced" -ne 0 ]; then
        echo "audit: trail each declaration with a clause cut from the module's own paragraph" >&2
    fi
    if [ "$undeclared" -ne 0 ]; then
        echo "audit: declare every module file under a crate's tests tree in the module that owns it" >&2
    fi
    exit 1
fi
echo "audit: $checked declarations, every module is placed where the search starts"
echo "audit: $named_tests test files, every one is named by a declaration"
