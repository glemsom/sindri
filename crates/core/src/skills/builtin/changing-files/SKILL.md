---
name: changing-files
description: "Changing part of a file: the whole-file rewrite loses what surrounds the change, so read the anchor, assert it once, and let the tool's own answer stand."
---

# Changing a file

A file is mostly somebody else's text, and the work is one clause of it. The
default that loses the rest is the whole-file rewrite — read the file, write it
back with a line different — and the diff then claims that what was read was all
there was. What the read never reached, the rewrite deletes: a comment past the
range that was printed, the file's own formatting, a sibling's edit made since.
Every step below exists to make that impossible rather than to notice it after.

## Read the anchor, and copy it verbatim

The anchor is the smallest run of text that is exactly where the change goes: a
line, a signature, the opening of a block. Read it in a `bash` call — `rg -n` for
the line, `sed -n 'A,Bp'` for the shape around it — and copy it out of that
output byte for byte, indentation and trailing comma included. An anchor written
from memory is a change that lands somewhere else, or nowhere and silently.

## Assert it appears exactly once, before the write

An anchor found twice is the change made twice; an anchor found nowhere is a
write that changes nothing. So the edit counts first and writes second —
`text.count(anchor) == 1` — and a count that fails names the file, the anchor and
how many times it was found. The write comes after the count, so a failed
assertion leaves the tree untouched instead of half-edited.

## Keep what surrounds the change

Change the fewest lines the task needs and nothing else: the neighbouring code
keeps its words, its comments and its indentation, and a line that is added
copies the style of the lines it lands between. A reviewer reads the diff, and a
reformatted function buries the one line that changed.

## Let the tool answer where there is one

A fact another program owns is not yours to hand-edit. A formatter — `cargo fmt`,
`gofmt`, `prettier` — settles layout; a package manager settles a lockfile; a
generator writes the file it is named for, and a change made by hand to a
generated file is one the generator's next run deletes. So let the tool answer,
or change the input the file is generated from, and edit by hand only what no
tool owns.

## Many files: every anchor asserted before any file is written

A loop that writes as it goes leaves the tree half-edited when the fourth anchor
turns out wrong: three files have already moved and the failure names only the
fourth. So the pass is two — check every anchor of every file, collect what did
not hold, and only then write — and one `bash` call carries the whole pass
instead of one call per file. A bad anchor then names its file and its count, and
nothing has moved. `references/edit-plan.md` is that pass, whole.

## Never weaken the assertion

The moment the check is in the way, the wrong move is to make it pass: an
`assert` turned into a `print`, a `count <= 1`, an `|| true` after the writer, a
`2>/dev/null` on it. Each of them stops noticing the breakage the check was
written for, and the weakened check is the one that would have caught the next.

## Re-read the changed region, then prove it

A write that printed nothing is not a change that happened. Read the region back
— `sed -n 'A,Bp'` on the lines that moved — and run the narrowest check that
proves it: the one test, the parser, the formatter's `--check`, the build of the
one crate. The narrowest check is the fast one, so it is the one that gets run;
the whole suite is for the end of the work. Then say what changed: the file, the
region, and what the check printed.
