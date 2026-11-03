#!/bin/bash
# build_variant.sh VARIANT — patch the stance in the tree, build the CLI, and
# keep the binary as bin-<variant>. The variants are the texts in `variants.py`,
# and v00 is not one of them: it is the tree's own `STANCE`, which `variants.py`
# reads out of the file the tree ships it in, so v00 patches nothing and builds
# the prompt the tree carries.
#
# The file goes back byte for byte on every way out — a finished build, a failed
# one, a Ctrl-C — from a copy taken before the first byte was written. `git
# checkout` is not used: it would also throw away work that was never committed.
# `PROMPT` names another copy of the file to patch, which is how the gate proves
# that restore without ever holding the shipped prompt open.
set -eu
DEVBOX=${DEVBOX:-devbox}
BENCH=$(dirname "$(readlink -f "$0")")
variant=${1:?usage: build_variant.sh VARIANT}
repo=$(cd "$BENCH/../.." && pwd)

prompt=${PROMPT:-$(python3 - "$BENCH" <<'PY'
import sys
sys.path.insert(0, sys.argv[1]); import variants
print(variants.SHIPPED)
PY
)}
saved=$(mktemp "${TMPDIR:-/tmp}/stance.XXXXXX")
cp "$prompt" "$saved"
restore() {
  [ -f "$saved" ] || return 0
  cp "$saved" "$prompt" && rm -f "$saved"
}
trap restore EXIT INT TERM HUP

python3 - "$variant" "$BENCH" "$prompt" <<'PY'
import sys
sys.path.insert(0, sys.argv[2]); import variants
text = variants.patch(sys.argv[1], sys.argv[3])
print(f"{sys.argv[1]}\twords={len(text.split())}\tchars={len(text)}")
PY

cd "$repo" && $DEVBOX run -- cargo build --quiet --package sindri-cli --bin sindri
cp "$repo/target/debug/sindri" "$BENCH/bin-$variant"
restore
trap - EXIT INT TERM HUP
