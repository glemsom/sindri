# Both halves of a release-script test: the assertions, and the fixtures that
# stand in for a release (a `file://` directory of assets and checksums) and for
# the user (the installer run through `sh`). A failed assertion is a non-zero
# exit status, which the runner reports and counts; there is no other channel.

fail() {
    printf '  %s\n' "$*" >&2
    exit 1
}

assert_status() { # expected actual
    [ "$1" = "$2" ] || fail "exit status: wanted $1, got $2${ERR:+ (stderr: $ERR)}"
}

assert_contains() { # haystack needle
    case "$1" in
        *"$2"*) ;;
        *) fail "output does not contain '$2': $1" ;;
    esac
}

assert_file() { # path
    [ -f "$1" ] || fail "no file at $1"
}

assert_no_file() { # path
    [ ! -e "$1" ] || fail "unexpected file at $1"
}

assert_executable() { # path
    [ -x "$1" ] || fail "$1 is not executable"
}

# A release as a user's `curl` would find it: an asset whose `--version` names
# its version and target, and the checksum file the installer verifies.
make_release() { # dir version target
    mkdir -p "$1/v$2"
    printf '#!/bin/sh\n# built for %s\necho "sindri %s (2025-10-29)"\n' "$3" "$2" \
        >"$1/v$2/sindri-$2-$3"
    chmod +x "$1/v$2/sindri-$2-$3"
    (cd "$1/v$2" && sha256sum ./*sindri-* | sed 's#\./##' >SHA256SUMS)
    printf '%s\n' "$1/v$2/sindri-$2-$3"
}

# The installer, run the way `curl | sh` runs it: through `sh`, never bash.
run_installer() { # args…
    # Off the network: the release is a directory in the test's own temp.
    SINDRI_RELEASE_BASE_URL="${SINDRI_RELEASE_BASE_URL:-file://$PWD/releases}"
    export SINDRI_RELEASE_BASE_URL
    set +e
    OUT=$(sh "$SINDRI_ROOT/install.sh" "$@" 2>"$PWD/stderr")
    STATUS=$?
    set -e
    ERR=$(cat "$PWD/stderr")
}

assert_contains_absent() { # haystack needle
    case "$1" in
        *"$2"*) fail "output contains '$2': $1" ;;
        *) ;;
    esac
}
