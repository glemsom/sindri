#!/bin/bash
# build_arm.sh with|without|no-trigger — the three arms are three builds.
#
# `with` builds the tree as it stands. `without` empties the built-in table in
# `crates/core/src/skills/builtin.rs`, so the catalog holds no skill and the
# `skill` tool is never registered. `no-trigger` writes the shipped doctrine one
# sentence shorter into `crates/core/src/agent/skill/doctrine.md`: the same
# catalog, the same body, and the line that asks for a load before a task is
# touched gone. Each arm puts every file it touched back byte for byte, so the
# difference is the text the binary was built from and not a switch a run could
# forget to set. `git checkout` is not used: it would also throw away work that
# was never committed.
#
# Two ways this can lie are closed here. A tree whose table is already empty
# builds a `with` arm that ships nothing, so the table is read before anything is
# touched and an emptied one is refused rather than papered over — that is the
# state an interrupted build leaves behind. And the binary that was built is
# asked what it ships: its catalog through `doctor`, and the trigger sentence
# through the text it carries. An arm whose catalog or doctrine is not the one it
# is named after fails here instead of becoming a row in a table.
set -eu
BENCH=$(dirname "$(readlink -f "$0")")
arm=${1:?usage: build_arm.sh with|without|no-trigger}
repo=$(cd "$BENCH/../.." && pwd)
table="$repo/crates/core/src/skills/builtin.rs"
doctrine="$repo/crates/core/src/agent/skill/doctrine.md"

# shellcheck source=arms.sh
. "$BENCH/arms.sh"
if ! arm_is_a_name "$arm"; then
  echo "build_arm: arm must be $(arms_words), not '$arm'" >&2; exit 2
fi

# The untouched table both arms that ship a built-in start from: `with` and
# `no-trigger` build it as it stands and `without` empties it. Already empty
# means a previous build died holding the file, and no arm can be built from it.
python3 - "$table" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
source = path.read_text()
if re.search(r"pub const BUILTINS: &\[Content\] = &\[\];", source):
    raise SystemExit(
        "build_arm: the built-in table is already empty, so this tree cannot build "
        "any arm — restore it with `git checkout -- crates/core/src/skills/builtin.rs`"
    )
if not re.search(r"pub const BUILTINS: &\[Content\] = &\[.*?\n\];", source, re.S):
    raise SystemExit("build_arm: the built-in table is not where it was")
PY

saved=$(mktemp -d "${TMPDIR:-/tmp}/build-arm.XXXXXX")
cp "$table" "$saved/builtin.rs"
cp "$doctrine" "$saved/doctrine.md"
# A failure past an edit would leave the tree holding one arm's text, so every
# touched file goes back on every way out — a non-zero exit, a Ctrl-C, a kill —
# and only once, because the saved copies are gone after the first restore. A
# `kill -9` cannot be caught; the check above is what catches the tree it leaves.
restore() {
  [ -d "$saved" ] || return 0
  cp "$saved/builtin.rs" "$table" && cp "$saved/doctrine.md" "$doctrine" \
    && rm -rf "$saved"
}
trap restore EXIT INT TERM HUP

if [ "$arm" = without ]; then
    python3 - "$table" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
source = path.read_text()
emptied, count = re.subn(
    r"pub const BUILTINS: &\[Content\] = &\[.*?\n\];",
    "pub const BUILTINS: &[Content] = &[];",
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

if [ "$arm" = no-trigger ]; then
    # The text the arm is, written by the module that also refuses a doctrine
    # that lost the sentence: the arm and the guard rail are one function.
    python3 "$BENCH/doctrines.py" without-trigger "$doctrine" > "$saved/arm.md"
    cp "$saved/arm.md" "$doctrine"
    echo "build_arm: the doctrine is one sentence shorter for this build"
fi

cd "$repo" && devbox run -- cargo build --quiet --package sindri-cli --bin sindri
cp "$repo/target/debug/sindri" "$BENCH/bin-$arm"
restore
trap - EXIT INT TERM HUP

# What the arm is, asked of the binary rather than assumed from the flag: a
# `with` build over an emptied table would otherwise be a row of numbers with no
# built-in in it, and a `no-trigger` build over the shipped doctrine would be a
# second row of the same words.
if ! reason=$(arm_mismatch "$arm" "$BENCH/bin-$arm"); then
  echo "build_arm: bin-$arm is not the $arm arm: $reason" >&2
  exit 1
fi
echo "build_arm: bin-$arm is the $arm arm"
