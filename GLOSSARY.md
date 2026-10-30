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
between turns, the command's process group is killed, stdout stays empty, exit is 130.

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

**spill file** — where an over-long tool result's full text is written when the
model-facing copy is truncated head+tail.

**batch mode** — `-b`: the Unix-filter front end. stdout is the answer only; stderr is
silent unless `-v`; `--format json` changes the output shape (ADR-0004).

**reasoning** — provider-side thinking tokens, off by default, shown on stderr with `-v`.

## Terminal UI

**frame** — bare `sindri` on a terminal: the alternate-screen interface as a whole
(ADR-0011).

**rail** — the right column: cache hit percentage, turns against the cap, tokens, context
meter, session GUID, session temp path, git branch and sandbox posture. 32 columns from
100 up; below that, the same facts as one line under the transcript. The GUID is copied
with `Ctrl-G`, or by clicking its row.

**canvas** — the terminal's own black: the lowest of the three tiers, and the one
the frame paints nothing on. Above it: **panel**, then **raised** (ADR-0016).

**panel** — a surface the frame draws on, one tier above the canvas: the composer,
a fenced code block, the rail, the band a turn's **prompt** sits on.

**raised** — the surface an overlay is drawn on, one tier above a panel; the frame
is dimmed beneath it.

**composer** — the input at the bottom: 2–6 rows, `Shift+Enter` for a newline, `@` for a
path completion, `/` for a command.

**thread** — the growing conversation of one TUI session; `/new` starts over. Laid
out as a document: two blank rows between turns and none inside one, a two-column
gutter a turn's answer and calls hang off, and the prompt at full strength on its
own band.

**turn header** — the row that opens a turn: `YOU  <hh:mm>  <workspace>`.

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
(ADR-0014).

**from environment** — the API key field's default, and where emptying it returns: the
credential comes from `SINDRI_API_KEY`, the provider's variable, and nothing is written.

**door** — the frame's front door (`crates/cli/src/frame.rs`): the one place a run with
no credential opens the interface instead of exiting, with the **settings modal** staged
on the `api_key` row (ADR-0004).

**card** — the full-screen failure drawn instead of a frame when a `doctor` check fails.