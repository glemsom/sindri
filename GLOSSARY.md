# Glossary

Sindri's vocabulary. Use these words exactly; do not drift to synonyms.

## Agent

**turn** — one round trip to the model: our request, its response, the tool results we
feed back. Bounded by the **turn cap** (`max_turns`, default 250).

**tool call** — one request from the model to run a tool — `bash`, `skill` or `open` —
plus the structured **tool result** returned. One `bash` call may run a whole pipeline or script.
The code's word for the request once its arguments are read is `Asked`: a variant per
tool, so a third tool is a compile error wherever a call is dispatched or written down.

**bash tool** — the one tool that runs anything. Runs a command string inside the
sandbox; the model composes shell tools through it rather than being handed one per job.

**open** — the third tool: one **target** handed to the host's opener, for the human to look
at. It runs nothing inside the sandbox — the core parses the target and refuses it, and the
one front end a human with a browser sits in front of is the only one that registers the tool
at all. A run with no window, **batch mode**, has no `open` in its schema: an absence, not a
refusal.

**target** — what one `open` call names, and exactly one of them: `url`, an `http://` or
`https://` address, or `path`, a file. The code's word for it is `Target`, and its two shapes are
`Url` and `Path`, because the rules that read them differ. Both are read in one place — a scheme
that is neither is refused there, and so is an extension no browser reads — so the opener is only
ever handed a target the core accepted. A `file:` URI is a second spelling of `path` and not a
second door: it is percent-decoded first and read for its authority afterwards, so an empty one and
`localhost` reduce to the same path and any other is refused naming it, and what is left is a `Path`
that runs the guard below and is
refused in the identical words — so it opens nothing a `path` could not, and the record cannot tell
which spelling was chosen. A `path` is then guarded against the **sandbox policy**'s
own sets and never a second list: it must lie inside the **writable set** or the **session temp**,
as given and as resolved, and must be a regular file, since the host hands it to whichever handler
its desktop registered. The guard is the `open` tool's, because it reads the policy, and it reads
the extension rule a second time on the resolved path, since that is the file handed over: the
path it hands over is the resolved absolute one, and it is the resolved file's extension the
allow-list holds, so a link cannot promise one file and deliver another. A refusal never names a
path outside what a run may read: `open` is not a way to ask the host what exists out there.

**started** — the word an `open` result carries when the host was asked and the opener had
not refused it when the quarter-second probe window closed; `failed:` with the opener's own
reason when it exited non-zero inside that window. Neither says a human looked: whether a
window drew is on the far side of a process boundary.

**answer** — the final assistant text of the last turn. The product. Markdown read into
the frame: prose, fences drawn as panels, links that keep their destinations, and **pictures**
with their **captions**.

**stopped** — why the loop ended: `final`, `max_turns`, `budget`, `cancelled` or `error`.
The set is the loop's and it is closed: `Stopped` holds those five and no sixth. The
**record** writes one word more — `running` — from the moment it starts, and it is the
record's word for a run still going, not a way the loop stops: a record still saying
`running` never finished.

**budget** — what one prompt's commands may spend before the run stops: an hour of
command time and 8 MiB of output fed back, spent inside a script where the **turn
cap** cannot count it. A **tool result** is re-sent with every later turn, so the bytes
counted are the bytes the conversation keeps; a run that reaches the budget stops as a
run that reaches the cap does, with whatever answer exists.

**cancel** — Ctrl-C (SIGINT): one flag the loop and the executor share. The loop stops
between turns, the command's process group is killed, stdout stays empty, exit is 130.
In the **frame** a cancel is answered the moment the key lands — the footer reads
`cancelling…` and the draft stays in the **composer** — and the state clears when the
run has actually stopped, because a request in flight is not interrupted.

## Sandbox

**sandbox policy** — the value holding writable set and temp mapping: the only source
of both enforcement and description.

**writable set** — the paths mounted read-write besides the session temp: the workspace
plus **extras**.

**extras** — user-configured extra writable paths.

**session temp** — per-run writable scratch, mapped to `TMPDIR` in the sandbox, and
mounted as `HOME` beneath it.

**read-only root** — `--ro-bind / /`: the host filesystem inside the sandbox read-only.
A boundary on writes, not on reads.

**description** — the model-facing text derived from the sandbox policy, also printed by
`sindri doctor`. One sentence may be appended to it that the policy does not derive: the
**pointer** to the **catalog**, added only where the model may see one.

**pointer** — the one sentence appended to the **description** saying this environment's
instructions are behind `skill`, so a model reading the schema it reaches for first learns
the **catalog** is there. Not a **skill** body's and not the **preamble**'s: the preamble is
re-sent on every turn of every run, and the pointer is paid for only where there is a
catalog the model may see. The record is `docs/adr/0004-built-in-skills.md` §3.

**yolo-unsafe** — `--yolo-unsafe`: the policy whose enforcement is `none`, so commands run
with the caller's own privileges, outside bubblewrap, inheriting its environment and
working directory. Never persisted: asked for by the invocation that makes it.

**doctor** — `sindri doctor`: verifies bwrap, the binary set and the mount plan, prints
the description, and lists the **skill** **catalog**.

## Run

**boot** — turning the flags and the process's own facts into a session ready to start,
and the one line a front end says when it cannot. Not a synonym for **session**: a boot
ends where a session begins.

**instructions** — a repository's own house rules: the `AGENTS.md` at its root, found
through the workspace's git root — or the workspace itself where that is no repository —
and read once at **boot**. They travel beside the **preamble** and never as a turn of
their own, so the **transcript** stays the user's words and the preamble's measured words
are the preamble's: the read is capped by a byte count stated in one place, and a file
past it arrives cut with a marker rather than silently short. A repository with no
`AGENTS.md` says nothing and adds nothing; one that cannot be read is a diagnostic naming
the file and the reason, and the run still starts. The record's start block, `-v` and
`sindri doctor` name the file and the bytes handed over, in one sentence. The code's word
for the file is `Taken`, and its three answers are `Absent`, `Found` and `Unreadable`.

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
the reader to the tail. Reading is not following: `Enter` on a call, or a click on its
row, is a place too — the window stops following and that entry's own row becomes the top
of the column, so the output the fold opens grows beneath it — and a hold is the reader's
until a key takes the tail, so a column the thread is too short to fill does not hand them
back to it. `End` takes the tail again, `Home` the first turn, and nothing in the last
column claims to say where the window sits: the **new-rows badge** is the frame's only
word on it.

**new-rows badge** — `↓ 4 new rows`, against the footer's right edge: how many rows
arrived below a **window** that is off the **tail**. Only what arrived after the reader
took their place counts: going back, reaching the first turn, opening a call and folding
one all take the rows below the window as read, so the count starts again at what is below
it then and never says that more arrived than is there. It says `1 new row` for one,
clears the moment the window reaches the tail again, and its room is taken before the
footer's hints are chosen.

**tail** — the newest row of the **transcript**, and the place a **window** follows
from unless a reader has scrolled back or engaged with a call; `End` is the key that
takes it again.

**spill file** — where an over-long tool result's full text is written when the
model-facing copy is truncated head+tail.

**batch mode** — `-b`: the Unix-filter front end. stdout is the answer only; stderr is
silent unless `-v`, but for a fault the run cannot leave unsaid — a repository config it
refused, an `AGENTS.md` it could not read; `--format json` changes the output shape.

**reasoning** — provider-side thinking tokens, off by default, shown on stderr with `-v`.

## Skills

**skill** — a directory holding a `SKILL.md`, or a **built-in skill** with no directory at
all: the frontmatter's `name` and `description`, the two flags that say who may reach for it,
and the body beneath the fence that **activation** returns. A skill no version of the
specification knows how to read is a **refusal**, never a silence.

**description ceiling** — the character budget a **skill**'s `description` may spend, read
as characters and not bytes: the catalog sits in the `skill` tool's description, so every
description is paid for on every turn of every run that has a skill, and a description is a
routing key — the task it covers and where the model's default is wrong — rather than a
summary of the body. A description past it is a **refusal** naming the ceiling, what the
description measured, and the line to cut, never a truncation: a truncated routing key is a
skill that silently stops being found. The code's word for it is `DESCRIPTION_CEILING`, and
every **built-in skill** is tested against it so none can refuse itself at runtime.

**skill scope** — one of the three places a **catalog** reads, and the three are ordered by
nearness: `builtin`, compiled into the binary; `user`, under the home directory at
`~/.agents/skills`; and `repo`, at every `.agents/skills` between the workspace and its
repository root. The specification governs what is inside a skill directory and says nothing
about where one lives, so these paths are ours. The code's word for it is `Scope`, and its
three values are `Builtin`, `User` and `Repo`, in that order of nearness.

**built-in skill** — a **skill** shipped inside the binary rather than installed to a path:
its `SKILL.md` is compiled in, so there is no directory to be absent, stale or unwritable,
and nothing an upgrade can leave behind as the previous release's instructions. It is the
lowest of the three **skill scopes**, so a `user` skill of the same name shadows it and a
`repo` skill shadows them both: a user who disagrees with a built-in drops their own skill of
the same name in `~/.agents/skills` and it wins. Its **scope** is a field and not an
inference, so `sindri doctor` prints it beside the skill's name — with `(compiled in)` where
a scanned skill's directory would go, there being no directory to print — and names the
built-in a nearer skill displaced. It is loaded once per **session** like any other skill, through
the `skill` tool and no other way. The record is `docs/adr/0004-built-in-skills.md`.

**activation** — one `skill` call's result: the body with the frontmatter stripped, in a
named `<skill_content>` wrapper, with the skill's directory and its file listing beside
it so a relative reference in the body resolves. A **built-in skill** has no directory to
name, so its files are sent with the body instead of left as references into a tree the
binary never wrote. Once per **session** and no more: a
second call is `already_loaded` with no body, because a **tool result** is sent again
with every later turn, and the loaded set is cleared with the history it was made in.
`/skill:name` is the same activation asked for by a human instead of by the model —
`Agent::activate` — so it is one call and not a second way of loading something: the
claim is made in the conversation's own set, and the model's later call for the same
name is `already_loaded`.

**loaded set** — the skills one conversation has been told, held with the **history** and
cleared with it: a body is in force while the transcript still carries it. Two identical
calls in one turn's wave are one load, because the wave is one turn, and a load by hand
claims the name the same way the tool's own call does. The code's word for it is
`Loaded`, and `Agent::loaded_skills` is how a frame reads it.

**catalog** — everything one scan found: the **skills**, the **refusals**, and the
**shadowed** names. Sorted by name, so two scans of one tree say the same thing.

**refusal** — a `SKILL.md` the scan would not load, held with its file and the line that
broke it: no description, no YAML, a description past the **description ceiling**, a link
to nothing. A refusal is a diagnostic, not a
failure — the rest of the **catalog** still holds.

**user-only skill** — one the model may not see: `disable-model-invocation: true`. It is
absent from the **catalog** the `skill` tool carries and from that tool's schema, and a
human reaches it from the **composer** alone. A scan still found it, so `sindri doctor`
still prints it.

**model-only skill** — one a human may not type: `user-invocable: false`. It is in the
catalog the tool carries and callable, and absent from completion. A skill that says both
is one nobody may run, and the report says `none` for it.

**shadowing** — one name two **skill scopes** carry. The nearer one's skill holds it and
the farther one is recorded as shadowed, by its scope, its name and its directory where it has
one, so a reader sees both what won and what it displaced: a repository's skill over the home
directory's, a crate's over its repository's, and both over a built-in's, which is the lowest
and is named by its origin rather than by a path it has not.

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
through, so the bands of it with a colour behind them are the rows its **chip**s sit
on. What
a row copies is the fact — `context 38%` — never the leader that drew it.
The margin between the words and the rail stays canvas, and on a terminal that speaks
Kitty graphics it carries the seam's light strip (`seam.rs`): from the top of the body
down to the **composer**'s last row, brightest against the rail and dissolving toward the
words until it leaves at the footer, and two cells wide because a picture's escape costs
the cell after it and that cell has to be the strip's own, never the rail's first
(`render::GAP_COLUMNS`). The margin is canvas beside the panel for the panel's whole
height, so the light runs beside the composer, and the brightest end of its fade is the
corner the composer's border walk brightens.
32 columns from 100 up; below that, the same facts in the same order as one line under
the transcript, inset the same two cells, which never loses the posture, the model or the
workspace. One priority order (`rail::priority`) ends with the posture, and the column
and the strip both read it: a column too short for its facts gives them up from the bottom
of that order — the identity block first, since the first frame already names it — and
keeps one row to say how many went (`+7 more`), and a fact too long for its room is
shortened rather than skipped — a posture to `⬢ bwrap` — and dropped only when nothing of
it can be said. The GUID is copied with `Ctrl-G`, or by clicking its row.

**mark** — the cells a drag lights and the words that leave with them: a wash on the rows
a copy takes, live from press to release and standing after it — the first row from the
column the drag started on, the last up to the column it ended on, the rows between in
full, and a wide glyph at either edge whole. One **ink** of the theme is painted and only
as a surface, so the ink under it — syntax, a heading, a panel's border — still reads as
itself while the row is marked. One function computes the spans a drag covers and both the
mark and the copy read it (`mark::span`), so the mark can never light a cell the copy does
not take nor leave dark a cell it does — but for a **picture**'s block, which is marked as
one block: a drag that reaches any row of it lights the graphic's rows and its caption
together — however deep in the block the drag began or ended, and whichever rows of it the
**window** holds — and the block's words are taken once, on the row the block starts at. A
block is the one thing the copy differs from the screen in, and marking it whole is what
keeps that honest: the reader sees a whole thing taken, not a run of blank cells. A plain
click lights nothing, a click that opens a spill row or a call's row starts no mark, and
nothing is painted behind an overlay that covers the **frame**. The mark outlives the
release: it stands on the rows it took — a place in the **thread** and not on the glass,
so a note landing under it moves the mark with the text — until the
reader's next gesture, a key, a click, the wheel or an overlay taking the frame, which
clears it and copies nothing further; `Ctrl-Y` while it stands copies those rows again
rather than the last **answer**, and `Shift`+`↑`/`↓` replace it with a mark of their own,
grown a row at a time up from the last row the **window** holds, and shrunk back the same
way. A mark whose rows the **window** no longer holds has nothing left to stand on, and
goes rather than copy the rows that took its place. A release that covered no text copies
nothing, says nothing, and leaves no mark.

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
walked cell to the segments exactly as it holds a flat one to `PALETTE`. A picture's
pixels are held the same way, one at a time, by `theme::owns_pixel`: every pixel is a
step on a segment whatever its alpha, because alpha is not a hiding place and a strip's
edge antialiases in alpha alone. On terminals that speak no Kitty graphics it is the only
light the frame has, the seam's strip being absent there.

**surface** — one of the three tiers of nearness the frame paints on:
**canvas**, **panel**, **raised**, in that order. A renderer asks
`theme::canvas()`, `theme::surface()` or `theme::raised()`, and the frame fills the
canvas from `theme::canvas()` before the layout, so no renderer writes a background of
its own: `theme.rs` names every colour the frame paints. A renderer that *places* a
picture — the seam's strip, the face — paints the canvas under it first, because a
pixel the picture left transparent shows the cell behind it rather than the
terminal's. One dark theme, three tiers.

**ink** — the foreground a word is drawn in, and the theme's as surely as a **surface**
is: `theme::plain()` is body ink — prose, a prompt, a call's command, a **table**'s
cells — and every other ink is a named colour or a **walk**. A surface names the ink it
asks for, because a cell emptied by `Clear` has no span to bring one. Nothing in the
frame is left at `Color::Reset`: that is the terminal's default foreground, a reader may
set it to red, and `theme::owns` refuses it. `tests/theme/cells.rs` holds every cell of a
painted frame to an ink and a surface the theme names.

**canvas** — the lowest **surface**, and the theme's own: the frame fills `frame.area()`
with it before the first region is drawn, so a resize, a scrolled region, a card or a
gap a picture's escape leaves behind is still the theme's and never the terminal's — a
light terminal or one with a background image would otherwise change the frame as
surely as a palette would. A code word in prose is drawn straight on it, with no wash,
since a wash is a patch and the canvas has nothing quieter than itself. Above it:
**panel**, then **raised**.

**panel** — the **surface** the frame draws on, one tier above the canvas: the
composer, a fenced code block, the **queue panel**, the band a turn's **prompt**
sits on. The rail is not one: it is chrome, drawn on the canvas.

**raised** — the **surface** an overlay is drawn on, one tier above a panel; the frame
is dimmed beneath it. The rail's sandbox **chip** sits on it too, a tier above the
canvas the rest of the rail is drawn on.

**role** — one of the seven inks a **fence** is painted in — `PLAIN`, `COMMENT`,
`KEYWORD`, `FUNCTION`, `TYPE`, `NUMBER`, `STRING` — each a colour `theme.rs` names and
each clearing 4.5:1 on the **panel** a fence sits on, because code is read as body text.
Which scope is which role is a fact about languages and is decided in
`markdown/code.rs`; which hex a role is painted in is a fact about the theme and lives in
`theme.rs`, so a fence's cell is the theme's like any other and a language no rule knows
is `PLAIN` rather than wrong.

**chip** — the rail's two statements that are not label-and-value pairs, drawn as
**raised** boxes on the edge the values hang off: the sandbox posture, `⬢ bwrap · 1
writable`, and — only when there is one — the **skills** in force, `◆ house-style`. A
chip is shortened to its head when its column runs out (`⬢ bwrap`, `◆ house-style`), and
the posture's ink is the boundary's state: the first frame's posture line wears the same
chip, `GOOD` while the run is inside `bwrap`, `BAD` once it has left the sandbox
(`--yolo-unsafe`), whose posture begins `unsafe`. The skills chip is the accent instead,
because what is in force is the frame pointing at something rather than a boundary's
state, and it goes when a **session** starts over.

**composer** — the input at the bottom: 2–6 rows, `Shift+Enter` for a newline, `@` for a
path completion, `/` for a command. With no text in it at all it carries the placeholder
`Ask, or drop a log here · @ path · / command`, which a draft — a space is a draft —
never sits behind. It is laid out in the **transcript**'s column and not across the
terminal, so the panel ends where the prose does: its right border sits one cell off the
seam's light strip and no cell of the **rail**'s column is the panel's — the footer, a
strip of hints rather than a panel, is the one row that still runs under the rail. Its
border is a **walk**: `PROMPT` while it holds the keyboard and
`FAINT` while a run has it, walked toward `SEAM` across the top and bottom rules so the
panel is lit from the rail's side and its right-hand corners take the rail's own ink —
which is the light the strip sets the walk beside.

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
hand. Once `/skill:` or `/skill ` is typed the offers are **skill** names rather than
commands, ranked by that one function and no walk either: a name the user may not invoke
was never in the list the frame was handed, so it cannot be offered, while a user-only
skill is offered like any other because a human is who it is for. `Tab` and `Enter` take
the marked offer, `↑`/`↓` walk it, `Esc` dismisses it and leaves the draft as typed;
taking one closes the box and never sends, and it is taken in the spelling the draft is
already in.

**block** — the rows a **picture** takes of an answer, and the one place a copy
differs from the screen: its graphic's rows together with its **caption**, or with
the `‹›` that says what it could not be drawn as. A drag that reaches any row of
one takes it whole, and every row of it the **window** holds is marked, however
deep in it the drag began or ended; its words are taken once, on the row the block
starts at — the **caption**, or a **diagram**'s fence body (`Viewport::block` in
`crates/tui/src/selection.rs`).

**picture** — a markdown image the frame draws: a block of its own in the answer, drawn
only where the terminal speaks Kitty graphics, from a file in the workspace and never from
a URL, at most twenty rows — the face's own allowance — whole in the **window**
or not at all, and a **diagram** — a picture whose source is a fence's body — twice that,
forty, because it is drawn top to bottom and its height carries its words: a block whose
graphic asks for more rows than the **window** holds is drawn nowhere and says so on its
first row, and one the reader merely scrolled past says nothing. A picture that
cannot be drawn is its **caption** with the reason in the `‹›`. A source is a file or, for
a fence marked `mermaid`, the fence's own body: mermaid is a second kind of source for the
same store and not a second pipeline, so a diagram is measured, laid out and
**captioned** exactly as an image is. Where a picture is not a block — a table cell, a
list item, a heading — a mermaid fence is the **panel** it was, and a fence that will not
parse is that panel with the parser's own words under it, in the ink a **caption** keeps.
A fence the stream has not closed is that panel too: a layout is tens of milliseconds,
and a body that another delta will lengthen is not yet one to draw. A fence that has closed
is that panel for a tick while the **frame** asks for its **drawing** — the svg the
renderer writes for a body, made off the frame's own thread and handed back through the
loop's channel, because no frame may wait for a layout — and the diagram then takes the
panel's rows, the entry it landed in measured again. A block waiting for its drawing is a
panel and nothing else: "not drawn yet" is silence, and a refusal is a sentence the caption
prints.

**caption** — the words and `‹src›` a **picture** is drawn under, `a chart ‹plot.png›`: the
alt text's words, the destination in `‹›` as a link keeps its own, the reason in their place
when there is no picture. A mermaid fence has no destination and its source is not printed, so
its caption is the fence's own name, `‹mermaid›`. It is what the rows cost when nothing is
drawn, and what a copy of an image's block yields. A **diagram**'s block copies as the
fence body it was drawn from instead, because `‹mermaid›` is a name and not a diagram: a
reader who pastes one out of the frame gets mermaid. The diagram does not open in place
for it — an **expanded call** opens an entry, and a diagram is a block inside an answer's
(`crates/tui/src/expand.rs`) — so the block's source is what the frame owes a reader who
wants it, and the copy is where they get it.

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
nowhere else. `/skill` is the one name that takes an argument from a fixed set and the
one a colon may end — `/skill:house-style` is `/skill house-style` — and the rest of the
line after the name is the prompt the skill is put to work on: a bare name arms it and
starts no turn, a tail runs as one.

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

**ablation** — one **built-in skill** measured against its own absence: two builds of the
binary that differ only in the built-in table, one **arm** each — `with`, the built-in in the
**catalog**, and `without`, the table emptied — run on one task shaped like the capability the
body claims and scored by a rule rather than by eye. What the line costs is paid by every run
and what the body costs is paid after a load, so an ablation decides whether the body changes
the outcome for less than the line is worth; the `activated` field is what tells a built-in no
run reached from one that was reached and did not help. The machine is
`scripts/bench-skill-ablation`, the reading is `docs/skill-ablation.md`.
