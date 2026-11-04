# The file list the four audits walk, written once so the four agree on what the
# tree is — and so none of them is the one that checks nothing.
#
# The set is what `git status` sees: a tracked file and an unstaged one, with a
# gitignored tree out. A listing that failed is not an empty tree. The `fatal:` a
# `git ls-files` outside a repository prints is quoted, and the call returns
# non-zero, which under the audits' `set -e` stops the audit before it prints a
# count: an audit that reported a clean sweep of nothing would say the rule holds
# when nobody looked.
#
# Sourced, never run — `make audit` runs the four audits, and this is the tree
# they share. It sets no shell option of its own. It calls `git` and nothing
# else, matching the pattern in the shell, so the tool whose failure it reports
# is the only tool that can go missing: a `grep` that vanished could otherwise
# turn the listing into the same small emptiness it is meant to catch.
#
# The path to it is cut by parameter expansion at each call site rather than by
# `dirname`, so a bare PATH reaches the listing and hears it out.

tree_listing() { # a `grep -E` pattern, then the paths matching it on stdout
    local pattern=$1
    local listing line
    # `&&` rather than `;`: the second call cannot pass once the first has
    # failed, and one `fatal:` reads better than the same one twice. Stderr is
    # folded into the capture so that failure can be quoted whole; a listing
    # that succeeded has nothing there, and a line of it that is not a path is
    # stepped over by each audit's own `[ -f ]`.
    if ! listing=$({ git ls-files && git ls-files --others --exclude-standard; } 2>&1); then
        printf 'audit: could not list the tree: %s\n' "$listing" >&2
        printf 'audit: no file was checked, so this audit fails rather than passing over nothing\n' >&2
        return 1
    fi
    while IFS= read -r line; do
        if [[ $line =~ $pattern ]]; then
            printf '%s\n' "$line"
        fi
    done <<<"$listing"
    # A tree with nothing matching is a list of nothing, not a failed call: it is
    # listed, and it passes with the count it prints.
    return 0
}
