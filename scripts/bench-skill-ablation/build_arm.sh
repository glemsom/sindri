#!/bin/bash
# build_arm.sh with|without — the two arms are two builds.
#
# `with` builds the tree as it stands. `without` empties the built-in table in
# `crates/core/src/skills/builtin.rs`, builds, and puts the file back byte for
# byte — so the ablation is the absence of the catalog line and not a switch a
# run could forget to set. `git checkout` is not used: it would also throw away
# work that was never committed.
#
# Two ways this can lie are closed here. A tree whose table is already empty
# builds a `with` arm that ships nothing, so the table is read before anything is
# touched and an emptied one is refused rather than papered over — that is the
# state an interrupted build leaves behind. And the binary that was built is
# asked what it ships: an arm whose catalog is not the arm it is named after
# fails here instead of becoming a row in a table.
set -eu
BENCH=$(dirname "$(readlink -f "$0")")
arm=${1:?usage: build_arm.sh with|without}
repo=$(cd "$BENCH/../.." && pwd)
table="$repo/crates/core/src/skills/builtin.rs"

case $arm in with|without) : ;; *) echo "build_arm: arm must be with or without, not '$arm'" >&2; exit 2;; esac

# The untouched table both arms start from: `with` builds it as it stands and
# `without` empties it. Already empty means a previous build died holding the
# file, and neither arm can be built from it.
python3 - "$table" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
source = path.read_text()
if re.search(r"pub const BUILTINS: &\[Builtin\] = &\[\];", source):
    raise SystemExit(
        "build_arm: the built-in table is already empty, so this tree cannot build "
        "either arm — restore it with `git checkout -- crates/core/src/skills/builtin.rs`"
    )
if not re.search(r"pub const BUILTINS: &\[Builtin\] = &\[.*?\n\}\];", source, re.S):
    raise SystemExit("build_arm: the built-in table is not where it was")
PY

saved=$(mktemp "${TMPDIR:-/tmp}/builtin.XXXXXX")
cp "$table" "$saved"
# A failure past the edit would leave the tree holding an emptied table, so the
# file goes back on every way out — a non-zero exit, a Ctrl-C, a kill — and only
# once, because the saved copy is gone after the first restore. A `kill -9`
# cannot be caught; the check above is what catches the tree it leaves.
restore() {
  [ -f "$saved" ] || return 0
  cp "$saved" "$table" && rm -f "$saved"
}
trap restore EXIT INT TERM HUP

if [ "$arm" = without ]; then
    python3 - "$table" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
source = path.read_text()
emptied, count = re.subn(
    r"pub const BUILTINS: &\[Builtin\] = &\[.*?\n\}\];",
    "pub const BUILTINS: &[Builtin] = &[];",
    source,
    count=1,
    flags=re.S,
)
if count != 1:
    raise SystemExit("build_arm: the built-in table is not where it was")
path.write_text(emptied)
print("build_arm: the built-in table is empty for this build")
PY
fi

cd "$repo" && devbox run -- cargo build --quiet --package sindri-cli --bin sindri
cp "$repo/target/debug/sindri" "$BENCH/bin-$arm"
restore
trap - EXIT INT TERM HUP

# What the arm is, asked of the binary rather than assumed from the flag: a
# `with` build over an emptied table would otherwise be a row of numbers with
# no built-in in it.
ships=no
"$BENCH/bin-$arm" doctor 2>/dev/null | grep -q '^  builtin ' && ships=yes
if [ "$ships" != "$([ "$arm" = with ] && echo yes || echo no)" ]; then
    echo "build_arm: bin-$arm does not match the $arm arm — it ships $ships built-in" >&2
    exit 1
fi
[ "$ships" = yes ] && echo "build_arm: $arm ships the built-in in its catalog" \
    || echo "build_arm: $arm ships no built-in"
