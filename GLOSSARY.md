# Glossary

Sindri's vocabulary. Use these words exactly; do not drift to synonyms.

## Agent

**turn** — one round trip to the model: our request, its response, the tool results we
feed back. Bounded by the **turn cap** (`max_turns`, default 250).

**tool call** — one request from the model to run `bash`, plus the structured **tool
result** returned. One call may run a whole pipeline or script.

**bash tool** — the only tool. Runs a command string inside the sandbox.

**answer** — the final assistant text of the last turn. The product.

**stopped** — why the loop ended: `final`, `max_turns`, `cancelled` or `error`.

**cancel** — Ctrl-C (SIGINT): one flag the loop and the executor share. The loop stops
between turns, the command's process group is killed, stdout stays empty, exit is 130. In the **frame** a cancel is answered the moment the key
lands — the footer reads `cancelling…` and the draft stays in the **composer** — and the
state clears when the run has actually stopped, because a request in flight is not
interrupted.

## Sandbox

**sandbox policy** — the value holding writable set, network posture and temp mapping:
the only source of both enforcement and description (ADR-0002).

**writable set** — the paths mounted read-write besides the session temp: the workspace
plus **extras**.

**extras** — user-configured extra writable paths.

**session temp** — per-run writable scratch, mapped to `TMPDIR`, `TEMP` and `TMP` in the
sandbox, and mounted as `HOME` beneath it.

**read-only root** — `--ro-bind / /`: the host filesystem inside the sandbox read-only.
A boundary on writes, not on reads (ADR-0010).

**description** — the model-facing text derived from the sandbox policy, also printed by
`sindri doctor`.

**doctor** — `sindri doctor`: verifies bwrap, the binary set and the mount plan, and
prints the description.

## Run

**boot** — turning the flags and the process's own facts into a session ready to start,
and the one line a front end says when it cannot. Not a synonym for **session**: a boot
ends where a session begins (ADR-0015).

**session** — one run, identified by a GUID, recorded under
`$XDG_STATE_HOME/sindri/sessions/<guid>/` (ADR-0006).

**transcript** — the ordered record of messages, tool calls and tool results of a
session.

**window** — the rows of the **transcript** the frame shows at once. It has a place in
the thread rather than an offset from its tail: a reader on the **tail** follows what
arrives, and a reader scrolled back keeps the row they are reading while a run appends
below them, told how much by a `↓ 4 new rows` badge. `End` takes the tail again, `Home`
the first turn, and a thread longer than the window marks its position in the
transcript's last column (ADR-0013).

**tail** — the newest row of the **transcript**, and the place a **window** follows
from unless a reader has scrolled back.

**spill file** — where an over-long tool result's full text is written when the
model-facing copy is truncated head+tail.

**batch mode** — `-b`: the Unix-filter front end. stdout is the answer only; stderr is
silent unless `-v`; `--format json` changes the output shape (ADR-0004).

**reasoning** — provider-side thinking tokens, off by default, shown on stderr with `-v`.

## Terminal UI

**frame** — bare `sindri` on a terminal: the alternate-screen interface as a whole
(ADR-0011).

**rail** — the right column: a two-column grid. Above the rule, what a run is spending —
cache hit percentage, turns against the cap, tokens, context meter. Below it, what the run
*is*: its model, workspace, git branch, session GUID and session temp path. Labels sit in
a faint nine-column field so every value starts at the same column, a value too long for
its column is ellipsised in the middle (`4f1c8f2e…5c1a77`) rather than wrapped, and the
sandbox posture is a chip (`⬢ bwrap · 1 writable`) rather than another pair. 32 columns
from 100 up; below that, the same facts in the same order as one line under the
transcript, which never loses the posture, the model or the workspace. The GUID is copied
with `Ctrl-G`, or by clicking its row.

**pulse** — the frame's own clock, and the only thing drawn without being asked:
a ~120 ms tick while a run is live or a completion is walking, nothing at all
while the frame is still. It shows a braille marker at the head of the newest
answer with the run's elapsed time (`⠸ 0.8s`), settling on `✓` or `✗` and the
duration it stopped at; the duration of every call, frozen when the call ends;
and the rail's output and context figures moving on what streams, accented as
they grow and trued up when the run's report lands (ADR-0011 §4).

**canvas** — the terminal's own black: the lowest of the three tiers, and the one
the frame paints nothing on. Above it: **panel**, then **raised** (ADR-0016).

**panel** — a surface the frame draws on, one tier above the canvas: the composer,
a fenced code block, the rail, the band a turn's **prompt** sits on.

**raised** — the surface an overlay is drawn on, one tier above a panel; the frame
is dimmed beneath it. The rail's sandbox **chip** sits on it too, a tier above the
panel the rest of the rail is drawn on.

**composer** — the input at the bottom: 2–6 rows, `Shift+Enter` for a newline, `@` for a
path completion, `/` for a command. With no text in it at all it carries the placeholder
`Ask, or drop a log here · @ path · / command`, which a draft — a space is a draft —
never sits behind.

**first frame** — the frame before the first prompt: an empty **thread** is not a blank
page, but the line naming the wire, the model, the workspace and the sandbox, one dim
sentence of the contract, and three things that can be typed as they read beside `/help`.
Every fact on it comes from `Launch`; nothing it says is a figure, because no run has
spent any (ADR-0011 §21).

**queued prompt** — a prompt sent while a run has the frame: it waits in one queue and runs
when the run stops, in the order it was typed. The queue is a panel above the **composer**
holding each prompt whole, titled with its size (`3 queued`) when there is more than one and
cut at six rows with that count standing for the rest; `↑` takes the newest back into the
composer above any draft typed since, `Ctrl-X` drops it. The footer carries no count.

**thread** — the growing conversation of one TUI session; `/new` starts over. Laid
out as a document: two blank rows between turns and none inside one, a two-column
gutter a turn's answer and calls hang off, and the prompt at full strength on its
own band. A thread shorter than the column is written on its floor, so the newest row
is the one just above the **composer** rather than the top row of a screenful of blank
ones.

**turn header** — the row that opens a turn: `YOU  <hh:mm>  <workspace>`.

**expanded call** — a **tool call** shown whole in place of the five-line tail it
collapses to: `Enter` on an empty composer toggles the call the frame last showed (the
newest one it can open if the reader has touched none), and a click toggles the call
whose row it landed on. The entry keeps a bounded tail of its whole output — 400 lines,
the cap a **spill file** exists to lift — so a call that never spilled still opens, and
the collapsed row says what is behind it (`✓ exit 0 · 1.4s · 400 lines`). The body is
read through the transcript's own window, so a 400-line expansion is scrolled like any
other rows. **reasoning** opens on the same key; collapsed, it stays one row
(`⌁ reasoned`).

**completion** — the `@` offer of workspace paths: a proximity-ranked background walk,
eight rows, `.git` skipped, `.gitignore` deliberately not honoured.

**record picker** — the `/sessions` overlay: one row per **session** under
`$XDG_STATE_HOME/sindri/sessions/`, newest first — when it ran, its model, its turns, why
it stopped, and the first prompt. `Enter` reads that **transcript** back read-only, `Esc`
closes. Reading is not resuming (ADR-0011 §8).

**export** — `/export [path]`: the **thread** as one markdown file, at the path named or
`sindri-<guid>.md` in the workspace, read out of the record so it says what `sindri
sessions show` prints. A file already there is not overwritten without a second ask.

**settings modal** — the `/settings` overlay: one row per key of the config schema.
`Enter` saves, `Esc` discards, `Ctrl-U` empties the field, `Space` flips a toggle
(ADR-0014). The rows scroll with the cursor; the `file` line, the warning, a refused
save's message and the hints are pinned outside the scroll, so the row a save failed
on is never the row that fell off (ADR-0013).

**binding** — one row of the **help overlay**: the keys, and the mouse gesture that is
their accelerator, that do one thing — and what that thing is. `BINDINGS` is the one
list, and nothing on it is reachable only with a pointer (ADR-0013).

**help overlay** — `?`'s list of every **binding** (ADR-0011 §14): the keys in their
column, each description wrapped under them, the box sized from the wrapped rows. A
list longer than the box scrolls by a screenful and says where it is (`3 of 12`) and
which way there is more; `Esc` and `Enter` close it as they always did (ADR-0013).

**from environment** — the API key field's default, and where emptying it returns: the
credential comes from `SINDRI_API_KEY`, the provider's variable, and nothing is written.

**door** — the frame's front door (`crates/cli/src/frame.rs`): the one place a run with
no credential opens the interface instead of exiting, with the **settings modal** staged
on the `api_key` row (ADR-0004).

**card** — the failure drawn instead of a frame when a `doctor` check fails, holding the
check and the doctor's report whole: sized from the rows that report wrapped into, paged
with `PgUp`/`PgDn` when the screen is shorter, and left only by a key — a resize lays it
out again rather than dismissing it.