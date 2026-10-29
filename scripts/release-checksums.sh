#!/usr/bin/env bash
# The checksum file a release carries. Exactly the artefacts, sorted by name,
# with the plain file name in the second column so `sha256sum -c SHA256SUMS`
# verifies from inside the release directory and the installer can find the
# line for the asset it downloaded.
set -euo pipefail

dir=${1:?usage: release-checksums.sh RELEASE_DIR}
out="$dir/SHA256SUMS"

shopt -s nullglob
artefacts=("$dir"/sindri-*)
shopt -u nullglob
if [ "${#artefacts[@]}" -eq 0 ]; then
    echo "release-checksums: no artefacts in $dir — nothing to checksum" >&2
    exit 1
fi

tmp="$out.tmp.$$"
trap 'rm -f "$tmp"' EXIT
: >"$tmp"
while IFS= read -r name; do
    printf '%s  %s\n' "$(sha256sum "$dir/$name" | cut -d' ' -f1)" "$name" >>"$tmp"
done < <(printf '%s\n' "${artefacts[@]##*/}" | LC_ALL=C sort)
mv "$tmp" "$out"
echo "release-checksums: ${#artefacts[@]} artefacts in $out"
