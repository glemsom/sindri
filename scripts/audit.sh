#!/usr/bin/env bash
# The four fast audits of spec §16, in one pass over the tree:
# no cargo, no compilation, no network, milliseconds on the whole workspace.
# Ticket #5 wires this into `make audit`; it is the entry point for that target
# and for the commit hook, so it takes the repo root and prints one line per
# rule, always all four, and exits non-zero if any rule found a violation.
#
#     bash scripts/audit.sh [REPO_ROOT]     (default: the script's parent)
#
# The four rules, each named in the output line it prints:
#
#   module-paragraph      every module opens with a `//!` paragraph of at least
#                         eight words. The paragraph is the first run of `//!`
#                         lines that carries text; the eight words are counted
#                         after whitespace is collapsed.
#   mod-clause            every `mod name;` in a crate root is trailed by `//`
#                         and one clause cut from that module's paragraph — the
#                         clause must appear in the paragraph verbatim, case
#                         aside.
#   undeclared-test-file  a file under a tests tree that no declaration names.
#   test-name             every `#[test]` name states the behaviour: no
#                         `test`, `it`, `works` or `check` opener, where an
#                         opener is the whole first word of the name.
#
# HOW `undeclared-test-file` IS READ. It is the reverse of `mod-clause`, and it
# exists because cargo silently forgets files a declaration never names. Cargo
# auto-discovers every file sitting directly under `tests/` as its own test
# target, so those files need no declaration and are never reported. A file
# *nested* under a tests tree — `crates/core/tests/common/helper.rs` — is not a
# target; it is compiled only if some declaration reaches it, so the rule is: a
# nested file must be named by a `mod` declaration in a file that reaches it,
# where "reaches" is Rust's own resolution (a `mod x;` in `tests/a.rs` looks in
# `tests/x.rs` and `tests/x/mod.rs`; in `tests/common/mod.rs` it looks beside
# that file; in a non-`mod.rs` module it looks in the directory named after the
# file) walked outward from every file directly under `tests/`. One reading,
# applied to every tests tree in the tree.
#
# Deliberately out of scope, so a reader knows what this does not catch: `#[path]`
# attributes and `#[cfg]` are ignored, one `mod` declaration per line is assumed,
# block comments are not read as clauses, and `test-name` reads `#[test]` itself
# rather than `#[tokio::test]` and its kin. Nothing else is exempt: the file list
# is whatever git holds plus what the tree has not ignored, so a build or a
# devbox shell leaves nothing to skip by hand — which is why this script's own
# fixtures are built in a temp dir, where the audit never looks.
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "${1:-$here/..}" && pwd) || {
  printf 'audit: cannot enter %s\n' "${1:-$here/..}" >&2
  exit 2
}
cd "$repo_root" || exit 2

# Every Rust file in the tree, relative and in a stable order so two runs agree.
# A git tree is read through git, which is also the fastest way to skip what a
# build or a devbox shell leaves behind: tracked files plus untracked files the
# tree does not ignore. A tree that is not a git checkout — the audit's own
# fixtures — is walked directly.
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

# Collapse runs of whitespace and trim, so a clause and a paragraph compare as
# the words a reader sees rather than as the lines they were wrapped on.
normalize() {
  local out= word
  local IFS=$' \t\n'
  set -f
  for word in $1; do out+=" $word"; done
  set +f
  printf '%s' "${out# }"
}

# read_paragraph FILE -- sets PARA to the file's opening `//!` paragraph,
# normalised, and PARA_LINE to the line it starts on. Returns non-zero, with
# PARA_LINE naming the first line that is not part of one, when the file does
# not open with a paragraph.
read_paragraph() {
  local file=$1 line body lineno=0 started=0 text=
  PARA= PARA_LINE=1
  while IFS= read -r line || [[ -n $line ]]; do
    lineno=$((lineno + 1))
    line=${line%$'\r'}
    if [[ $line =~ ^[[:space:]]*//! ]]; then
      body=${line#*//!}
      body=${body# }
      if [[ -z ${body//[[:space:]]/} ]]; then
        if ((started)); then
          PARA_LINE=$((lineno - 1)) PARA=$(normalize "$text")
          return 0
        fi
        continue
      fi
      if ((!started)); then started=1 PARA_LINE=$lineno; fi
      text+=" $body"
    else
      if ((started)); then
        PARA=$(normalize "$text")
        return 0
      fi
      [[ -z ${line//[[:space:]]/} ]] && continue
      PARA_LINE=$lineno
      return 1
    fi
  done <"$file"
  if ((started)); then
    PARA=$(normalize "$text")
    return 0
  fi
  return 1
}

# word_count TEXT -- prints how many whitespace-separated words TEXT holds.
word_count() {
  local -a words=()
  read -r -a words <<<"$1"
  printf '%s' "${#words[@]}"
}

status=0

# report RULE VIOLATION... -- one line for the rule, always; the violations it
# found are listed beneath it and never reach into another rule's line, so four
# violations are four independent failures.
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

violations_module_paragraph=()

# Rule 1. Every module opens with a `//!` paragraph of at least eight words.
audit_module_paragraph() {
  local file words
  for file in "${files[@]}"; do
    if ! read_paragraph "$file"; then
      violations_module_paragraph+=("$file:$PARA_LINE: does not open with a //! paragraph")
      continue
    fi
    words=$(word_count "$PARA")
    if ((words < 8)); then
      violations_module_paragraph+=("$file:$PARA_LINE: //! paragraph is $words words, want at least 8")
    fi
  done
}

# is_crate_root FILE -- true when cargo compiles FILE as a crate target: a
# lib.rs or main.rs, or a file directly under a tests, benches or examples
# directory (each of those is a crate root of its own).
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

# Every `mod name;` declaration in the tree, as records of
# file US line US name US clause, where the clause is the trailing `//` text
# with its whitespace collapsed, or nothing when the line trails none. Rules 2
# and 3 read the same records: one rule wants the declarations a crate root
# makes, the other wants every declaration that can reach a nested test file.
decl_lines=()
read_declarations() {
  ((${#files[@]})) || return 0
  mapfile -t decl_lines < <(awk -v US=$'\x1f' '
    { sub(/\r$/, "") }
    match($0, /^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?mod[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*;/) {
      decl = substr($0, 1, RLENGTH)
      sub(/;[[:space:]]*$/, "", decl)
      name = parts[split(decl, parts, /[[:space:]]+/)]
      rest = substr($0, RLENGTH + 1)
      sub(/^[[:space:]]*/, "", rest)
      clause = ""
      if (rest ~ /^\/\//) clause = substr(rest, 3)
      gsub(/[[:space:]]+/, " ", clause)
      sub(/^ /, "", clause)
      sub(/ $/, "", clause)
      print FILENAME US FNR US name US clause
    }
  ' "${files[@]}")
}

violations_mod_clause=()

# Rule 2. Every `mod` declaration in a crate root is trailed by one clause cut
# from that module's paragraph.
audit_mod_clause() {
  local record file line name clause
  for record in "${decl_lines[@]}"; do
    IFS=$'\x1f' read -r file line name clause <<<"$record"
    is_crate_root "$file" || continue
    if [[ -z $clause ]]; then
      violations_mod_clause+=("$file:$line: 'mod $name;' trails no clause")
      continue
    fi
    resolve_mod "$file" "$name"
    if [[ -z $RESOLVED ]]; then
      violations_mod_clause+=("$file:$line: 'mod $name;' names no file to cut a clause from")
      continue
    fi
    if ! read_paragraph "$RESOLVED"; then
      violations_mod_clause+=("$file:$line: 'mod $name;' names $RESOLVED, which opens with no //! paragraph")
      continue
    fi
    if [[ ${PARA,,} != *"${clause,,}"* ]]; then
      violations_mod_clause+=("$file:$line: clause '$clause' is not cut from the paragraph of $RESOLVED")
    fi
  done
}

violations_undeclared_test_file=()

# Rule 3. A file under a tests tree that no declaration names is reported where
# it lies. Files directly under a tests directory are cargo's own targets and so
# are the roots of the walk; everything nested must be reached from one.
audit_undeclared_test_file() {
  local -A children=() reached=()
  local -a queue=()
  local record file line name clause child dir index

  # The walk's edges: every declaration, resolved to the file it names.
  for record in "${decl_lines[@]}"; do
    IFS=$'\x1f' read -r file line name clause <<<"$record"
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

# The opening word of a name, when it is one the spec bans: `test_`, `it_`,
# `works` and `check_`, where a bare `test` opens nothing either.
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

# Rule 4. Every `#[test]` name states the behaviour.
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
audit_module_paragraph
report module-paragraph "${violations_module_paragraph[@]}"
audit_mod_clause
report mod-clause "${violations_mod_clause[@]}"
audit_undeclared_test_file
report undeclared-test-file "${violations_undeclared_test_file[@]}"
read_test_names
audit_test_name
report test-name "${violations_test_name[@]}"

exit "$status"
