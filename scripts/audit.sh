#!/usr/bin/env bash
# The fast audits, in one pass over the tree: no cargo, no compilation, no
# network. One line per rule, always, and a non-zero exit if any rule failed.
#
#     bash scripts/audit.sh [REPO_ROOT]     (default: the script's parent)
#
# A file directly under a tests tree is a cargo target and needs no declaration;
# a nested one must be named by a `mod` that reaches it, resolved the way rustc
# does. The walk starts at every target and stops where declarations do.
#
# Out of scope: `#[path]` and `#[cfg]` are ignored, one `mod` per line is
# assumed, and `test-name` reads `#[test]` rather than `#[tokio::test]`.
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "${1:-$here/..}" && pwd) || {
  printf 'audit: cannot enter %s\n' "${1:-$here/..}" >&2
  exit 2
}
cd "$repo_root" || exit 2

# Every Rust file in the tree, relative and in a stable order so two runs agree.
# A git tree is read through git — tracked plus untracked-but-not-ignored, which
# skips a build or a devbox shell; anything else is walked directly.
files=()
collect() {
  local path
  while IFS= read -r path; do
    path=${path#./}
    [[ -f $path ]] && files+=("$path")
  done < <("$@")
}
if git rev-parse --git-dir >/dev/null 2>&1; then
  collect git ls-files --cached --others --exclude-standard -- '*.rs'
else
  collect find . \( -name .git -o -name target -o -name .worktrees -o -name node_modules -o -name vendor \) -prune \
    -o -type f -name '*.rs' -print
fi
if ((${#files[@]})); then
  mapfile -t files < <(printf '%s\n' "${files[@]}" | LC_ALL=C sort)
fi

status=0

# report RULE VIOLATION... -- one line for the rule, always; the violations it
# found are listed beneath it and never reach into another rule's line.
report() {
  local rule=$1
  shift
  if (($# == 0)); then
    printf 'audit: %s ... ok\n' "$rule"
    return
  fi
  printf 'audit: %s ... FAIL\n' "$rule"
  printf '  %s\n' "$@"
  status=1
}

# is_crate_root FILE -- true when cargo compiles FILE as a crate target: a
# lib.rs or main.rs, or a file directly under a tests, benches or examples tree.
is_crate_root() {
  local file=$1 dir=${1%/*}
  [[ $dir == "$file" ]] && dir=.
  case ${file##*/} in
    lib.rs | main.rs) return 0 ;;
  esac
  case $dir in
    tests | */tests | benches | */benches | examples | */examples) return 0 ;;
  esac
  return 1
}

# owns_directory FILE -- true when FILE's child modules live in the directory
# FILE sits in rather than in a directory named after it, which is Rust's rule
# for crate roots and for mod.rs.
owns_directory() {
  is_crate_root "$1" && return 0
  [[ ${1##*/} == mod.rs ]]
}

# resolve_mod FILE NAME -- sets RESOLVED to the file `mod NAME;` in FILE names,
# or to nothing when no such file exists.
resolve_mod() {
  local file=$1 name=$2 dir
  if owns_directory "$file"; then dir=${file%/*}; else dir=${file%.rs}; fi
  [[ $dir == "$file" || -z $dir ]] && dir=.
  RESOLVED=
  if [[ -f $dir/$name.rs ]]; then
    RESOLVED=$dir/$name.rs
  elif [[ -f $dir/$name/mod.rs ]]; then
    RESOLVED=$dir/$name/mod.rs
  fi
}

# Every `mod name;` declaration in the tree, as records of file US line US name.
decl_lines=()
read_declarations() {
  ((${#files[@]})) || return 0
  mapfile -t decl_lines < <(awk -v US=$'\x1f' '
    { sub(/\r$/, "") }
    match($0, /^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?mod[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*;/) {
      decl = substr($0, 1, RLENGTH)
      sub(/;[[:space:]]*$/, "", decl)
      name = parts[split(decl, parts, /[[:space:]]+/)]
      print FILENAME US FNR US name
    }
  ' "${files[@]}")
}

violations_undeclared_test_file=()

# Rule 1. A file under a tests tree that no declaration names is reported where
# it lies; the reading at the top of this file says what reaches it.
audit_undeclared_test_file() {
  local -A children=() reached=()
  local -a queue=()
  local record file line name child dir index

  # The walk's edges: every declaration, resolved to the file it names.
  for record in "${decl_lines[@]}"; do
    IFS=$'\x1f' read -r file line name <<<"$record"
    resolve_mod "$file" "$name"
    if [[ -n $RESOLVED ]]; then children[$file]+="$RESOLVED"$'\n'; fi
  done

  for file in "${files[@]}"; do
    dir=${file%/*}
    if [[ $dir != "$file" && ${dir##*/} == tests ]]; then
      reached[$file]=1
      queue+=("$file")
    fi
  done

  index=0
  while ((index < ${#queue[@]})); do
    file=${queue[index]}
    index=$((index + 1))
    while IFS= read -r child; do
      [[ -z $child || -n ${reached[$child]:-} ]] && continue
      reached[$child]=1
      queue+=("$child")
    done <<<"${children[$file]:-}"
  done

  for file in "${files[@]}"; do
    case $file in
      tests/* | */tests/*) ;;
      *) continue ;;
    esac
    dir=${file%/*}
    [[ ${dir##*/} == tests ]] && continue
    [[ -n ${reached[$file]:-} ]] && continue
    violations_undeclared_test_file+=("$file:1: no mod declaration reaches this file")
  done
}

# Every `#[test]` name in the tree, as records of file US line US name. An
# attribute may sit between the `#[test]` and the `fn`, as a cfg or a
# should_panic does; anything else in between ends the search.
test_lines=()
read_test_names() {
  ((${#files[@]})) || return 0
  mapfile -t test_lines < <(awk -v US=$'\x1f' '
    { sub(/\r$/, "") }
    /^[[:space:]]*#\[test\]/ { pending = 1; next }
    pending && /^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?(async[[:space:]]+)?fn[[:space:]]+[A-Za-z_]/ {
      name = $0
      sub(/^.*fn[[:space:]]+/, "", name)
      sub(/[^A-Za-z0-9_].*$/, "", name)
      print FILENAME US FNR US name
      pending = 0
      next
    }
    pending && $0 !~ /^[[:space:]]*#\[/ { pending = 0 }
  ' "${files[@]}")
}

violations_test_name=()

# The opening word of a name, when it is one the rule bans: `test`, `it`, `works`
# or `check`, bare or trailed by an underscore.
test_opener() {
  local name=$1 opener
  for opener in test it works check; do
    if [[ $name == "$opener" || $name == "$opener"_* ]]; then
      printf '%s' "$opener"
      return 0
    fi
  done
  return 1
}

# Rule 2. Every `#[test]` name states the behaviour.
audit_test_name() {
  local record file line name opener
  for record in "${test_lines[@]}"; do
    IFS=$'\x1f' read -r file line name <<<"$record"
    if opener=$(test_opener "$name"); then
      violations_test_name+=("$file:$line: test '$name' opens with '$opener' instead of naming the behaviour")
    fi
  done
}

read_declarations
audit_undeclared_test_file
report undeclared-test-file "${violations_undeclared_test_file[@]}"
read_test_names
audit_test_name
report test-name "${violations_test_name[@]}"

exit "$status"
