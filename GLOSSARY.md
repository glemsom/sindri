# Glossary

Sindri's vocabulary. Use these words exactly; do not drift to synonyms.

## Agent

**turn** — one round trip to the model: our request, its response (text, and
possibly tool calls), and the tool results we feed back. Bounded by the **turn
cap** (`max_turns`, default 250).

**tool call** — one request from the model to run `bash`, plus the structured
**tool result** we return. A single call may run a whole pipeline or script.

**bash tool** — the only tool. Runs a command string inside the sandbox.

**answer** — the final assistant text of the last turn. The product.

**stopped** — why the loop ended: `final` (the model finished), `max_turns`,
`cancelled`, `error`. Appears in the JSON output.

**cancel** — Ctrl-C (SIGINT): one flag the loop and the bash executor share. The
loop stops between turns; a running command's process group is killed; nothing
reaches stdout; exit 130.

## Sandbox

**sandbox policy** — the value holding writable set, network posture and temp
mapping. The *only* source of both enforcement and description (ADR-0002).

**writable set** — the paths mounted read-write in addition to the session temp:
the workspace plus configured extras.

**extras** — user-configured extra writable paths.

**session temp** — per-run writable scratch directory, mapped to `TMPDIR`,
`TEMP` and `TMP` inside the sandbox, and mounted as `HOME` beneath it.

**read-only root** — `--ro-bind / /`: the whole host filesystem mounted inside
the sandbox read-only. A boundary on writes, not on reads — everything under
the host root, credentials included, stays readable in there (ADR-0010).

**description** — the model-facing text derived from the sandbox policy, also
printed by `sindri doctor`.

**yolo-unsafe** — `--yolo-unsafe`: run `bash` with the user's own privileges, no
bwrap. Always visible in the description and the session record.

**doctor** — `sindri doctor`: verifies bwrap, the binary set and the mount plan,
and prints the sandbox description.

## Run

**session** — one run, identified by a GUID, recorded under
`$XDG_STATE_HOME/sindri/sessions/<guid>/` (ADR-0006). Also the value a front end
holds to have one: `session::Session` in `sindri-core` is the only place a run is
started, so the session directory, the sandbox policy, the record, the executor,
the tool and the agent are built in one order, once. A front end adds only what
is not the session's own — the branch and the model's context window are the
rail's facts, not the session's.

**run environment** — the three ambient facts a session is opened in: the
workspace, the `PATH` the sandbox is given, and the state root the record lives
under. Read from the process once, at `Session::open`, and passed as a value
everywhere below it, so a test can open a session in a temporary workspace
without touching the process.

**superseded session** — a session replaced by `/new`. Its record ends where it
ended: it gets no end written on its way out, because a superseded session did
not stop, it was left.

**transcript** — the ordered record of messages, tool calls and tool results of
a session.

**spill file** — where the full text of an over-long tool result is written when
the model-facing copy is truncated head+tail.

**batch mode** — `-b`: the Unix-filter front end. stdout is the answer only;
stderr is silent unless `-v`; `--format json` switches the output shape
(ADR-0004).

**reasoning** — provider-side thinking tokens, off by default, shown on stderr
with `-v`.

## Terminal UI

**frame** — bare `sindri` on a terminal: the alternate-screen interface as a
whole, and the thing this repository calls the TUI when it is being precise
(ADR-0011).

**rail** — the right column of the TUI: cache hit percentage, turns against the
cap, tokens, live context meter, session GUID, session temp path, git branch,
sandbox posture, and the face, where Kitty graphics allows one (ADR-0011). At
100 columns and above it is always visible; below, a one-line strip. The face
takes only the rows the facts leave, so the rail is complete with or without it
(ADR-0014).

**composer** — the input at the bottom: 2–8 rows, `Shift+Enter` for a newline,
`@` for a path completion, `/` for a command.

**delta** — one piece of streamed model text, as it arrives (`TextDelta`,
`ReasoningDelta`). Batch accumulates deltas into the same answer it prints.

**thread** — the growing conversation of one TUI session: every prompt, answer
and tool result, in order, as the model is shown it. One launch is one thread,
so a second prompt is a follow-up and a model changed in `/settings` answers the
next turn on the thread the last one built (ADR-0014). `/new` throws it away and
starts a new session. A prompt cut short — by the turn cap, a cancel or a
failure — leaves no unanswered half-exchange behind.

**queued prompt** — a prompt typed while a run is in flight, sent when that run
ends rather than interrupting it.

**spill affordance** — the offer to open a tool result's spill file from the
transcript, when the output was too large to display in full: the row says
`output too large`, `Ctrl-O` or a click on it opens the untruncated text.

**window** — the rows of the transcript that fit, built from the newest row
backwards so an unbounded thread still lays out a bounded frame. Scrolling moves
the window; it never re-lays out the thread (ADR-0013).

**selection** — a drag over rendered rows, resolved against the plain text of
what the last frame drew: columns, not styles, so a copy is the characters the
user saw.

**copy channel** — where a selection goes: OSC 52 to the terminal (reported as
*sent*, because a terminal does not answer), `wl-copy` then `xclip` (reported by
name), or a refusal, which is said as a refusal and never as success (ADR-0013).

**completion** — the `@` offer of workspace paths: a background walk ranked by
proximity, eight rows, `.git` skipped, `.gitignore` deliberately not honoured,
dotfiles only once a dot is typed. `Tab` or `Enter` takes one, `Esc` dismisses
(ADR-0013).

**help overlay** — the `?` list of every binding, each mouse gesture beside the
key that does the same thing.

**live tail** — the last five lines of a running command's output, shown under
its call line while it runs and folded away when it ends.

**settings modal** — the `/settings` overlay: one row per key of the config
file's schema — provider, model, base URL, API key, turn cap, timeout, thinking,
extra writable paths, extra headers — the path a save writes to, and the
warning a secret earns. `Enter` saves, `Esc` discards, `Ctrl-U` empties the
field with the cursor, and `Space` flips a toggle (ADR-0014).

**config document** — `Document` in `sindri-core`: the one config file staged
for editing, validated by the one schema. `editable`, `is_set`, `set` and
`save` are the whole of it, so the frame edits settings without ever parsing
TOML (ADR-0014).

**staged setting** — a field's value in the settings modal, held in memory. It
is not a setting until `Enter` writes it; `Esc` throws away everything staged
and touches nothing.

**reload** — the frame's request, after a save, that the front end read the
config file again so the next prompt runs on it. `Driver::reload` answers with
what the rail should now show, or with why the next run cannot start (ADR-0014).

**from environment** — the API key field's default, and the one the modal
returns to when the field is emptied: the credential comes from
`ANTHROPIC_API_KEY`/`OPENAI_API_KEY` or `SINDRI_API_KEY`, and nothing is
written to disk.

**masked field** — a settings row whose value is never drawn: `api_key` and
`extra_headers`. The document does not hand their values back at all, so the
row can say *set in the file* or *from environment* and nothing more
(ADR-0014).

**strip** — the rail as one line, on a terminal narrower than 100 columns: the
same facts, never hidden behind a keystroke.

**card** — the full-screen failure a launch draws instead of a frame, when a
`doctor` check fails before the first frame. It names the check and says what to
do about it.

**driver** — the port a front end implements so the frame can ask for a run
(`Driver::run`): one prompt in, a `Finished` out, and the run's events on the
way. The frame holds no loop of its own (ADR-0012).

**output tap** — `OutputSink`, the port a running command's output is handed to
line by line, so a front end can show a live tail without the executor knowing
what a terminal is (ADR-0012).

## Release

**release artefact** — one static musl binary, named
`sindri-<version>-<target>`; the installer derives exactly that name from
`uname`, so the build and the installer share one naming contract.

**checksums** — `SHA256SUMS` in a release: every artefact, sorted, verified by
the installer before it writes anything.

**installer** — `install.sh`, run as `curl … | sh`: picks the target from the
machine, verifies the checksum, installs into the user's local bin and ends by
pointing at `doctor`.

**changelog** — the notes a release page and its `CHANGELOG.md` carry, derived
from the tickets a release closes: the merge commits since the previous tag,
which is what `TN: … (#issue)` is for.

**release image** — the container generated from the committed `devbox.json`
that builds the release, so dev, CI and release share one toolchain (ADR-0009).
