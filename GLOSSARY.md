# Glossary

The vocabulary Sindri is written in. One entry per word, one meaning per entry: where
the code says a word, that word is used here and its synonyms are not. Each entry
points at the module that carries the thing, so the code is where the detail lives and
this file is only what a reader needs to look in the right place.

## The run and its loop

**run** — one prompt carried through the turn loop to an answer, from the session that
opens it to the report it leaves. `crates/core/src/agent/mod.rs` holds the loop;
`crates/cli/src/stack.rs` composes the stack a run stands on, and a front end keeps only
what is its own.

**turn** — one request to the model and the calls it asks for in reply. The loop spends a
turn per request, and `max_turns` caps them: the key is set in the config file and
defaults to 250, so a run that never stops of its own accord still ends and says why.

**turn cap** — the bound `max_turns` puts on a run, and one of the two figures the `bash`
description states. Spending it is not a failure: the answer is still the product, so the
run reports `done` and `stopped: max_turns`.

**stopped** — why the loop stopped, a closed set of five words `Stopped` spells: `final`
an answer was delivered, `max_turns` the cap was spent, `budget` the command time or output
was, `cancelled` a signal raised the flag, `error` the run could not deliver. A record
written from the moment it starts says `running` until one of them lands, which is what a
session `/new` superseded still says: it did not stop, it was left
(`crates/core/src/agent/report.rs`).

**wave** — the tool calls one turn emitted, run together because a model that emitted
them declared them independent. Results come back in the order asked for, whatever order
they finished in, and the wave is what carries the loaded-skill set from one turn to the
next (`crates/core/src/agent/calls.rs`).

**budget** — what a run may spend inside one wave where the turn cap cannot see it: the
wall clock its commands take and the bytes of their output fed back to the model
(`crates/core/src/agent/budget.rs`). Spending it stops the run with `stopped: budget`,
which still reports `done`.

**cancel** — the one flag a run checks to know it must stop now. A signal handler raises
it, the loop reads it between turns and the executor kills the command's process group
mid-flight, so a cancelled run leaves no orphan (`crates/core/src/cancel.rs`).

**preamble** — the fixed system prompt, re-sent on every turn of every run
(`crates/core/src/provider/rig_wire.rs`). Its words are a tax paid per request, which is
why the catalog is in a tool description instead, and why `AGENTS.md` travels beside it
rather than as a turn.

**provider** — the model behind one seam: a call asks, and gets an answer or a failure
that is safe to print (`crates/core/src/provider/mod.rs`). No part of the tree above that
seam knows the client library exists — `rig_wire` is the one file that does.

**wire** — which provider API a run speaks, Anthropic Messages or OpenAI-compatible Chat
Completions, and what each needs: its credential variable, its base URL variable and
whether it needs a credential at all (`crates/core/src/config/provider.rs`).

**usage** — what the provider reports it spent, in its own figures. It rides beside a
turn and totals into the report; the rail's tokens and context meter are drawn from it.

**context window** — how much conversation a model family can hold. Only the adapter
knows the family names, so only it can give the rail the denominator its meter needs
(`crates/core/src/provider/window.rs`).

**answer** — the text the model delivered, and the whole of stdout in batch mode: the
answer and exactly one newline, or nothing when the run failed or was cancelled.

## The tools

**tool set** — the tools a run holds: `bash` always, `skill` only when the workspace
holds one the model may see, `open` only in a front end a human with a browser is sitting
in front of (`crates/core/src/agent/tools.rs`). A tool a front end does not build is a
tool the schema never mentions.

**call** — one tool call: the variant of `Asked` that names the tool, read once so every
match over it is exhaustive (`crates/core/src/provider/asked.rs`). A tool added there is
a variant the loop, the record and the frame must each answer.

**bash** — the one tool that runs things: search, fetch, patch and HTTP, all behind the
sandbox. Its description is derived from the sandbox policy and the limits in force, so
the boundary the model is told about is the boundary enforced.

**skill** — the second tool: instructions written for the model, loaded into the
conversation once, frontmatter stripped and the skill's own files listed beside the body
(`crates/core/src/agent/skill.rs`). It is a tool and not a file read through `bash`,
because a permission nobody enforces is a suggestion.

**open** — the third tool: one target handed to the host's opener
(`crates/core/src/agent/open.rs`). The core decides and refuses; the spawn belongs to the
process that still holds a display, which the sandbox is the thing that takes away.

**target** — the one thing `open` hands over: an `http` or `https` URL the host's browser
fetches itself, or a path to a regular file inside what the run may write. Exactly one of
the two, and a `file:` URI is a path in another spelling, refused in the path rules'
words.

**outcome** — what `bash` returns instead of a process: an exit, a timeout, a cancel or a
sandbox error, with the two streams (`crates/core/src/exec/result.rs`). "The sandbox never
started" and "the command failed" are different outcomes and are never the same word.

**spill file** — where output too large to feed back is written in full, while the model
is handed the head and the tail. The truncation is the cap; the file is how a reader
recovers what the cap cut.

**refusal** — a thing the tool declined to do, said in the call and before anything was
attempted, naming the ground it stopped on. A refusal is a result the model can act on,
where a failure is a fact about the machine.

## Skills

**skill scope** — where the catalog looks, lowest precedence first: `builtin`, the skills
compiled into the binary; `user`, `~/.agents/skills`; `repo`, every `.agents/skills` from
the workspace up to its repository root, nearest first and never above it. A nearer scope
takes a name from a further one.

**built-in skill** — a `SKILL.md` compiled into the binary with `include_str!` and not a
directory any scan walks (`crates/core/src/skills/builtin.rs`). It has no path, because a
path is a thing that can be absent, stale or unwritable, and the instructions a run is
given and the binary that enforces them ship in one artefact or neither does.

**user-only skill** — a skill carrying `disable-model-invocation: true`: a human may put
it in force, and it is absent from the schema and the catalog the model reads.

**model-only skill** — a skill carrying `user-invocable: false`: the model may load it,
and the frame's completion never offers it. Both flags are Claude-compatible extensions,
read as plain booleans and ignored when spelled any other way.

**catalog** — every skill the scan found, one line per skill in the `skill` tool's own
description. It is there and not in the preamble because it changes with the workspace,
and it is paid only where it buys something: with no model-visible skill the tool is not
registered at all.

**shadowing** — a name two scopes both carried, kept as a record rather than dropped, so
`sindri doctor` prints the winner and the skill it displaced.

**frontmatter** — the YAML fence that opens a `SKILL.md`, read as a value tree rather
than a schema: the specification's required fields are taken, the two extensions are
honoured, and everything else is ignored rather than refused
(`crates/core/src/skills/frontmatter.rs`).

**description ceiling** — the limit `DESCRIPTION_CEILING` puts on a skill's description,
200 characters, because that line is what the catalog carries on every turn and the
refusal names the constant and the figure.

**instructions** — a repository's own rules: the `AGENTS.md` at the repository root, read
once at boot and handed to the model beside the preamble. Never a nested file, never an
ancestor's; the read is capped at 8 KiB and a longer file arrives cut with a marker saying
so; `Taken` is what the read produced — `Absent`, `Found` or `Unreadable` — and an
unreadable file is a diagnostic that never stops the run
(`crates/core/src/instructions.rs`).

## The sandbox

**sandbox** — unprivileged bubblewrap, host root read-only, plus a writable set the user
chose. Read-only is a boundary on writes and not on reads, which is why a host socket the
process already holds is reachable and why a tool that would route around a refusal is
not the shape to build.

**policy** — the one sandbox value: private fields, a validating constructor and the
accessors the two derivations read (`crates/core/src/sandbox/policy.rs`). Nothing in the
tree builds a bwrap invocation by hand.

**bubblewrap** — `bwrap`, the program the policy is enforced through. A word a run reports
as its `enforcement` when it is in force; a `bwrap` that cannot start is a failure to
report and never one to paper over.

**writable set** — the paths a run may write beyond its session temp, and it stays the
user's sentence: a level that names one replaces the shipped floor rather than adding to
it (`crates/core/src/sandbox/extras.rs`).

**session temp** — the writable scratch a run's own session directory carries, mapped to
`TMPDIR`. `HOME` is the host's own, read-only, and nothing is created for it.

**unsafe** — what a run says of itself under `--yolo-unsafe`: commands run with the user's
own privileges, outside bubblewrap, inheriting environment and working directory. The
record, the JSON and the rail all say it; the flag is asked for in the invocation that
makes it and dies with that invocation.

**enforcement** — the word the JSON's last-but-two field carries for how a run was held
to the boundary: `bubblewrap` or `unsafe`.

**description** — the published text of the sandbox, derived from the policy rather than
written by hand, so the model's words and the mounts it runs under cannot disagree
(`crates/core/src/sandbox/describe.rs`). `sindri doctor` prints the same text.

**doctor** — the environment probe behind `sindri doctor`: prove bubblewrap, the required
binaries and the mount plan before any model is in the loop
(`crates/core/src/doctor.rs`). It also prints the skill catalog and the rules a run stands
in, and writes no record.

## Status and records

**status** — what a run tells its terminal it is doing: one `OSC 7501` report, written
unqueried, carrying one of the specification's closed set of states — `idle`, `working`,
`done`, `blocked`, `error` — a coarse line, the turn count and the workspace's name. The
value is `sindri_core::status::State`, the encoder is the pure `status::report`
(`crates/core/src/status.rs`), the identity is `app=sindri`, and the writing belongs to
whoever owns the tty.

**blocked** — the one state no turn sends: nothing in a turn waits on a human. The frame
sends it, with `kind=auth`, while the settings modal is asking for a key a run cannot
answer without.

**record** — the files a session leaves: `session.json`, `transcript.jsonl`, `status.json`
and the temp, written as the run goes, so a crashed run still reads
(`crates/core/src/record/mod.rs`). Two other senses of the word live in the tree and are
never a **status** report's meaning: what a terminal stores from one is the specification's
*record*, and `agent::Report` is what one run did — the answer, why it stopped and what it
cost.

**transcript** — `transcript.jsonl`: one entry per thing that happened, in order, prompt
and turns and calls and results, written as it happens and read back by `sindri sessions
show` and `/export` from the same `Entry` values so the two cannot disagree.

**session** — one run's value a front end holds to have a run: the session directory with
its temp, the record, and the identity that names them (`crates/core/src/session.rs`).
`Session::open` is the only place the order a run is started in exists.

**session id** — the GUID naming a session's directory, printed as the `session` field of
`--format json`, as the last line of `-v`, and in the rail. A record may be named by a
whole id or a leading prefix of one; an ambiguous prefix is a usage error listing the
candidates.

**redaction** — the one value holding the secrets a run knows, through which every string
the record writes passes first, so a new field cannot forget
(`crates/core/src/record/redact.rs`).

## The frame

**frame** — the terminal UI: alternate screen, streaming markdown thread, rail and
composer, launched by a bare `sindri` on a terminal (`crates/cli/src/frame.rs`). Off a
terminal the same invocation is a refusal at exit 2.

**rail** — the frame's ledger of facts, always visible: cache hits, turns against the cap,
tokens, the context meter, the session id, the temp path, the branch, the sandbox posture
and why the last run stopped.

**thread** — the transcript as the frame holds it: the entries in order and the rows each
one is worth at the width it was measured at, with a mutation the only way in so exactly
the heights a change invalidated are forgotten (`crates/tui/src/thread.rs`).

**entry** — one row of the thread, and the state a call carries while it runs: the
command, the output kept and the word its row ended on (`crates/tui/src/entry.rs`).

**composer** — the prompt being typed, held as rows and a character-index cursor and
reported in cells, because the terminal draws cells and a glyph may be two wide.

**queue** — the prompts typed while a run had the frame, oldest first, taken back or
dropped by two keys and never drawn by the module that holds them.

**pulse** — the clock a live run keeps: a braille marker at the head of the newest answer
with the elapsed time, settling on `✓` or `✗`, drawn only while something is moving.

**thought** — reasoning the run is having: the turn it belongs to, the text so far and
when it began. Held beside the thread and never in it, because a thought is a state the
run is in and not something that was said.

**pane** — the one surface a **thought** is drawn on (`render::pane`), unfolding and
folding back into the scar the thread keeps of it.

**scar** — the one row a finished thought leaves in the thread, which the whole of that
thought opens from as an overlay.

**mark** — the wash a drag leaves on the transcript. `mark::span` computes the cells from
press to release, and the copy reads the same function, so the mark cannot light a cell
the copy does not take nor leave dark one it does (`crates/tui/src/mark.rs`).

**block** — one **picture**'s rows taken as a single region: `Viewport::block` names the
picture a row belongs to (`crates/tui/src/selection.rs`), its graphic's rows and caption
are lit together, and its words are taken once. Which rows a copy takes is **mark**'s
rule, told there and not here.

**picture** — an image the thread shows: a markdown `src` resolved to a file in the
workspace, or a mermaid fence's body, measured without spending its pixels and refused
with a reason the caption can print (`crates/tui/src/picture.rs`).

**selection** — the rows a copy takes: a pair of screen cells resolved against the rows
the last frame drew, with every style dropped, so a copy is always of what the user saw.

**spill** — the box a file too large to show is opened in: read once, scrolled and
windowed while it is up, closed with `Esc` (`crates/tui/src/spill.rs`).

**overlay** — a surface that takes the screen while it is open: the binding list, a spill,
the settings modal and a finished thought. An open overlay owns its own space, and none is
the only way to do what it does.

**card** — what a broken boot draws instead of a half-drawn interface: the doctor's own
words, paged when the screen is shorter than they are, left only by a key.

**window** — where the reader is in the thread: a row of an entry and not an offset from
the tail, so neither a run that appends nor a call that opens above them moves the text
under a reader who has scrolled back (`crates/tui/src/window.rs`).

**tail** — the end of the thread, and a place the window takes again on `End`; a reader
who is scrolled back is told what arrived below them instead of being moved.

**seam** — the light strip between the thread and the composer, made once per reach and
replaced on a resize (`crates/tui/src/seam.rs`).

**face** — Sindri's face, drawn where the terminal speaks the Kitty graphics protocol and
absent, with no gap left for it, where it does not.

**completion** — the column just above the composer holding the eight nearest matches,
from `@` paths or `/` commands, the chosen one marked.

**theme** — one dark theme in truecolour, owning every colour the frame paints and the
three surfaces that make nearness legible: canvas, panel, raised tier.

## Front ends and surfaces

**front end** — a way into the same core: the frame, batch mode, `sindri sandbox exec`,
`sindri doctor` and `sindri sessions`. Each owns only what is its own, and none of them
owns a decision the core could make.

**batch mode** — the Unix filter front end (`sindri -b`), which reads the prompt and the
stdin context, runs the agent to an answer and prints it plus exactly one newline. Exit
codes, not prose, tell a script what happened, and the toolset it is offered carries no
`open` because it has no one to look.

**boot** — the one door a front end calls to turn flags and the process's own facts into a
run ready to start (`crates/cli/src/boot.rs`, `Boot`). It probes nothing: the frame proves
the sandbox and draws the card, batch prints the answer, `sandbox exec` prints the
outcome.

**stack** — a run's stack, composed once: the cancel flag, the executor over the sandbox,
`bash` with the limits its description states, the tool set, the dials the loop is wired
with and the channel its events travel on (`crates/cli/src/stack.rs`).

**failure** — the words that follow `sindri: ` and the exit code the shell reads, in one
module, so a front end never learns its failure words from another front end
(`crates/cli/src/failure.rs`).

**exit code** — what a script branches on: `0` an answer, `1` a model or provider error,
`2` a usage error, `3` a sandbox or setup failure, `100` the turn cap, `101` the budget,
`130` cancelled with nothing on stdout. `sindri sandbox exec` reports the sandbox instead,
`3` only when the sandbox failed.

**notice** — the one line a run that left the sandbox says on the way in: stderr, because
stdout is the answer or the one object a script parses (`crates/cli/src/notice.rs`).

**json output** — one run as exactly one compact JSON object on one line, its field order
the contract, later fields only appended, and a field nobody reported left out rather than
`null` (`crates/core/src/output.rs`).

**verbose** — `-v`: what the run did and what it stood in, on stderr after it ran, leaving
stdout untouched so a verbose run pipes exactly like a silent one.

## Configuration

**config file** — `$XDG_CONFIG_HOME/sindri/config.toml` and nowhere else, read by one
schema. A checked-in `.sindri.toml` is never read: a repository that could grant writable
paths would turn every clone into a privilege escalation.

**precedence** — one order for every setting, stated once: the config file, then
`SINDRI_*` environment, then flags. A repeated flag replaces the configured value rather
than adding to it, and an unknown key is an error naming the key and the file.

## The gate

**gate** — `make check`, the whole of done: fmt, clippy with warnings denied, the four
audits, the tests, the evals, proof that an unchanged tree does not relink, and the
release scripts run as processes. Nothing lands on a subset.

**audit** — the four fast checks `make audit` runs in milliseconds and the commit hook
refuses a broken commit with: no `*.rs` over 800 lines, a `//!` paragraph opening every
module, a clause beside every module declaration in a crate root, and a test name that
states the behaviour it proves.

**eval** — one scripted batch run in its own copy of a fixture repository, asserting on
exit codes, JSON fields, the record left and the files on disk rather than on the model's
prose. `make eval` replays recorded responses offline; `make eval-live` asks your own
provider at your own cost.

**fixture** — the small repository a case runs in, and the only place a fact a case turns
on may live: a house marker in one skill, a heading in one `AGENTS.md`, so an answer that
states it could not have been guessed.

**harness** — the seam a test runs the loop through, so the loop's tests need no provider
and no bubblewrap (`crates/harness`).

**mock provider** — the recorded-response provider the evals replay through: no
credential, no network, no bill.

**workspace** — the tree a run stands in: the path the sandbox mounts, the repository root
a repository scope stops at, and the directory the rail names
(`crates/core/src/workspace.rs`).

**repository root** — the nearest ancestor of the workspace holding `.git`, the ceiling a
repository scope never rises above and the directory an `AGENTS.md` is read from.
