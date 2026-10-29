#!/usr/bin/env bash
# The changelog a release page carries, derived from the tickets it closes.
# Every slice lands as a merge commit whose subject is "TN: <title> (#issue)",
# so the list of tickets in a release is the list of merges since the previous
# tag — no GitHub API, no network, and identical on a maintainer's machine.
set -euo pipefail

tag=${1:?usage: release-changelog.sh TAG}

if ! git rev-parse --verify --quiet "$tag" >/dev/null; then
    echo "release-changelog: no such tag: $tag" >&2
    exit 1
fi

# The previous release is the newest tag merged into this one, excluding this
# one. `$tag^` would be wrong: a release tag sits on the merge commit, whose
# first parent is the branch the work started from, not the last release.
previous=$(git tag --merged "$tag" --sort=-v:refname | grep -vx -- "$tag" | head -1 || true)
if [ -n "$previous" ]; then
    range="$previous..$tag"
else
    range="$tag"
fi

entries=$(
    git log --merges --format=%s "$range" |
        sed -n 's/^T[0-9][0-9]*: //p' |
        sed 's/^/- /'
)

printf '## %s\n\n' "$tag"
if [ -n "$entries" ]; then
    [ -n "$previous" ] && printf 'Changes since %s.\n\n' "$previous"
    printf '%s\n' "$entries"
else
    printf 'No tickets closed in this release.\n'
fi
