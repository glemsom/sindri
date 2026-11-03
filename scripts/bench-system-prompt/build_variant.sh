#!/bin/bash
# build_variant.sh VARIANT — patch the system prompt in the tree, build the CLI,
# and keep the binary as bin-<variant>. `git checkout` the file afterwards: the
# variants are the texts in variants.py, and v00 is not one of them — it is the
# tree's own `PREAMBLE`, which variants.py reads out of that file — so v00
# patches nothing and builds the prompt the tree ships.
set -eu
BENCH=$(dirname "$(readlink -f "$0")")
variant=$1
python3 - "$variant" "$BENCH" <<'PY'
import pathlib, sys
variant, bench = sys.argv[1], sys.argv[2]
sys.path.insert(0, bench); import variants
repo = pathlib.Path(bench).resolve().parents[1]
wire = repo / "crates/core/src/provider/rig_wire.rs"
source = wire.read_text()
head = 'const PREAMBLE: &str = "'
start = source.index(head) + len(head)
end = source.index('";', start)
if variant == "v00":
    text = variants.BASE
else:
    text = variants.VARIANTS[variant][1]
    wire.write_text(source[:start] + text.replace("\\", "\\\\").replace('"', '\\"') + source[end:])
print(f"{variant}\twords={len(text.split())}\tchars={len(text)}")
PY
repo=$(cd "$BENCH/../.." && pwd)
cd "$repo" && devbox run -- cargo build --quiet --package sindri-cli --bin sindri
cp "$repo/target/debug/sindri" "$BENCH/bin-$variant"
