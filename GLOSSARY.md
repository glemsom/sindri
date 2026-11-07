# Sindri

An AI agent that is a Unix filter and a terminal UI over one core. The vocabulary below
is the load-bearing half: most of it exists because an ordinary word would blur two
things this project keeps apart.

## The run

**Run**:
One invocation of the agent loop for one prompt, from the first request to a stop reason. Owns a session, a policy and its budgets.
_Avoid_: Job, task, query, conversation

**Turn**:
One request to the model and the tool calls it asks for. Capped by the turn cap; a turn is not a unit of progress.
_Avoid_: Step, iteration, wave

**Turn cap**:
The ceiling on turns in a run (`max_turns`), whose exhaustion is a stop reason and not an error.
_Avoid_: Max steps, budget

**Budget**:
One of the two run-scoped limits that a turn cap cannot see — command wall clock, and bytes of command output fed back. Distinct from the turn cap and from the per-call timeout.
_Avoid_: Limit, quota, allowance

**Stop reason**:
Why a run ended, from a closed set: `final`, `max_turns`, `budget`, `cancelled`, `error`. A record says `running` until one lands.
_Avoid_: Exit reason, status, outcome

**Answer**:
The text a run produces for stdout — the only thing on stdout in batch mode.
_Avoid_: Response, output, result

**Exit**:
How one call ended, from a closed set: the code a command exited with, the per-call timeout that killed it, the user's word that stopped it, or a signal that left no code to report. A fact of the result entry, and what a tool row says.
_Avoid_: Verdict (that one is the wire's), status, outcome, result

**Status**:
What the run says it is doing: one of §14's five states — `idle`, `working`, `done`, `blocked`,
`error` — with one coarse line of the run's own words, the turn count and the workspace's name.
Built from `Told`'s permitted facts and never from a formatted string, so it can never be the
command, the prompt or the answer, and written by whoever owns the tty.
_Avoid_: Progress, notice, indicator

## Confinement

**Enforcement**:
How a run's commands are confined, as a two-valued fact: `bubblewrap` or `unsafe`. Said in the record, the JSON and the rail.
_Avoid_: Sandbox mode, safety level, permission

**Policy**:
The boundary in force for a run — writable set, per-call timeout, budgets — which is what tool descriptions are derived from and the model is told about.
_Avoid_: Config, permissions, settings

**Writable set**:
The canonical, absolute, distinct, non-nested paths a run may write, never including `/`.
_Avoid_: Allowed paths, whitelist, mounts

**Session temp**:
Per-session scratch mapped to `TMPDIR`, alive as long as the session, so a run's calls see each other's files and no run sees another's.
_Avoid_: Tmpdir, scratch dir, work dir

## Words from the wire

**Wire**:
A provider dialect and its endpoint configuration: `anthropic`, `openai`, `copilot`. The parser accepts exactly these words.
_Avoid_: Provider, backend, API

**Served name**:
A model identifier read off the wire, never invented, kept beside the `(wire, base_url)` that produced it.
_Avoid_: Model list, catalog, available models

**Route**:
Which completion endpoint an OpenAI-shaped wire serves — `chat` (`POST /chat/completions`) or
`responses` (`POST /responses`) — chosen by rig from the served name, overridable per run when a
route misbehaves, and kept in the session document.
_Avoid_: Endpoint, API, mode, protocol

**Preamble**:
The system message, re-sent every turn, so every word in it is a tax.
_Avoid_: System prompt, instructions

**Verdict**:
What kind of failure a wire classified, from a closed set — timeout, rate limit, server fault, transport, client, refusal, cancel, broken — never the status code that carried it.
_Avoid_: Error code, status, reason

**Attempt**:
One send of one ask. Three at most, waiting 200 ms and then twice that between them, and only for a failure the wire called retryable that streamed no text.
_Avoid_: Try, retry, resend

## Sessions and skills

**Session**:
One recorded run's directory, named by its GUID, holding the session document, the transcript, the status documents and the temp.
_Avoid_: Conversation, chat, history

**Entry**:
One thing that happened in a run, in order, as written to the transcript — the unit the UI and any reader share.
_Avoid_: Event, log line, message

**Skill**:
A directory holding a `SKILL.md` whose body the model can load by name, in one of three scopes.
_Avoid_: Prompt, plugin, recipe, tool

**Loaded-skill set**:
The names loaded so far, in load order — one set, which the human's `/skill:name` and the model's `skill` call both write to, and which clearing the history empties. The conversation *is* the run's history (A14).
_Avoid_: Conversation state, skill cache, loaded prompt

**Scope**:
Where a skill came from, lowest precedence first: `builtin`, `user`, `repo`.
_Avoid_: Source, origin, level

**Shadowing**:
A nearer scope taking a name from a further one, which is reported rather than resolved silently.
_Avoid_: Override, collision, duplicate

## Secrets

**Secret**:
A literal string a run was trusted with — the credential, an extra header's value — that nothing the run writes may contain.
_Avoid_: Key, token, password, environment value

**Redacted**:
A string that scrubs itself wherever it leaves the core, so a window prints a safe value without remembering to. It carries the `SecretSet` it scrubs through.
_Avoid_: Masked, sanitised, filtered

**Secret set**:
Every literal a run must never let out, matched longest first, so a secret that contains another cannot leave a tail behind.
_Avoid_: Denylist, pattern list, filter

## The shape

**Core**:
The part of Sindri that makes every decision — run loop, policy, records, providers — and that has no terminal.
_Avoid_: Engine, library, backend

**Front end**:
A way of driving the core — the TUI or batch mode. Owns no decision the core could make.
_Avoid_: UI, client, interface

**Ink**:
A colour role the interface asks for by name — dim, muted, accent, ok, err — never a literal colour.
_Avoid_: Colour, style, RGB

**Mode**:
A screen that answers keys, one of the explicit stack the frame is in: the composer at the
base and a modal opened over it. The mode on top answers every key.
_Avoid_: State, layer, screen (too general), view

**Key table**:
The keys one mode answers, as data — the key, how the `?` screen spells it, what it does —
so the screen that explains the keymap is painted from the rows that are the keymap.
_Avoid_: Keymap (one word for the whole frame's), binding list, hotkeys

**Frame fixture**:
A frame rendered at a fixed size and checked into the tests as the accepted picture of it.
_Avoid_: Snapshot (too general), screenshot, golden image

**Path picker**:
The screen `@` opens over the draft: the files of the workspace as `git` reads it, nearest
the working directory first, narrowed by a query typed into the screen and capped at eight
rows. A row a selection takes is a path in one shell-safe word.
_Avoid_: File browser, autocomplete, fuzzy finder, file search
