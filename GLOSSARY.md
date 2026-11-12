# Sindri

An AI agent that is a Unix filter and a terminal UI over one core. Most of these words exist
because an ordinary one would blur two things this project keeps apart.

## The run

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

**Prompt**:
The reader's own words a run was asked with: the first fact of the session's transcript, and what the
wire is asked last, after the turns that came before it.
_Avoid_: Query, question, input, message (that one is the wire's)

**Turn**:
One request to the model and the tool calls it asks for. Not a unit of progress.
_Avoid_: Step, iteration, wave

**Answer**:
The text a run produces for stdout — the only thing on stdout in batch mode.
_Avoid_: Response, output, result

**Context window**:
How many tokens the model's window holds, when a reader names it in the config. No wire reports
one and nothing here guesses, so a run with none named has a size and no share of it.
_Avoid_: Context limit, token budget, max context

**Exit**:
How one call ended: a code, the timeout that killed it, the user's word, or a signal. A fact of
the result entry.
_Avoid_: Verdict (that one is the wire's), status, outcome, result

**Status**:
`idle`, `working`, `done`, `blocked`, `error`, with one coarse line of the run's own words — never
the command, the prompt or the answer.
_Avoid_: Progress, notice, indicator

## The facts

**Ledger**:
The one module that holds a run's **facts**, the one way each **value** is spelled, and the rows the
facts are laid out in — the size in tokens, the pair whose halves line up on a slash, the meter a
share fills and the ink it is read in, the cut of a value to the room its row leaves it — the head of
a name, the tail of a path, the whole of a word — the em dash a fact is owed, the first eight of a
GUID, the blocks and the order they are read in, the hairline between two that both kept a fact, the
order facts are given up in, the row that counts what went, and the columns a **stats screen**
widens them to. No drawing spells a fact for itself, and no drawing decides where one goes: a fact
reads one way wherever it is said.
_Avoid_: Facts module, layout module, status bar, sidebar

**Rail**:
The ledger's adapter to a frame: a band and the shape it is drawn in, the rows the ledger lays there
drawn, and no rule of its own about which fact goes where.
_Avoid_: Status bar, sidebar, panel, dock

**Strip**:
The shape the rail draws below a hundred columns: the ledger's row along a band's bottom row, read
left to right with a gap between two facts and its count at the end. The other shape the rail draws
is the ledger's own column, beside the transcript; the **stats screen** is the ledger widened.
_Avoid_: Status line, footer bar, ticker

**Fact**:
One named thing a run holds, drawn as a label and a **value**: the wire, the served name, the route,
thinking, the turns, the window, the cache, the spend, the workspace, the branch, the commit, the
skills, the sandbox, the state a run stopped in and its instants, the session, the temp. A fact
nobody can state is absent rather than blank, a fact a launch names and a run has yet to fill is an
em dash, and a value that is its own name — a loaded skill — hangs where its siblings' values do.
_Avoid_: Row, field, item, stat

**Value**:
The glyphs one fact is drawn as, the meter beside a share of a window, and the ink it is read in.
_Avoid_: Text, string, cell

## Confinement

**Writable set**:
The canonical, absolute, distinct, non-nested paths a run may write, never `/`.
_Avoid_: Allowed paths, whitelist, mounts

**Session temp**:
Per-session scratch mapped to `TMPDIR`, alive as long as the session, unseen by any other run.
_Avoid_: Tmpdir, scratch dir, work dir

## Words from the wire

**Wire**:
A provider dialect and its endpoint configuration: `anthropic`, `openai`, `copilot`, and exactly
those words.
_Avoid_: Provider, backend, API

**Served name**:
A model identifier read off the wire, never invented, kept beside the `(wire, base_url)` that
produced it.
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
clearing the history empties it.
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

**Ink**:
A colour role asked for by name — dim, muted, accent, ok, err — never a literal colour.
_Avoid_: Colour, style, RGB

**Face**:
One of the weights an ink can wear beyond its colour — bold, italic, reversed, faint — and a look
wears a set of them, so the accent dimmed is a face on the accent, a strong word inside a heading
wears two, and neither is a second ink. A look that wears none is `plain`.
_Avoid_: Style, emphasis, attribute

**Underline**:
The line a look draws under the cells it is written in, in an ink of its own and never a face: a link
will wear one, and a cell that carries one still reads back as its ink and its faces.
_Avoid_: Rule (that one fills a row), decoration, border

**Palette**:
The colours the seven inks, the three bands and the ramp a raised surface is drawn on, resolved once
at boot from what the terminal says it can do: one design, spelled in truecolour, in ANSI, or not at
all.
_Avoid_: Theme, scheme, colours

**Band**:
One of the three places a frame paints a background under its cells — the rail, the composer band,
the well the transcript and each code fence in it sit in. A raised band is drawn on a ramp and a
flat one in a single colour. A cell on a band still reads back as its ink.
_Avoid_: Panel, colour, stripe

**Edge**:
The light a raised surface catches along its top and its side, and the shadow it leaves under its
bottom: the two ends of a ramp, beside the three bands, and never an ink.
_Avoid_: Border, elevation, highlight

**Accent edge**:
The accent in the faint face along the edge row of the one surface that takes the keys — the
composer at rest, the card while a screen is open — where every surface that takes none keeps the
dim edge, and a terminal that draws no colour is given neither. Not an **edge**: an edge is the
ground a row is raised on, and this is the ink written on it.
_Avoid_: Focus ring, highlight, active border

**Ramp**:
The grounds a raised surface's face is drawn in, one per row, from the lit edge along its top to the
shadow under its bottom, so a band of any height reads as one raised surface and its cells keep
their ink.
_Avoid_: Gradient, fade, shading

**Meter**:
The six cells a share of a window is drawn as, filled to the share: a ceiling can be a meter, and a
hit rate — which has no ceiling — never is.
_Avoid_: Bar, gauge, progress bar, sparkline

**Picture**:
An image an answer points at, the diagram a fence becomes, or **Sindri's face** — measured into its
**footprint** of cells before a pixel is spent, drawn as a **placement** over those rows.
_Avoid_: Image (that is the `src`), graphic, thumbnail, raster (one way of drawing one)

**Surface**:
The three facts a picture is measured against — the workspace, the pixels in a cell, whether this
terminal places pictures at all.
_Avoid_: Context, environment, settings

**Sindri's face**:
The picture of Sindri that travels in the **artefact** beside the bundled font, measured from its
own header into a **footprint** and drawn over those cells — decoration, so a terminal that places
no picture is given no cells and no words for it. A reader turns it off in the config, and a frame
with it off reserves no band: the facts stand from the **rail**'s first row. Not the **face** an ink
wears.
_Avoid_: Logo, avatar, mascot, icon

**Card**:
The frame a screen is drawn on: a rounded border carrying the screen's name in its top row and
the key that leaves in its bottom, at most seventy-two columns wide and centred, and — under fifty
— the whole frame with its title row and no border.
_Avoid_: Modal, dialog, popup, overlay, sheet

**Scrim**:
The frame behind a card, painted and dimmed rather than skipped: a reader sees the transcript
they will return to and sees that no band under the card takes a key.
_Avoid_: Backdrop, shade, veil, mask

**Path picker**:
The screen `@` opens: the workspace's files as `git` reads them, nearest first, capped at eight
rows.
_Avoid_: File browser, autocomplete, fuzzy finder, file search

**Front door**:
The frame a launch opens on: the wordmark, one line saying what the product is, and three commands
with the line that explains each, beside the **rail** of the facts the launch already holds — the
wire, the model, the workspace, its branch and the enforcement, and an em dash for the session and
the temp a run has yet to fill. Below a hundred columns it is drawn with no rail. One paint pass,
and never again.
_Avoid_: Splash screen, welcome screen, landing page, boot animation

**Stats screen**:
The screen `/stats` opens: the facts the rail is too narrow to hold — the route, the spend, the
instants, the whole GUID, the whole temp, the whole workspace — drawn from the record the run wrote
and the facts it was handed, and settled rather than live. It is the ledger's second adapter: the
band in, and the rows the ledger widens into as many columns as hold a whole one.
_Avoid_: Status screen, dashboard, details, inspector

**Artefact**:
The one file the product ships, static and `x86_64-unknown-linux-musl`, built and held by the
release stage. What the evals hold to a golden is a fixture, not this.
_Avoid_: Build, build output, package

## The paper trail

**Spec**:
What must be true — the acceptance criteria of the issue that asked for the work — and never the
reason it is built this way.
_Avoid_: Requirement, plan (an ADR is the reason)

**ADR**:
One numbered file under `docs/adr/`, holding one decision: what was chosen, why, and what a reader
is held to.
_Avoid_: Note, memo, doc, record (the run writes those)
