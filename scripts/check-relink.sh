#!/usr/bin/env bash
# An unchanged tree must not relink (ADR-0009). A build script that stamped the
# wall clock, or an env var that changed on every build, would recompile and
# relink every crate each time; this fails if it does.
set -euo pipefail

binary=target/debug/sindri
stamp() { stat -c '%Y %s %i' "$binary"; }

cargo build --quiet --package sindri-cli --bin sindri
before=$(stamp)
# Long enough that a one-second timestamp would differ if it were being used.
sleep 1.2
cargo build --quiet --package sindri-cli --bin sindri
after=$(stamp)

if [ "$before" != "$after" ]; then
    echo "relink: an unchanged tree rebuilt $binary ($before -> $after)" >&2
    exit 1
fi
echo "relink: an unchanged tree did not relink"
