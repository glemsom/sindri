# The facts the rail gives up have a screen, and its name is `/stats`

The rail is thirty-two columns wide and it has to choose. What it cannot hold — the route the wire
answered on, what the run has spent in tokens, when it started, when and why it stopped, the whole
GUID, the whole temp, the whole workspace — has no home at all, so a reader who needs one of them
cannot see it anywhere. Five decisions put them on a screen, and each of them rules something out.

**The screen is `/stats`, and there is no `/status`.** `GLOSSARY.md`'s **Status** is the run's own
five words — `idle`, `working`, `done`, `blocked`, `error` — and "never the command, the prompt or
the answer". A screen named `/status` that holds the model, the spend, the route and the paths blurs
that word twice: it is not the run's state, and it does hold the words Status is forbidden to hold.
`/stats` is an ordinary word that blurs nothing, and it is the word a person types to ask what a run
cost. There is no `/status` command now, and there is not one later: a reader who types it is asking
the model a question, which is what every line that names no command does.

**The screen reads the run's record; the rail's `Facts` is not widened into it.** `ctx` and `cache`
already have exactly one owner — the rail's `Facts`, derived by the run's own rules — and a value
widened to hold everything would carry a document and a derivation in one shape. Two values built
from one record, at one place, at one moment, cannot disagree; one value with two shapes can. So the
screen is built from the three things the frame was handed: the facts, the record, and the names it
has loaded.

**The rail keeps `temp` and `skills`; nothing is shed.** The rail's blocks are a contract its tests
and its fixtures hold it to, read by people who never open the screen. The screen adds a second home
for a fact; it does not take the first one's away, and a rail with a hole in it buys nothing for a
fact one keystroke away.

**The screen is settled, not live.** It draws the facts last handed to the frame — at open, after
every turn, and once when the run stops — and never reaches for the record itself. A screen that read
the file the run is writing beside it could show, on one frame, a number the rail beside it did not.
The hand-over is the same one the rail rides on, so one path carries both and neither can arrive
without the other.

**It is a screen and not a band.** It holds more rows than any band, and every other full view in
this front end already trades the bands for one view on `Esc`. What is under it is not painted,
exactly as nothing is painted under `?`, `@`, `/model` or `/settings`.

## What a reader is held to

- No fact reaches this screen that the core does not record, and no fact is computed twice: `ctx`
  and `cache` come from `Facts`, and `exit` from the core's own `Stop::exit_code()` table.
- A fact the run knows nothing about is not drawn, and a row is never drawn in part: a path keeps
  whole components and marks the head it left with `…/`, a name keeps its head and marks the tail
  with `…`, and a value with no room for either is not there at all.
- The rail's blocks, its sacrifice order and its fixtures are unchanged by the screen's landing.

ADR-0018's habits hold this file as they hold every other: a numbered name, a slug, no number
reused, and no retired marker on a live record.
