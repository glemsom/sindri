#!/bin/sh
# The installer. One command, no Rust toolchain, and nothing is written until
# the download's checksum matches the release's SHA256SUMS. It ends by sending
# the user to `sindri doctor`, because a sandbox that cannot start is the one
# failure a fresh install must not leave them to discover on their own.
#
#   curl -fsSL https://raw.githubusercontent.com/glemsom/sindri/main/install.sh | sh
#
# Flags: --version VERSION, --dir DIR. Exit 2 is a usage error — including a
# kernel or machine no release serves — and 1 anything else: a failed download,
# a checksum mismatch, an unwritable directory.

set -eu

repo=${SINDRI_REPO:-glemsom/sindri}
releases=${SINDRI_RELEASE_BASE_URL:-https://github.com/$repo/releases/download}
latest_url=${SINDRI_LATEST_URL:-https://api.github.com/repos/$repo/releases/latest}
version=${SINDRI_VERSION:-}
dir=${SINDRI_INSTALL_DIR:-}

usage() {
    cat <<'EOF'
usage: install.sh [--version VERSION] [--dir DIR]

  --version VERSION  the release to install (default: the latest one)
  --dir DIR          where to put the binary (default: ~/.local/bin)

environment:
  SINDRI_VERSION             same as --version
  SINDRI_INSTALL_DIR         same as --dir
  SINDRI_RELEASE_BASE_URL    where releases live (a mirror, or a file:// dir)
  SINDRI_REPO                owner/name, default glemsom/sindri
  SINDRI_LATEST_URL          where "the latest release" is announced
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --version)
            [ $# -ge 2 ] || { echo "install: --version needs a value" >&2; exit 2; }
            version=$2
            shift 2
            ;;
        --dir)
            [ $# -ge 2 ] || { echo "install: --dir needs a value" >&2; exit 2; }
            dir=$2
            shift 2
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo "install: unknown argument '$1'" >&2
            usage >&2
            exit 2
            ;;
    esac
done

die() {
    printf 'install: %s\n' "$*" >&2
    exit 1
}

command -v curl >/dev/null 2>&1 || die "curl is needed to download the release"
command -v sha256sum >/dev/null 2>&1 || die "sha256sum is needed to verify the release"

# The kernel first: a static Linux binary is the only thing a release carries.
kernel=${SINDRI_UNAME_S:-$(uname -s)}
if [ "$kernel" != "Linux" ]; then
    printf 'install: sindri ships Linux binaries only; this is %s\n' "$kernel" >&2
    exit 2
fi
machine=${SINDRI_UNAME_M:-$(uname -m)}
case "$machine" in
    x86_64 | amd64) target=x86_64-unknown-linux-musl ;;
    aarch64 | arm64) target=aarch64-unknown-linux-musl ;;
    *)
        printf 'install: no static binary for this machine (%s)\n' "$machine" >&2
        printf 'install: releases carry x86_64-unknown-linux-musl and aarch64-unknown-linux-musl\n' >&2
        exit 2
        ;;
esac

tmp=$(mktemp -d "${TMPDIR:-/tmp}/sindri-install.XXXXXX")
trap 'rm -rf "$tmp"' 0 1 2 15

fetch() { # url destination
    curl -fsSL "$1" -o "$2" || die "could not download $1"
}

if [ -z "$version" ]; then
    fetch "$latest_url" "$tmp/latest.json"
    tag=$(sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' "$tmp/latest.json" | head -1)
    [ -n "$tag" ] || die "could not find the latest release; pass --version"
    version=$tag
fi
version=${version#v}

asset="sindri-$version-$target"
fetch "$releases/v$version/$asset" "$tmp/$asset"
fetch "$releases/v$version/SHA256SUMS" "$tmp/SHA256SUMS"

# Verify before installing: a truncated download must not become a binary that
# fails in the user's shell a minute later.
expected=$(awk -v name="$asset" '{ sub(/^\*/, "", $2) } $2 == name || $NF == name { print $1; exit }' \
    "$tmp/SHA256SUMS")
[ -n "$expected" ] || die "SHA256SUMS for v$version lists no $asset"
actual=$(sha256sum "$tmp/$asset" | cut -d' ' -f1)
if [ "$expected" != "$actual" ]; then
    printf 'install: checksum mismatch for %s\n' "$asset" >&2
    printf 'install: expected %s\ninstall: got      %s\n' "$expected" "$actual" >&2
    exit 1
fi

dir=${dir:-${HOME:?HOME is not set; pass --dir}/.local/bin}
mkdir -p "$dir" || die "could not create $dir"
# Also remove the half-written copy, so a failed install leaves no surprise in
# a directory the user has on their PATH.
trap 'rm -f "$dir/.sindri.$$"; rm -rf "$tmp"' 0 1 2 15
cp "$tmp/$asset" "$dir/.sindri.$$" || die "could not write to $dir"
chmod 0755 "$dir/.sindri.$$"
mv "$dir/.sindri.$$" "$dir/sindri" || die "could not install $dir/sindri"

# The binary's own answer, not the release's claim: if it cannot run here, say
# so rather than reporting a version nobody verified.
if ! resolved=$("$dir/sindri" --version 2>/dev/null); then
    printf 'note: %s did not run here; reporting the release version\n' "$dir/sindri" >&2
    resolved="sindri $version"
fi
printf 'installed %s to %s\n' "$resolved" "$dir"

case ":$PATH:" in
    *":$dir:"*) ;;
    *) printf 'note: %s is not on your PATH; add it to run sindri by name\n' "$dir" ;;
esac

printf 'Next: run `sindri doctor` to prove the sandbox works on this machine.\n'
