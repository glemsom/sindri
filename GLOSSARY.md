# Sindri

An AI agent that is a Unix filter and a terminal UI over one core. Most of these words exist
because an ordinary one would blur two things this project keeps apart.

## The run

**Run**:
One invocation of the agent loop for one prompt, from the first request to a stop reason.
_Avoid_: Job, task, query, conversation

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
The one module that holds a run's **facts** and the one way each **value** is spelled — the size in
tokens, the pair whose halves line up on a slash, the meter a share fills and the ink it is read in,
the head of a name, the tail of a path, the em dash a fact is owed, the first eight of a GUID. No
drawing spells a fact for itself, so a fact reads one way wherever it is said.
_Avoid_: Facts module, status bar, sidebar (the rail is what draws it)

**Fact**:
One named thing a run holds, drawn as a label and a **value**: the wire, the served name, thinking,
the turns, the window, the cache, the workspace, the branch, the skills, the sandbox, the session,
the temp. A fact nobody can state is absent rather than blank, and a fact a launch names and a run
has yet to fill is an em dash.
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
One recorded run's directory, named by its GUID: session document, transcript, status documents,
temp.
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

**Palette**:
The colours the seven inks and the three bands are drawn in, resolved once at boot from what the
terminal says it can do: one design, spelled in truecolour, in ANSI, or not at all.
_Avoid_: Theme, scheme, colours

**Band**:
One of the three places a frame paints a background under its cells — the rail, the composer band,
the well a code fence sits in. A cell on a band still reads back as its ink.
_Avoid_: Panel, colour, stripe

**Meter**:
The six cells a share of a window is drawn as, filled to the share: a ceiling can be a meter, and a
hit rate — which has no ceiling — never is.
_Avoid_: Bar, gauge, progress bar, sparkline

**Picture**:
An image an answer points at, or the diagram a fence becomes — measured into its **footprint** of
cells before a pixel is spent, drawn as a **placement** over those rows.
_Avoid_: Image (that is the `src`), graphic, thumbnail, raster (one way of drawing one)

**Surface**:
The three facts a picture is measured against — the workspace, the pixels in a cell, whether this
terminal places pictures at all.
_Avoid_: Context, environment, settings

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
and the facts it was handed, and settled rather than live.
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
