# Both halves of a release-script test: the assertions, and the fixtures that
# stand in for a release (a `file://` directory of assets and checksums) and for
# the user (the installer run through `sh`) — and for a tree the audits walk, a
# module of the test's own words and a file that has outgrown the limit. A
# failed assertion is a non-zero exit status, which the runner reports and
# counts; there is no other channel.

module() { # path, then the file's lines on stdin
    mkdir -p "$(dirname "$1")"
    cat >"$1"
}

long_file() { # path lines
    mkdir -p "$(dirname "$1")"
    for _ in $(seq "$2"); do echo '// a line'; done >"$1"
}

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

assert_empty() { # text
    [ -z "$1" ] || fail "output is not empty: $1"
}

assert_executable() { # path
    [ -x "$1" ] || fail "$1 is not executable"
}

# One asset of a release as a user's `curl` would find it: a file under exactly
# the name it is given, plus the checksum file the installer verifies. A test
# that names an artefact hands over the name it read, never one it composes.
make_named_release() { # dir version name
    mkdir -p "$1/v$2"
    printf '#!/bin/sh\n# built as %s\necho "sindri %s (2025-10-29)"\n' "$3" "$2" \
        >"$1/v$2/$3"
    chmod +x "$1/v$2/$3"
    (cd "$1/v$2" && sha256sum ./*sindri-* | sed 's#\./##' >SHA256SUMS)
    printf '%s\n' "$1/v$2/$3"
}

# The same fixture for a test that has only a version and a target to go on: the
# installer's own naming, spelled out here, which is why the artefact-naming
# test above reads the build's answer instead.
make_release() { # dir version target
    make_named_release "$1" "$2" "sindri-$2-$3"
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
