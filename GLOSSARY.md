# Glossary

Sindri's vocabulary. Use these words exactly; do not drift to synonyms.

## Agent

**turn** — one round trip to the model: our request, its response, the tool results we
feed back. Bounded by the **turn cap** (`max_turns`, default 250).

**tool call** — one request from the model to run `bash`, plus the structured **tool
result** returned. One call may run a whole pipeline or script.

**bash tool** — the only tool. Runs a command string inside the sandbox.

**answer** — the final assistant text of the last turn. The product. Markdown read into
the frame: prose, fences drawn as panels, links that keep their targets, and **pictures**
with their **captions**.

**stopped** — why the loop ended: `final`, `max_turns`, `cancelled` or `error`.

**cancel** — Ctrl-C (SIGINT): one flag the loop and the executor share. The loop stops
between turns, the command's process group is killed, stdout stays empty, exit is 130.
In the **frame** a cancel is answered the moment the key lands — the footer reads
`cancelling…` and the draft stays in the **composer** — and the state clears when the
run has actually stopped, because a request in flight is not interrupted.

## Sandbox

**sandbox policy** — the value holding writable set, network posture and temp mapping:
the only source of both enforcement and description.

**writable set** — the paths mounted read-write besides the session temp: the workspace
plus **extras**.

**extras** — user-configured extra writable paths.

**session temp** — per-run writable scratch, mapped to `TMPDIR`, `TEMP` and `TMP` in the
sandbox, and mounted as `HOME` beneath it.

**read-only root** — `--ro-bind / /`: the host filesystem inside the sandbox read-only.
A boundary on writes, not on reads.

**description** — the model-facing text derived from the sandbox policy, also printed by
`sindri doctor`.

**yolo-unsafe** — `--yolo-unsafe`: the policy whose enforcement is `none`, so commands run
with the caller's own privileges, outside bubblewrap, inheriting its environment and
working directory. Never persisted: asked for by the invocation that makes it.

**doctor** — `sindri doctor`: verifies bwrap, the binary set and the mount plan, and
prints the description.

## Run

**boot** — turning the flags and the process's own facts into a session ready to start,
and the one line a front end says when it cannot. Not a synonym for **session**: a boot
ends where a session begins.

**session** — one run, identified by a GUID, recorded under
`$XDG_STATE_HOME/sindri/sessions/<guid>/`.

**record** — the files one **session** leaves in that directory: what `sindri sessions
show` and `/export` read, and what the frame never writes
directly. A record is a session read back, not a second thing, and it is added to and
never rewritten — except for the metadata it settles at its end.

**transcript** — the ordered history of messages, tool calls and tool results of a
session.

**window** — the rows of the **transcript** the frame shows at once. It has a place in
the thread rather than an offset from its tail: a reader on the **tail** follows what
arrives, and a reader scrolled back keeps the row they are reading while a run appends
below them, told how much by the **new-rows badge**. That place is an entry and the
rows into it, so a block that grows or folds above the window moves no row under the
reader, and one the top row sits in moves the place back to that entry's own row — the
row `Enter` was pressed on, and the only row of a folded call that is there both before
and after — where a fold would otherwise leave a place that no longer exists and hand
the reader to the tail. `End` takes the tail again, `Home` the first turn, and nothing in
the last column claims to say where the window sits: the **new-rows badge** is the
frame's only word on it.

**new-rows badge** — `↓ 4 new rows`, against the footer's right edge: how many rows
arrived below a **window** that is off the **tail**. It says `1 new row` for one, clears
the moment the window reaches the tail again, and its room is taken before the footer's
hints are chosen.

**tail** — the newest row of the **transcript**, and the place a **window** follows
from unless a reader has scrolled back.

**spill file** — where an over-long tool result's full text is written when the
model-facing copy is truncated head+tail.

**batch mode** — `-b`: the Unix-filter front end. stdout is the answer only; stderr is
silent unless `-v`; `--format json` changes the output shape.

**reasoning** — provider-side thinking tokens, off by default, shown on stderr with `-v`.

## Terminal UI

**frame** — bare `sindri` on a terminal: the alternate-screen interface as a whole.

**rail** — the right column: a ledger in two blocks, split by a hairline. Above the rule,
what a run is spending — cache hit percentage, turns against the cap, tokens, context
meter. Below it, what the run *is*: its model, workspace, git branch, session GUID and
session temp path. Labels sit in a faint nine-column field and the value hangs off the
rail's last column, so every figure ends on one edge and is compared down it; a dot
leader in `LEADER` bridges the field to a value that leaves three cells of gap or more,
where a value that leaves fewer — or none, because it fills its room — is shortened by
`Fit` (`4f1c8f2e…5c1a77`) and draws no leader, and nothing wraps. The rule is a `─`
hairline across the rail's whole width, and the labels begin two cells in: the rail
carries no spine of its own, and the inset holds the first word of a fact off the seam's
light strip, which sits against the rail's first column. The sandbox posture is a chip
(`⬢ bwrap · 1 writable`) rather than another pair, right-aligned on the edge the values
hang off. The rail is chrome, not a surface: it draws no fill and lets the canvas
through, so the one band of it with a colour behind it is the row the chip sits on. What
a row copies is the fact — `context 38%` — never the leader that drew it.
The margin between the words and the rail stays canvas, and on a terminal that speaks
Kitty graphics it carries the seam's light strip (`seam.rs`): the height of the body,
brightest against the rail and dissolving toward the words, and two cells wide because
a picture's escape costs the cell after it and that cell has to be the strip's own, never
the rail's first (`render::GAP_COLUMNS`).
32 columns from 100 up; below that, the same facts in the same order as one line under
the transcript, inset the same two cells, which never loses the posture, the model or the
workspace. One priority order (`rail::priority`) ends with the posture, and the column
and the strip both read it: a column too short for its facts gives them up from the bottom
of that order — the identity block first, since the first frame already names it — and
keeps one row to say how many went (`+7 more`), and a fact too long for its room is
shortened rather than skipped — a posture to `⬢ bwrap` — and dropped only when nothing of
it can be said. The GUID is copied with `Ctrl-G`, or by clicking its row.

**pulse** — the frame's own clock, and the only thing drawn without being asked:
a `pulse::TICK` of ~120 ms while a run is live or a completion is walking, nothing at
all while the frame is still. It shows a braille marker at the head of the newest
answer with the run's elapsed time (`⠸ 0.8s`), settling on `✓` or `✗` and the
duration it stopped at; the duration of every call, frozen when the call ends;
and the rail's output and context figures moving on what streams, accented as
they grow and trued up when the run's report lands. The tick lasts only while one of
those is moving. While it runs the footer says `working · ` before its hints, and
`cancelling… · ` once a cancel has been answered.

**walk** — a rule lit from one end: the theme's `ramp` interpolates two named stops and
`RAMPS` lists the pairs a renderer may interpolate, one step per cell. It is how a
border gets a gradient without a picture and without a colour a renderer wrote down: a
cell is enough room for a step across a hundred columns, and `tests/theme.rs` holds a
walked cell to the segments exactly as it holds a flat one to `PALETTE`. On terminals
that speak no Kitty graphics it is the only light the frame has, the seam's strip
being absent there.

**surface** — one of the three tiers of nearness the frame paints on:
**canvas**, **panel**, **raised**, in that order. A renderer asks
`theme::surface()` or `theme::raised()` and never asks for a background, because
`theme.rs` names every colour the frame paints. One dark theme, three tiers.

**canvas** — the terminal's own black: the lowest **surface**, and the one the frame
paints nothing on. Above it: **panel**, then **raised**.

**panel** — the **surface** the frame draws on, one tier above the canvas: the
composer, a fenced code block, the **queue panel**, the band a turn's **prompt**
sits on. The rail is not one: it is chrome, drawn on the canvas.

**raised** — the **surface** an overlay is drawn on, one tier above a panel; the frame
is dimmed beneath it. The rail's sandbox **chip** sits on it too, a tier above the
canvas the rest of the rail is drawn on.

**chip** — the rail's statement of the sandbox posture, `⬢ bwrap · 1 writable`: a
**raised** box rather than another label and value, shortened to `⬢ bwrap` when its
column runs out.

**composer** — the input at the bottom: 2–6 rows, `Shift+Enter` for a newline, `@` for a
path completion, `/` for a command. With no text in it at all it carries the placeholder
`Ask, or drop a log here · @ path · / command`, which a draft — a space is a draft —
never sits behind. Its border is a **walk**: `PROMPT` while it holds the keyboard and
`FAINT` while a run has it, walked toward `SEAM` across the top and bottom rules so the
panel is lit from the rail's side and its right-hand corners take the rail's own ink.

**first frame** — the frame before the first prompt: an empty **thread** is not a blank
page, but the line naming the wire, the model, the workspace and the sandbox, one dim
sentence of the contract, and two prompts that can be typed as they read beside `/help`.
Every fact on it comes from `Launch`; nothing it says is a figure, because no run has
spent any.

**queued prompt** — a prompt sent while a run has the frame: it waits in the queue and
runs when the run stops, in the order it was typed. `↑` takes the newest back into the
**composer** above any draft typed since, `Ctrl-X` drops it. The footer carries no count
it cannot fit; the **queue panel** states the size.

**queue panel** — the **panel** above the **composer** that holds the **queued prompts**
whole: titled `queued` when one waits and `3 queued` when more, cut at six rows with that
count standing for the rest, and drawn only when the queue is not empty.

**thread** — the growing conversation of one TUI session; `/new` starts over. Laid
out as a document: two blank rows between turns and none inside one, the **gutter** a
turn's answer and calls hang off, and the prompt at full strength on its own band. A
thread shorter than the column is written on its floor, so the newest row is the one
just above the **composer** rather than the top row of a screenful of blank ones.

**gutter** — a left edge the frame draws beside a thing that hangs off it: in the
**thread**, the two columns a turn's answer and its calls hang off; in a fenced code
block, the `│` down its left, which carries a tinted row's wash.

**turn header** — the row that opens a turn: `YOU <hh:mm> <workspace>`.

**expanded call** — a **tool call** shown whole in place of the five-line tail it
collapses to: `Enter` on an empty composer toggles the call the frame last showed (the
newest one it can open if the reader has touched none), and a click toggles the call
whose row it landed on. The entry keeps a bounded tail of its whole output — 400 lines,
the cap a **spill file** exists to lift — so a call that never spilled still opens, and
the collapsed row says what is behind it (`✓ exit 0 · 1.4s · 400 lines`). The body is
read through the transcript's own **window**, so a 400-line expansion is scrolled like any
other rows. **reasoning** opens on the same key; collapsed, it stays one row
(`⌁ reasoned`).

**completion** — the offer the **composer** makes above itself, one column and one set of
keys for two triggers. `@` offers workspace paths: a proximity-ranked background walk,
eight rows, `.git` skipped, `.gitignore` deliberately not honoured. `/` offers the
**command** catalogue, in the one place a command can be sent — a one-row draft that
starts with the slash, the name not yet ended by whitespace — ranked by the name being
typed with the names it starts first, and needing no walk because the catalogue is in
hand. `Tab` and `Enter` take the marked offer, `↑`/`↓` walk it, `Esc` dismisses it and
leaves the draft as typed; taking one closes the box and never sends.

**picture** — a markdown image the frame draws: a block of its own in the answer, drawn
only where the terminal speaks Kitty graphics, from a file in the workspace and never from
a URL, at most twenty rows — the face's own allowance — whole in the **window**
or not at all. A picture that
cannot be drawn is its **caption** with the reason in the `‹›`.

**caption** — the words and `‹src›` a **picture** is drawn under, `a chart ‹plot.png›`: the
alt text's words, the target in `‹›` as a link keeps its own, the reason in their place
when there is no picture. It is what the rows cost when nothing is drawn, and what a copy
of the block yields.

**export** — `/export [path]`: the **thread** as one markdown file, at the path named or
`sindri-<guid>.md` in the workspace, read out of the record so it says what `sindri
sessions show` prints. A file already there is not overwritten without a second ask.

**settings modal** — the `/settings` overlay: one row per key of the config schema.
`Enter` saves, `Esc` discards, `Ctrl-U` empties the field, `Space` flips a toggle. The
rows scroll with the cursor; the `file` line, the warning, a refused save's message and
the hints are pinned outside the scroll, so the row a save failed on is never the row
that fell off.

**binding** — one row of the **help overlay**: the keys, and the mouse gesture that is
their accelerator, that do one thing — and what that thing is. `BINDINGS` is the one
list, and nothing on it is reachable only with a pointer. The one row that is
not a binding is the **command** catalogue's: the commands share a row, because one
name there is five names.

**command** — one of the `/` names the **composer** sends: the name, the one line the
**help overlay** says of it, and what it opens. One catalogue (`COMMANDS`) holds them
all, in the order the frame says them; the parser, the line an unknown name is refused
with and the help's command row all read it, so a seventh command is added there and
nowhere else.

**help overlay** — `?`'s list of every **binding**: the keys in their
column, each description wrapped under them, the box sized from the wrapped rows. A
list longer than the box scrolls by a screenful and says where it is (`3 of 12`) and
which way there is more; `Esc` and `Enter` close it as they always did.

**from environment** — the API key field's default, and where emptying it returns: the
credential comes from `SINDRI_API_KEY`, the provider's variable, and nothing is written.

**door** — the frame's front door (`crates/cli/src/frame.rs`): the one place a run with
no credential opens the interface instead of exiting, with the **settings modal** staged
on the `api_key` row.

**card** — the failure drawn instead of a frame when a `doctor` check fails, holding the
check and the doctor's report whole: sized from the rows that report wrapped into, paged
with `PgUp`/`PgDn` when the screen is shorter, and left only by a key — a resize lays it
out again rather than dismissing it.