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

**Status**:
`idle`, `working`, `done`, `blocked`, `error`, with one coarse line of the run's own words — never
the command, the prompt or the answer.
_Avoid_: Progress, notice, indicator

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

**Sindri's face**:
The picture of Sindri that travels in the **artefact** beside the bundled font, measured from its
own header into a **footprint** and drawn over those cells — decoration, so a terminal that places
no picture is given no cells and no words for it. It stands over the **rail**'s top rows while a
run is live, and over the wordmark of the frame a launch opens on, where the rail beside it keeps
every fact. A reader turns it off in the config, and a frame with it off reserves no row: the facts
stand from the rail's first row. Not the **face** an ink wears.
_Avoid_: Logo, avatar, mascot, icon

**Card**:
The frame a screen is drawn on: a rounded border carrying the screen's name in its top row and
the key that leaves in its bottom, at most seventy-two columns wide and centred, and — under fifty
— the whole frame with its title row and no border.
_Avoid_: Modal, dialog, popup, overlay, sheet

**Path picker**:
The screen `@` opens: the workspace's files as `git` reads them, nearest first, capped at eight
rows.
_Avoid_: File browser, autocomplete, fuzzy finder, file search

**Front door**:
The frame a launch opens on: the wordmark, one line saying what the product is, and three commands
with the line that explains each, set as one block and centred in the band an answer is drawn in,
with Sindri's face over it, beside the **rail** of the facts the launch already holds — the wire, the
model, the workspace, its branch and the enforcement, and an em dash for the session and the temp a
run has yet to fill. In a frame too narrow for a rail's column it is drawn with no rail, and in one
too short or too narrow for the face it is drawn without it and reserves no row. One paint pass, and
never again.
_Avoid_: Splash screen, welcome screen, landing page, boot animation

**Key protocol**:
What a launch asks of its terminal when it takes the screen, so a key that names no glyph arrives as
a report of itself rather than as a byte: `Shift+Enter` breaks a line where `Enter` sends one. It is
pushed on the terminal's own stack and popped first on every way out, and a terminal that speaks none
is handed the same frame and the same keys.
_Avoid_: Keyboard enhancement, CSI u, key encoding

**Progress mark**:
The terminal's own indeterminate progress indicator, worn while a run streams and taken away by every
stop, by a telling turned off, and by the way out — a mark left on a finished run says a run is
working that has ended.
_Avoid_: Progress bar (that is a share, and a run's turns are a count), spinner, pulse (that one is
the marker at the head of an answer)

**Focus**:
Whether the terminal says this window is the one being read: asked for when the screen is taken and
given up on the way out, and the one fact the frame draws from it — the accent edge recedes while
nobody is reading and returns when they are. A terminal that reports nothing is being read.
_Avoid_: Active, selected, current window

**Artefact**:
The one file the product ships, static and `x86_64-unknown-linux-musl`, built and held by the
release stage. What the evals hold to a golden is a fixture, not this.
_Avoid_: Build, build output, package