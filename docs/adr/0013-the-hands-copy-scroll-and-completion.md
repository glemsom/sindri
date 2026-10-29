# 0013 — The hands: a copy nobody can confirm, a window of rows, a walk with a cap

- **Status:** accepted
- **Date:** 2026-10-30

## Context

ADR-0011 §7, §9, §14 and §16 decided that the frame would select rendered rows
and copy them, scroll a thread of unbounded length, complete `@` paths from a
background walk, and offer the spill file of any output too large to show.
Rendering them raised three questions the contract did not answer:

1. **What does "copied" mean** when the copy leaves through OSC 52, an escape
   sequence a terminal reads and never answers?
2. **How does an unbounded thread keep a bounded render tree** when scrolling
   means the window can start anywhere?
3. **What does the workspace walk cost**, and where does that cost land?

## Decision

**A copy is `Copied::Sent`, `Copied::Tool(name)` or `Copied::Refused(why)`, and
the words match the evidence.** OSC 52 goes to the terminal whenever stdout is a
terminal and the selection fits `OSC52_LIMIT` (100 kB — an escape of a megabyte
is a hang, not a clipboard). Since the terminal does not confirm, the transcript
says *sent to the terminal (OSC 52), which does not confirm*; it never says
*copied*. Without a terminal, or when the terminal refuses the bytes,
`wl-copy` then `xclip -selection clipboard` take the text on stdin, and the one
that exits cleanly is named. A refusal — nothing on `PATH` took the text, or a
tool that failed — becomes an error note in the transcript. There is no path
through the code that turns a refusal into a success. The clipboard is a
`Clipboard` trait in `copy.rs`, so no test reaches the user's real clipboard: a
test reads the escape from its own buffer and runs a fake `wl-copy` from a
temporary `PATH`.

**The window is built from the newest row backwards, and stops.** `window()`
accumulates entry heights from the last entry up, until it has a screenful plus
the scroll offset, and lays out only the entries it walked. Row counts are
cached per entry at the width they were measured at and dropped whenever an
entry changes, so scrolling costs what was scrolled and a thread of ten thousand
turns still lays out a handful of entries per frame. The offset is clamped by
the window itself — the thread's top is where the walking stopped — so no
separate total is kept. Scrolling a thread whose rows all fit is a no-op, and
every gesture has a key: the wheel is `PgUp`/`PgDn`, a click on a spill
affordance is `Ctrl-O`, and a drag is `Shift+Up`/`Shift+Down` to select whole
rows from the newest upwards and `Ctrl-Y` to copy them — or, with nothing
selected, the last answer. A selection is resolved against the plain text of
what the last frame drew and by display columns, so a wide glyph is taken whole
rather than cut in half.

**The walk is capped, on a thread, one at a time, and ranked, not exhaustive.**
`@` starts a scan of the workspace on its own thread. At most one walk runs at
any moment: a keystroke during a walk updates the query the frame is waiting
for, that walk's answer is applied only if its generation is still the newest,
and the frame then asks again for whatever is current — so typing fast costs one
walk at a time rather than a queue of them. The scan stops after `CAP` (20 000)
paths and says so twice: in the `capped` flag `Scan` returns, and as a row in
the completion itself, because a list that is not everything should say so. It
skips `.git` and nothing else, because a file the model can read is a file the
user can point at. Ranking is proximity: whole-path prefix, then
file-name prefix, then substring, then subsequence — eight rows at a time, and a
dotfile only once a dot has been typed.

**A spill file is read only when it is opened, and never re-copied.** The
transcript offers the file (`… output too large · Ctrl-O opens …`) from what the
executor already wrote (ADR-0007). Opening it — `Ctrl-O`, or a click on that
row, which opens *that* row's file rather than the newest one — reads the file
once, and each frame draws a slice of the lines it already holds, scrolled by
whole screenfuls so that the last one is reachable. The render tree holds one
screen of text, not the file.

## Consequences

- The interface never claims a clipboard it could not verify, and an over-SSH or
  `tmux` session gets the same wording as a local one.
- The frame's memory is bounded by the visible window plus the heights cached
  for the entries it has walked; the thread it draws is not.
- `@` completion is honest about its cost: `.gitignore` is not honoured, so a
  large build directory is walked — once per query change, on a thread, up to
  the cap. If that ever feels slow in a real repository, the cap and the ranking
  are the knobs, not the layout.
- Nothing new is written to the user's disk outside the session record; the only
  reads are the spill files the executor already wrote.
