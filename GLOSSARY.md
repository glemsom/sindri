# Sindri

An AI agent that is a Unix filter and a terminal UI over one core. Most of these words exist
because an ordinary one would blur two things this project keeps apart.

## The run

**Launch**:
One opening of the front end: the session it holds, the boundary it enforces, and everything a run
is opened with. `/new` ends the conversation without ending the launch, and a batch run is a launch
of one prompt.
_Avoid_: Invocation, startup, instance, process

**Run**:
One invocation of the agent loop for one prompt, from the first request to a stop reason.
_Avoid_: Job, task, query, conversation

**Conversation**:
The prompts one launch has already had answered, each beside the answer it got, and what the run that
joins next is asked after: it outlives the run that filled it and the launch that ends takes it away,
and `/new` empties it. A queued prompt joins it at the turn it was asked, and a run that stopped on
anything but a final answer still joins with what it ended on — except one `/new` superseded, whose
words went with the conversation it emptied.
_Avoid_: Chat, thread, memory, context, history (that one is a run's own turns)

**Exchange**:
The band's unit: the prompt wearing the sigil it was asked with, what the run did under it, and the
answer. That work is no row of the band — the thoughts, the calls, the results that came back and the
answers it went past each standing as its own row while the run fills it — and one row once the
exchange has ended, counting them: `⋯ 12 calls · 3 files · 4.1 s`. That row is the reader's to open,
and a result inside an opened work keeps the fold of its own.
_Avoid_: Pair, round, item, Activity, Trace, Log

**Prompt**:
The reader's own words a run was asked with: the first fact of the session's transcript, and what the
wire is asked last, after the turns that came before it.
_Avoid_: Query, question, input

**Turn**:
One request to the model and the tool calls it asks for. Not a unit of progress, and no row of the
band: the record writes one for every turn the run took and the drawing leaves it out.
_Avoid_: Step, iteration, wave

**Answer**:
The text a run produces for stdout — the only thing on stdout in batch mode.
_Avoid_: Response, output, result

**Stop**:
The reason a run ended — `final`, `max_turns`, `budget`, `cancelled` or `error` — in its own word. It
is laid in the transcript where the run ended, and it earns a row there where it says something the
answer above it does not: `cancelled`, `max_turns`, `budget` or an error, the error with one line of
the run's own words about it, redacted as every other fact is, and a cancellation with no line it
does not have. A run that ended on `final` draws no row, because the answer under the marker is the
whole of what it ended on. The marker at the head of the newest answer settles on the same word, so
no two drawings of one stop disagree.
_Avoid_: Exit status, failure, termination, reason

**State**:
Whether a run is running or the stop it ended on, in one word: `running` while it is, and the stop's
own word once it ends. The rail's first row is the state — under the frame's marker while a run is
live, and in the colour that stop earns — and the card's stop rows say the same word beside the exit
it left.
_Avoid_: Status, phase, progress

**Refusal**:
Why no run could start — no credential, a sandbox that will not build, a config the schema refuses. A
run's refusal is drawn in the transcript as the entry a stop is drawn as, in the refusal's own words;
the prompt it was asked with is the reader's again, and the launch, its conversation and its queue
stand. One no frame could survive is a line on the reader's shell and the end of the launch.
_Avoid_: Error, failure, exit, abort

**Context window**:
How many tokens the model's window holds, when a reader names it in the config. No wire reports
one and nothing here guesses, so a run with none named has a size and no share of the window.
_Avoid_: Context limit, token budget, max context

**Status**:
The document a session publishes about its run: the phase, one redacted line of the run's own words,
the turn the run is on, and the workspace's name. The line is never the command, the prompt or the
answer. One is appended to `status.json` beside the transcript as the run goes, so `status.json`
reads back in order, and the same document is reported off the terminal, so a listener outside the
frame reads it too. A report about a run held by a session, and never the run's own word about
itself, which is **State**'s alone.
_Avoid_: Progress, notice, indicator

**Phase**:
One of the four facts a **Status** holds, in one of five words: `idle`, `working`, `done`, `blocked`
or `error`. The five are the document's words.
_Avoid_: Status, state, step

## Confinement

**Session temp**:
Per-session scratch mapped to `TMPDIR`, alive as long as the session, unseen by any other run.
_Avoid_: Tmpdir, scratch dir, work dir

**Ground**:
Where a run stands when it judges a target: the writable set, the session temp and the directory a
relative path is read from, held resolved and sorted. The frame asks the same ground before it draws
a path a call names as a way to the file, so a reader is never handed a target the run would refuse.
_Avoid_: Context, environment, base path, root

## Words from the wire

**Wire**:
A provider dialect: `anthropic`, `openai`, `copilot`, and exactly those words.
_Avoid_: Provider, backend, API

**Endpoint**:
A wire at the address it is asked and on the route it serves: the one value a run and a listing are
both built from, so the names a listing serves are read from the endpoint a run asks.
_Avoid_: Provider, host, client

**Signing**:
The sign-in a wire that has no key of its own is asked with: the store the **Credential** is read
from, or the words saying why there is none. One value covers both, so a run and a listing cannot
disagree about whether there is a sign-in at all.
_Avoid_: Credential (that one is what a wire is asked with), token, account

**Credential**:
The credentials a wire is asked with: the key a config or the environment holds, the sign-in a wire
with no key of its own is read from, or nothing at all.
_Avoid_: Token, secret, API key, auth

**Served name**:
A model identifier read off the wire, never invented, kept beside the endpoint that produced it.
_Avoid_: Model list, catalog, available models

**Route**:
Which endpoint an OpenAI-shaped wire serves — `chat` or `responses` — by the served name,
overridable per run.
_Avoid_: Endpoint, API, mode, protocol

## Sessions and skills

**Session**:
The directory one recorded conversation is kept in, named by its GUID: session document, transcript,
status documents, temp. A launch in the terminal holds one for as long as it holds a conversation, so
its record grows with every prompt and its temp goes when the launch ends; a batch run is one
conversation of one prompt, and so one session.
_Avoid_: Conversation, chat, history

**Skill**:
A directory holding a `SKILL.md` whose body the model can load by name, in one of three scopes.
_Avoid_: Prompt, plugin, recipe, tool

**Loaded-skill set**:
The names loaded so far, in load order; `/skill:name` and the `skill` call both write to it, and
`/new` empties it.
_Avoid_: Conversation state, skill cache, loaded prompt

**Skill scope**:
Where a skill came from, lowest precedence first: `builtin`, `user`, `repo`.
_Avoid_: Source, origin, level

## The shape

**Core**:
The part of Sindri that makes every decision, and that has no terminal.
_Avoid_: Engine, library, backend

**Front end**:
A way of driving the core — the TUI or batch mode; owns no decision the core could make.
_Avoid_: UI, client, interface

**Sindri's face**:
The picture of Sindri that the `sindri` binary carries beside the bundled font, measured from its
own header into a footprint and drawn over those cells — decoration, so a terminal that places
no picture is given no cells and no words for it. It stands over the top rows of the rail's
column where the frame has room for one, and nowhere else. A reader turns it off in the config, and a
frame with it off reserves no row: the facts stand from the rail's first row.
_Avoid_: Logo, avatar, mascot, icon

**Band**:
A strip of rows the frame shares out — the transcript's, the **Composer**'s, the row that says the
frame stands off the tail, and the footer's — each drawn in the rows it was given and never past
them. A queued prompt has no band of its own: it stands in the composer's, under the draft, one
prompt to a row and the newest at the foot. What a band cannot hold it counts: the rows of a draft
past the composer's ceiling, and the prompts of a queue past the rows it was given. A fence is not
counted that way: it is kept in the shape it was written in, so a band never reflows one — the words
of a fence line past the band's edge are dropped, that row's last cell says more stands there, and a
reader walks the fence sideways for the rest. The **Caret** is no cell of a band.
_Avoid_: Region, pane, panel, viewport

**Rail**:
The column beside the transcript that carries the frame's facts: a column down the frame's
right-hand edge where the width holds one, and one row along the bottom of the transcript's band
where it does not.
_Avoid_: Sidebar, side panel, gutter, fact bar

**Measure**:
The three facts the rows of a band are laid at: its width, the surface a picture in an answer is
measured against, and the **Ground** a call's path is judged against. Any one of the three moving drops
every row laid, and only the width moving is a reflow — a narrower band lays the same history out in
more rows, and that is no row landing.
_Avoid_: Layout, size, metrics, measurement

**Plane**:
A band's surface drawn as one image placed under its cells, so it shades in pixels rather than
in whole rows: placed once per size, carrying an identity of its own so one is taken away and the
other stands, and handed back with the pass that drew it. The rail's plane reaches back over the
transcript's last columns, so the rail comes out of the transcript's edge rather than being parted
from it: no line is drawn there, the split costs neither band a column, and the words on both sides
are drawn unshaded. A terminal that places no picture is given none, and so is either band in a
palette that draws no colour, where each band's own rows are the whole of its shading; with no
picture nothing widens, and the lit column down the rail's boundary and the shadow across its last
row are what that frame draws.
_Avoid_: Ground (that one is where a run stands), backdrop, background, layer

**Footprint**:
The columns by rows a picture is fitted into under a ceiling: one scale on both axes, so the
picture's shape is kept — shrunk until it fits and never grown.
_Avoid_: Dimensions, bounding box, image size

**Alert**:
A quote whose first line names a kind — `NOTE`, `TIP`, `IMPORTANT`, `WARNING` or `CAUTION` — and
that draws that kind as a label: the name on a row of its own, in a colour of its own, with the
quote's words under it in the same column. A quote whose first line names no kind is a quote and
nothing more, marker-like words and all.
_Avoid_: Callout, admonition, banner, badge, note (that one is a kind)

**Source**:
The words a fence was written with, line for line, kept beside the rows the frame draws them as: a
copy of a marked fence hands the source back whole — the line breaks and the indentation it was
written with, and nothing the band drew into it — where the cells are what the band made of it. A
fence's tag row is no line of its source, and a mark on it takes the fence whole all the same.
_Avoid_: Body, content, raw text

**Composer**:
The draft a reader types: its words, the place the next glyph lands among them, the scroll they are
read through, and the columns a frame has measured for them. The columns are measured before the
band is placed, so a draft wraps at the width it is drawn at.
_Avoid_: Text box, input box, editor, entry field

**Draft count**:
What Enter would send, counted on the far corner of the **Composer**'s band: the files the draft's
words name, the command it opens with, and the characters it holds. It is a count of the draft and
never of the run's facts, it follows every edit, and a frame too narrow for the whole of it draws
none of it — the rule keeps the row.
_Avoid_: Tokens, stats, progress

**Layout**:
Where one pass put each part of the frame: the bands it shared the rows into, the columns the
transcript was given beside a rail, and the body a card was given when one was drawn. A press is read
off it rather than laid out a second time, so the frame a reader was shown is the frame a press is
answered from.
_Avoid_: Geometry, arrangement, tiling, regions

**Footer**:
The frame's last row: the hints of what a key does that this frame can honour, most valuable first,
each giving up its tail as the row narrows, and a key the frame cannot answer not named at all. An
idle draft names sending, the commands, the files and the keys; a run in flight names cancelling and
never sending; and a queue behind the draft is counted where the key that recalls one stands.
_Avoid_: Status bar, help line, key bar

**Card**:
The frame a body of rows is drawn on: a rounded border carrying the name of what it shows in its top
row and the key that leaves in its bottom, at most seventy-two columns wide and as tall as the rows
the body holds, centred — a body taller than the frame is capped, and the card counts the rows it
keeps back — and under fifty columns the whole frame with its title row and no border.
_Avoid_: Modal, dialog, popup, overlay, sheet

**List**:
The shape a card's rows share: the query they answer at the head, the one line they say in place of
their rows, and one row per item with the row a reader is on marked — filled down the band the card
hands it and cut at its frame's edge. A card that shows one names its own rows, its own mark and
its own line, so the list knows neither files nor served names.
_Avoid_: Table, menu, picker (that one is the Path picker)

**Path picker**:
The card `@` opens: the workspace's files as `git` reads them, ranked by how the query sits in one —
a name it starts, a name it holds, a path it holds — and nearest first within a rank. The card draws
the first eight as the file's name with the directory it sits in set quieter beside it, and one row
under them saying which matches it shows and how many the query left.
_Avoid_: File browser, autocomplete, fuzzy finder, file search

**Commands card**:
The card a draft beginning with a slash draws: every command with the one line that says what it
does, ranked by how the word after the slash sits in a name — a name it starts, then a name it holds —
and the row the draft's word names the row marked, `↑`, `↓` and a press walking the mark to another
row. `Esc` closes it and keeps the words, a word no command holds is a draft again, and the one
command that takes a word after it is written into the draft for the reader to name.
_Avoid_: Palette, slash menu, command list, autocomplete

**Splash screen**:
The frame a launch opens on: the wordmark, one line saying what the product is, and three commands
with the line that explains each, set as one block and centred in the band an answer is drawn in,
with the rail of the facts the launch already holds beside it — the wire, the model, the
workspace, its branch and the enforcement, and an em dash for the session and the temp a run has yet
to fill — and nothing over it. In a frame too narrow for a rail's column it is drawn with no rail.
One paint pass, and never again.
_Avoid_: Front door, welcome screen, landing page, boot animation

**Key protocol**:
What a launch asks of its terminal when it takes the screen, so a key that names no glyph arrives as
a report of itself rather than as a byte: `Shift+Enter` breaks a line where `Enter` sends one. It is
pushed on the terminal's own stack and popped first on every way out, and a terminal that speaks none
is handed the same frame and the same keys.
_Avoid_: Keyboard enhancement, CSI u, key encoding

**Pointer**:
The wheel's notch and the cell a press landed on. A launch asks whether its terminal reports one when
it takes the screen, and a terminal nobody can vouch for is handed a frame that takes none. Where it
is taken, one notch moves the transcript as the page keys do, a press on a card's row chooses that
row and sends the key it hangs from, and a press on no row of a card is no act at all. A pointer
is never drawn into a cell.
_Avoid_: Mouse, cursor (that one is the Caret), hover

**Caret**:
The terminal's own cursor, and never a cell of a frame: placed in the cell the draft waits in
wherever a reader can type there, and in the query row of a card that draws one, past the last glyph
typed at it and kept to the last column of the card's body where the query is cut there. A card that
draws no query row places none, and no answer and no prompt carries one. Its shape, colour and blink
are the reader's own settings, and no frame says them.
_Avoid_: Block (a caret is no cell), insertion point

**Terminal capabilities**:
What the terminal a frame draws on can do — the cell it measures a picture against, the colours it
holds, whether it draws text at more than one cell, whether it reports a **Pointer**, whether it
places pictures, whether this window is the one being read, which the code calls `Focus`, and whether
**Sindri's face** is drawn over the rail. Probed once before the first frame and handed to the frame
whole, and the focus is asked for when the screen is taken, reported again as the window is left and
returned to, and given up on the way out. It is one fact the frame draws from rather than a
behaviour: the accent edge recedes while nobody is reading and returns when they are. A terminal that
reports nothing is drawn in the cells one that reports everything is, minus the facts it does not
have — and one that reports nothing of the focus is being read.
_Avoid_: Terminal features, environment, detection, caps

**Frame facts**:
The workspace and the ground it is judged against, the home the skills are read from, the served name,
the run's record, the state the run is in, the facts the rail lays, the marker at the head of the
answer, and the focus the terminal reported, gathered as one value and handed to the frame whole. The
value is also which frame is drawn: the **Splash screen** until a prompt has been asked for —
holding what the launch already knows and no run's fact, so no fact that lands ends it — and that
run's frame from the prompt on.
_Avoid_: View model, props, session state, context

**Transcript**:
The record a frame draws — the entries a launch has written — with the row the reader stands at in it.
One of the three values a frame is built from, beside **Terminal capabilities** and the **Frame
facts** it draws: a launch hands over the tail, every entry the record writes after it is handed with
those facts and drawn once, and a reader moves the row.
_Avoid_: Log, scrollback, history (that one is a run's own turns)
