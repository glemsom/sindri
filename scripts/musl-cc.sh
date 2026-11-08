#!/usr/bin/env bash
# The musl C compiler the devbox shell names for the musl target
# (`CC_x86_64_unknown_linux_musl`): `zig cc`, told which target it is for.
#
# It is a script rather than the bare command because of one flag. `cc-rs` reads
# `zig cc` as clang and passes `--target=x86_64-unknown-linux-musl`, and zig parses
# `--target` as a target query of its own and refuses `unknown` in it — so the
# target this compiler is named for is the one given, and any other is dropped.
# Everything else `cc-rs` passes, its own flags included, goes through untouched.
args=()
for arg in "$@"; do
  case $arg in
    --target=*) ;;
    *) args+=("$arg") ;;
  esac
done
exec zig cc -target x86_64-linux-musl "${args[@]}"
