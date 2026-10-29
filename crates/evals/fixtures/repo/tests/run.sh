#!/bin/sh
# The project's tests: `sh tests/run.sh`.
set -eu
here=$(dirname "$0")
. "$here/../src/lib.sh"
. "$here/../src/greet.sh"

got=$(sh "$here/../bin/greet" Ada)
[ "$got" = "Hello, Ada!" ] || fail "greet Ada printed $got"

got=$(say_hello Ada)
[ "$got" = "Hello, Ada!" ] || fail "say_hello Ada printed $got"

echo "all tests pass"
