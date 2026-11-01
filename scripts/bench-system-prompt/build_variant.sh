#!/bin/bash
# build_variant.sh VARIANT — patch the system prompt in the tree, build the CLI,
# and keep the binary as bin-<variant>. `git checkout` the file afterwards: the
# prompt under test is the one in variants.py, not the one in the tree.
set -eu
BENCH=$(dirname "$(readlink -f "$0")")
variant=$1
python3 - "$variant" "$BENCH" <<'PY'
import pathlib, sys
variant, bench = sys.argv[1], sys.argv[2]
sys.path.insert(0, bench); import variants
repo = pathlib.Path(bench).resolve().parents[1]
wire = repo / "crates/core/src/provider/rig_wire.rs"
text = variants.BASE if variant == "v00" else variants.VARIANTS[variant][1]
source = wire.read_text()
start = source.index('const PREAMBLE: &str = "') + len('const PREAMBLE: &str = "')
end = source.index('";', start)
wire.write_text(source[:start] + text.replace("\\", "\\\\").replace('"', '\\"') + source[end:])
print(f"{variant}\twords={len(text.split())}\tchars={len(text)}")
PY
repo=$(cd "$BENCH/../.." && pwd)
cd "$repo" && devbox run -- cargo build --quiet --package sindri-cli --bin sindri
cp "$repo/target/debug/sindri" "$BENCH/bin-$variant"
