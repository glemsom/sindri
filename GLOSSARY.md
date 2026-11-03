# Glossary

The vocabulary Sindri is written in. One entry per word, one meaning per entry: where the
code says a word, that word is used here and its synonyms are not. Each entry points at the
module that carries the thing, so the code holds the detail and this file only tells a
reader where to look.

## The run and its loop

**turn** — one request to the model and the calls it asks for in reply. `max_turns` caps
them, defaults to 250, so a run that never stops of its own accord still ends and says why.

**wave** — the tool calls one turn emitted, run together because the model declared them
independent. Results come back in the order asked for, whatever order they finished in, and
the wave carries the loaded-skill set from one turn to the next
(`crates/core/src/agent/calls.rs`).

**provider** — the model behind one seam: a call asks, and gets an answer or a failure that
is safe to print (`crates/core/src/provider/mod.rs`). Nothing above that seam knows the
client library exists — `rig_wire` is the one file that does.

**wire** — which provider API a run speaks, Anthropic Messages or OpenAI-compatible Chat
Completions, and what each needs: its credential variable, its base URL variable and
whether it needs a credential at all (`crates/core/src/config/provider.rs`).

**answer** — the text the model delivered, and the whole of stdout in batch mode: the
answer and exactly one newline, or nothing when the run failed or was cancelled.

**stopped** — why a run ended, the closed set of `Stopped` (`crates/core/src/agent/report.rs`):
`final`, the run answered; `max_turns`, it spent the cap; `budget`, it spent its command
time or its output; `cancelled`, SIGINT with nothing on stdout; `error`, it could not
deliver. A record writes `running` until one of them lands.

**preamble** — the system message a run sends on every turn: Sindri's stance in its own voice,
the Environment section whose one fact is the **workspace** the run stands in, and a
repository's **instructions** when it has any (`crates/core/src/preamble.rs`). The module's
own paragraph holds the split between its stance, a **skill**'s procedure and the
**sandbox**'s boundary.

## The tools

**tool set** — the tools a run holds: `bash` always, `skill` only when the workspace holds
one the model may see, `open` only in a front end a human with a browser sits in front of
(`crates/core/src/agent/tools.rs`). A tool a front end does not build is a tool the schema
never mentions.

**call** — one tool call: the variant of `Asked` that names the tool, read once so every
match over it is exhaustive (`crates/core/src/provider/asked.rs`). A tool added there is a
variant the loop, the record and the frame must each answer.

**bash** — the one tool that runs things: search, fetch, patch and HTTP, all behind the
sandbox. Its description is derived from the sandbox policy in force, so the boundary the
model is told about is the boundary enforced.

**skill** — the second tool: instructions written for the model, loaded into the
conversation once, frontmatter stripped and the skill's own files listed beside the body
(`crates/core/src/agent/skill.rs`). A tool and not a file read through `bash`, because a
permission nobody enforces is a suggestion.

**doctrine** — the prose the `skill` tool is described to the model with, and nothing about
the workspace: what the tool loads, that it loads once, the key a task is matched on — the
catalog below — and the **trigger sentence**, which asks for one before acting
(`crates/core/src/agent/skill.rs`). A text of its own, `DOCTRINE` from the `doctrine.md`
beside that module, because it is the one text of the tool a bench can move to measure the
wording, and a wording nobody measured is an argument: the bench's third arm is the shipped
doctrine with the trigger sentence gone and the words that say the load happens once left in
place. The catalog sits beside the doctrine in the same description and is not part of it —
that one changes with the tree.

**trigger sentence** — the one line of a doctrine that asks for a load before acting, with
the clause that limits it to a task the skill covers and the brake that keeps it off every
other ("Call it before acting when a task is one it covers, and not otherwise.",
`crates/core/src/agent/skill.rs`). It is a sentence of its own and alone on its line, so
removing it takes nothing with it: that is what makes the bench's third arm the absence of
one sentence rather than a rewrite (`scripts/bench-skill-ablation/doctrines.py`).

**open** — the third tool: one target handed to the host's opener
(`crates/core/src/agent/open.rs`). The core decides and refuses; the spawn belongs to the
process that still holds a display, which the sandbox is what takes away.

## Skills

**skill scope** — where the catalog looks, lowest precedence first: `builtin`, the skills
compiled into the binary; `user`, `~/.agents/skills`; `repo`, every `.agents/skills` from
the workspace up to its repository root, nearest first and never above it. A nearer scope
takes a name from a further one.

**built-in skill** — a `SKILL.md` compiled into the binary with `include_str!`, not a
directory any scan walks (`crates/core/src/skills/builtin.rs`). It has no path, because a
path can be absent, stale or unwritable, and the instructions a run is given and the binary
that enforces them ship in one artefact or neither does.

**user-only skill** — a skill carrying `disable-model-invocation: true`: a human may put it
in force, and it is absent from the schema and the catalog the model reads.

**model-only skill** — a skill carrying `user-invocable: false`: the model may load it, and
the frame's completion never offers it. Both flags are Claude-compatible extensions, read
as plain booleans and ignored when spelled any other way.

**catalog** — every skill the scan found, one line per skill in the `skill` tool's own
description. There and not in the preamble because it changes with the workspace, and paid
only where it buys something: with no model-visible skill the tool is not registered at all.

**description ceiling** — `DESCRIPTION_CEILING`, 200 characters, the most of a skill's
description the catalog will carry (`crates/core/src/skills/frontmatter.rs`). About two
sentences — the task covered, and where the model's default is wrong — and a longer
description is a refusal said out loud, never a truncation, because a truncated routing key
is a skill that stops being found.

**instructions** — a repository's own rules: the `AGENTS.md` at the repository root, read
once at boot and appended to the preamble, after its Environment section. Never a nested
file, never an ancestor's; capped at 8 KiB, a longer file arriving cut with a marker saying
so; `Taken` is what the read produced — `Absent`, `Found` or `Unreadable` — and an
unreadable file is a diagnostic that never stops the run
(`crates/core/src/instructions.rs`).

## The sandbox

**sandbox** — unprivileged bubblewrap, host root read-only, plus a writable set the user
chose. Read-only bounds writes and not reads, which is why a host socket the process
already holds is reachable, and why a tool routing around a refusal is not the shape to
build.

**bubblewrap** — `bwrap`, the program the policy is enforced through, and the word a run
reports as its `enforcement` when it is in force. A `bwrap` that cannot start is a failure
to report and never one to paper over.

**session temp** — the writable scratch a run's own session directory carries, mapped to
`TMPDIR`. `HOME` is the host's own, read-only, and nothing is created for it.

**writable set** — the paths a run may write beyond its session temp. It stays the user's
sentence: a level that names one replaces the shipped floor rather than adding to it
(`crates/core/src/sandbox/extras.rs`).

**unsafe** — what a run says of itself under `--yolo-unsafe`: commands run with the user's
own privileges, outside bubblewrap, inheriting environment and working directory. The
record, the JSON and the rail all say it, and the flag dies with the invocation that asks
for it.

## Status and records

**status** — what a run tells its terminal it is doing: one `OSC 7501` report, written
unqueried, carrying one of the specification's closed set of states — `idle`, `working`,
`done`, `blocked`, `error` — a coarse line, the turn count and the workspace's name. The
value is `sindri_core::status::State`, the encoder the pure `status::report`
(`crates/core/src/status.rs`), the identity `app=sindri`, and the writing belongs to
whoever owns the tty.

**record** — the files a session leaves: `session.json`, `transcript.jsonl`, `status.json`
and the temp, written as the run goes, so a crashed run still reads
(`crates/core/src/record/mod.rs`). Two other senses of the word live in the tree and are
never a **status** report's meaning: what a terminal stores from one is the specification's
*record*, and `agent::Report` is what one run did — the answer, why it stopped and what it
cost.

**transcript** — `transcript.jsonl`: one entry per thing that happened, in order, written
as it happens and read back by `sindri sessions show` and `/export` from the same `Entry`
values, so the two cannot disagree.

**session** — one run's value a front end holds to have a run: the session directory with
its temp, the record, and the identity that names them (`crates/core/src/session.rs`).
`Session::open` is the only place the order a run is started in exists.

## The frame

**frame** — the terminal UI: alternate screen, streaming markdown thread, rail and composer,
launched by a bare `sindri` on a terminal (`crates/cli/src/frame.rs`). Off a terminal the
same invocation is a refusal at exit 2.

**rail** — the frame's ledger of facts, always visible: cache hits, turns against the cap,
tokens, the context meter, the session id, the temp path, the branch, the sandbox posture
and why the last run stopped.

**thread** — the transcript as the frame holds it: the entries in order and the rows each
one is worth at the width it was measured at, with a mutation the only way in, so exactly
the heights a change invalidated are forgotten (`crates/tui/src/thread.rs`).

**entry** — one row of the thread, and the state a call carries while it runs: the command,
the output kept and the word its row ended on (`crates/tui/src/entry.rs`).

**composer** — the prompt being typed, held as rows and a character-index cursor and
reported in cells, because the terminal draws cells and a glyph may be two wide.

**thought** — reasoning the run is having, as a row of the thread: the turn it belongs to,
the text streamed so far, when it began and — once it has ended — how long it took
(`crates/tui/src/entry.rs`). A fragment is appended to its entry as it lands, so it streams
in the way an answer does, off the same gutter and in the thought's own ink, and it ends in
place: its marker row above its text, where it was, with nothing to fold and nothing to open.

**mark** — the wash a drag leaves on the transcript. `mark::span` computes the cells from
press to release and the copy reads the same function, so the mark cannot light a cell the
copy does not take nor leave dark one it does (`crates/tui/src/mark.rs`).

**block** — one **picture**'s rows taken as a single region: `Viewport::block` names the
picture a row belongs to (`crates/tui/src/selection.rs`), its graphic's rows and caption are
lit together, and its words are taken once. Which rows a copy takes is **mark**'s rule.

**picture** — an image the thread shows: a markdown `src` resolved to a file in the
workspace, or a mermaid fence's body, measured without spending its pixels and refused with
a reason the caption can print (`crates/tui/src/picture.rs`).

## Front ends and surfaces

**front end** — a way into the same core: the frame, batch mode, `sindri sandbox exec`,
`sindri doctor` and `sindri sessions`. Each owns only what is its own, and none owns a
decision the core could make.

**batch mode** — the Unix filter front end (`sindri -b`): reads the prompt and the stdin
context, runs the agent to an answer, prints it plus exactly one newline. Exit codes, not
prose, tell a script what happened, and its toolset carries no `open` because it has no one
to look.

**boot** — the one door a front end calls to turn flags and the process's own facts into a
run ready to start (`crates/cli/src/boot.rs`, `Boot`). It probes nothing: the frame proves
the sandbox and draws the card, batch prints the answer, `sandbox exec` prints the outcome.

**failure** — the words that follow `sindri: ` and the exit code the shell reads, in one
module, so a front end never learns its failure words from another
(`crates/cli/src/failure.rs`).

**exit code** — what a script branches on: `0` an answer, `1` a model or provider error,
`2` a usage error, `3` a sandbox or setup failure, `100` the turn cap, `101` the budget,
`130` cancelled with nothing on stdout. `sindri sandbox exec` reports the sandbox instead,
`3` only when the sandbox failed.

**json output** — one run as exactly one compact JSON object on one line, its field order
the contract, later fields only appended, and a field nobody reported left out rather than
`null` (`crates/core/src/output.rs`).

## Configuration

**config file** — `$XDG_CONFIG_HOME/sindri/config.toml` and nowhere else, read by one
schema. A checked-in `.sindri.toml` is never read: a repository that could grant writable
paths would turn every clone into a privilege escalation.

**precedence** — one order for every setting, stated once: the config file, then `SINDRI_*`
environment, then flags. A repeated flag replaces the configured value rather than adding
to it, and an unknown key is an error naming the key and the file.

## The gate

**gate** — `make check`, the whole of done: fmt, clippy with warnings denied, the four
audits, the tests, the evals, proof that an unchanged tree does not relink, and the release
scripts run as processes. Nothing lands on a subset.

**audit** — the four fast checks `make audit` runs in milliseconds and the commit hook
refuses a broken commit with: no `*.rs` over 800 lines, a `//!` paragraph opening every
module, a clause beside every module declaration in a crate root, and a test name that
states the behaviour it proves.

**eval** — one scripted batch run in its own copy of a fixture repository, asserting on exit
codes, JSON fields, the record left and the files on disk rather than on the model's prose.
`make eval` replays recorded responses offline; `make eval-live` asks your own provider at
your own cost.

**harness** — the seam a test runs the loop through, so the loop's tests need no provider
and no bubblewrap (`crates/harness`).

## The workspace

**workspace** — the tree a run stands in: the path the sandbox mounts, the repository root a
repository scope stops at, and the directory the rail names
(`crates/core/src/workspace.rs`).

**repository root** — the nearest ancestor of the workspace holding `.git`, the ceiling a
repository scope never rises above and the directory an `AGENTS.md` is read from.
