# lib.sh: helpers shared by the tests.

fail() {
    echo "FAIL: $*" >&2
    exit 1
}
