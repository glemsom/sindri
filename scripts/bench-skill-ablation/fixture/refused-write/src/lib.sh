# lib.sh: helpers shared by the tests, and where greet keeps its log.

state_dir() {
    echo /var/lib/greet
}

fail() {
    echo "FAIL: $*" >&2
    exit 1
}
