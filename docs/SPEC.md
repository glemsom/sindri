# Sindri — capability spec

What Sindri is, what it must do, and what makes it different from a generic "LLM with
tools" agent. This is acceptance criteria, not a plan: it says *what must be true*, `docs/adr/`
says *why it is built this way*, `GLOSSARY.md` says *what the words mean*, and the issue
tracker says *when*. A `§N` here names a section of this file; `ADR-NNNN` names a record in
`docs/adr/`.

## 1. One sentence

- An AI agent for GNU/Linux that is *two things at once*: a **Unix filter for scripts**
  (`stdin → stdout`, exit codes, JSON) and a **terminal UI for humans** (streaming markdown, tool calls, a rail of live facts).
- Same core, several front ends. No front end owns a decision the core could make.

## 2. Design rules that shape everything

- **stdout is a contract.** Batch mode's stdout is the answer plus exactly one newline, and
  nothing at all when the run failed or was cancelled. All chatter goes to stderr.
- **Exit codes are the interface.** A script never parses prose to learn what happened.
- **Three tools at most.** `bash` runs everything; `skill` and `open` exist only where a
  shell cannot decide on the model's behalf. A front end registers what it honours, so batch
  mode has two.
- **The boundary the model is told about is the boundary enforced.** Tool descriptions are
  *derived* from the live sandbox policy and limits, never hand-written and never drifting.
- **Absence over refusal.** A tool a front end cannot honour is not registered at all, so
  the model's schema never mentions it.
- **Enforced permissions, not requested ones.** A permission nobody enforces is a
  suggestion — so skills get flags the model literally cannot call around.
- **Nothing is invented that the wire can answer.** Model names come from the provider, not
  from a hardcoded list.
- **Safe by default, unsafe only by invocation.** The escape hatch cannot be persisted.

## 3. Front ends

- `sindri` — the TUI, on a terminal; off a terminal it is a refusal at exit 2.
- `sindri -b PROMPT` — batch: ask, print the answer. Reads piped stdin as context appended
  after the prompt; with no prompt argument stdin *is* the prompt; a pipe without `-b` is
  refused.
- `sindri doctor` — prove the sandbox, print the description the model sees, list every skill
  found with its scope (builtin / user / repo) and any shadowing. A sandbox it cannot prove is a
  card naming the failing check, at exit 3.
- `sindri sessions list|show` — read past runs back.
- `sindri auth login copilot` — OAuth device-flow sign-in.
- `--format json` and `-v` are global, so a TUI launch accepts and ignores them. The JSON
  contract below is batch mode's, so a TUI launch writes nothing to stdout.
- `--yolo-unsafe` — run without the sandbox at all.

## 4. The run loop (the agent)

- **Turn** = one request to the model and the calls it asks for. Hard cap `max_turns`
  (default **250**) so a loopy run always stops and says why.
- **Streaming**: text and reasoning stream to the front end as they land; each has its own
  gutter and ink.
- **Loaded-skill set carries across turns** within a run; cleared when history is cleared;
  two identical loads in one wave are one load.
- **Two budgets, separate from the turn cap** (batching hides work inside one script, so a
  per-turn limit would be spent per script and not per run):
  - command wall clock per run (default **1 hour**)
  - bytes of command output fed back (default **8 MiB**)
- **Per-call timeout** (default **120s**, max 3600) — a killed command reports `timed_out`.
- **Retry policy** on the *verdict* the wire classified, not the status code: timeouts,
  rate limits, server faults and transport failures retry (3 attempts, 200 ms doubling);
  client errors do not. Never retry a reply that already streamed text, or a cancel.
- **Cancellation** is first class: SIGINT stops the run when the in-flight request does;
  exit 130 with empty stdout; the draft in the composer survives.
- **Stopped set** (closed, and reported everywhere): `final`, `max_turns`, `budget`,
  `cancelled`, `error`. Records say `running` until one lands.

## 5. Tools

### `bash` — the only way to act

- All of search, fetch, patching and HTTP go through the shell, so one pipeline or a short script does in a single turn what a tool-per-task agent needs several for.
- Its description is generated from the policy in force: the writable paths, the per-call timeout, the run's command budget.

### `skill` — instructions
- Global skills sources from `~/.agents/skills`, project skills sources from `repo-root/.agents/skills`
- A first-class tool, *not* a file read through `bash`: a call names a value from a catalog
- Loading returns the body with frontmatter stripped, in a wrapper, **with the skill's own files listed** so relative references resolve.
- Loads **once per conversation**; a second call returns `already_loaded` and no body (a tool result is re-sent with every later turn).
- **Catalog lives in the tool's description**, not the preamble: it changes with the
  workspace, so it is paid only for what exists. No model-visible skill ⇒ no tool at all ⇒
  the preamble stays byte-identical.
- Two permission flags, read as plain YAML booleans only (Claude-compatible):
  - `disable-model-invocation: true` → user-only skill; absent from schema and catalog.
  - `user-invocable: false` → model-only skill; never offered in the frame's completion.
- Skills also load from the *human* side: `/skill:name` and `/skill name` are the same line.

### `open` — hand one target to the host's browser

- One target: exactly one of `url` (http/https) or `path` (a file), empty means absent.
- Refusals live in the call, not the shell: scheme allowlist, no authority but empty or
  `localhost`, an **extension allowlist** (html, htm, pdf, png, jpg, jpeg, gif, webp, svg,
  txt, md, csv) with every one named in a refusal, containment inside the writable set +
  session temp, and a regular file only.
- Containment and extension are checked **twice**: on the string the model wrote, and on
  the path it resolves to (a symlink to `/` is the escape).
- The **host** spawns the opener (the sandbox has no display); the target is one argv word
- Absent in batch mode: no `open` where there is no one to look.

## 6. Sandbox

- Every `bash` call runs in **unprivileged bubblewrap**: host root read-only (a boundary on writes, not reads) plus a writable set the user chose.
- Per-session **temp** mapped to `TMPDIR`: writable scratch that lives as long as the session, so calls in one run see each other's files and no run sees another's.
- Writable set invariant: canonical, absolute, distinct, non-nested, never `/`.
- `--yolo-unsafe` is the one escape hatch: commands run with the user's own privileges,
  outside bubblewrap, inheriting environment and cwd. It has **no config key and no
  environment variable** — it dies with the invocation, because a persisted unsafe setting
  is one the user forgets. Record, JSON and rail all say `unsafe`.
- Boot proves the sandbox: a sandbox that cannot be created is a card naming the failing check, not a silent fallback.
- Linux only (`x86_64`), static musl release.

## 7. Skills on disk

- Three scopes, lowest precedence first: `builtin` (compiled into the binary), `user`
  (`~/.agents/skills`), `repo` (every `.agents/skills` from the workspace up to the repository root, nearest first, never above it).
- A nearer scope takes a name from a further one; the loser is reported as **shadowing**.
- A directory with a `SKILL.md` is a skill; directory symlinks are followed (that is how people share them); a dangling link is refused by its path.
- A `SKILL.md` that will not parse is a diagnostic naming the line, never a failed catalog and never a silence.
- **Built-ins are `include_str!`** — no path, because a path can be absent, stale or
  unwritable. Instructions and the binary that enforces them ship in one artefact.

## 8. Repository instructions

- The **`AGENTS.md` at the repository root** — never a nested one, never an ancestor's — is read once at boot and appended to the preamble.
- Appended to the **preamble**, not sent as a turn: the transcript stays the user's words.
- Absent → nothing added. Unreadable → a diagnostic naming file and reason; the run still starts. `-v`, `doctor` and the record's start block name the file and bytes given over.
- `AGENTS.md` is deliberately *not* a setting: house rules arrive without being configured.

## 9. Preamble (system message)

- Re-sent every turn, so **every word is a tax**.
  - `## Persona` — who is answering.
  - `## Principles` — a handful of habits.
  - `## Environment` — GNU/Linux, `bash` first, staged commands with `pipefail`, `$TMPDIR` for anything a run makes, and the one working directory the run stands in.
  - Then `<project_instructions>` from `AGENTS.md`, if any.

## 10. Providers

- One seam: a call asks and gets an answer or a failure safe to print. Nothing above the seam knows the client library exists — exactly one file reads it.
- Three wire dialects: Anthropic Messages, OpenAI Chat Completions, OpenAI Responses
  (chosen by model name where the identifier demands it).
- Wires: `anthropic`, `openai`, `copilot` — the provider row offers exactly the words the
  parser accepts, so a fourth wire cannot be invented in the UI.
- Per-wire config: credential variable, base URL variable, whether a credential is needed
  at all. Falls back to `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / matching `*_BASE_URL`.
- **Served names**: model listings are read off the wire and never invented, kept beside the
  `(wire, base_url)` pair that produced them, and dropped if the wire changed before they
  landed. A wire that cannot answer has no list — a hint, never a refusal.
- Reasoning/thinking toggle per run.
- Extra request headers, with `{session}` expanded to the session id so a gateway that caches per session follows `/new`.
- Copilot: OAuth device flow, token store owner-only and separate from the config file; a Copilot session is never treated as an API key.

## 11. Configuration

- One file: `$XDG_CONFIG_HOME/sindri/config.toml`. One schema. One owner-only (0600) atomic save. No other command writes it.
- Precedence everywhere: **file, then `SINDRI_*` environment, then flags.** The environment is read *above* the file, so an exported key wins.
- A repeated flag **replaces** the configured value rather than adding to it.
- An unknown key is an error naming the key and the file; a bad value is exit 2.
- **`.sindri.toml` in the working directory is never read** — a checked-in config granting
  writable paths would turn `git clone && sindri` into privilege escalation.
- Keys: provider, model, base_url, api_key, thinking, extra_writable_paths, extra_headers,
  max_turns, timeout, command_budget_ms, output_budget_bytes, status, ssh_agent.
  `--yolo-unsafe` is deliberately not among them.
- Secrets: masked, never in the transcript, never in a record, redacted everywhere.
- A first run needs no file: an API-key row that says where a key comes from, where it will
  be written, that the file is 0600, and that the value is never shown again.

## 12. Terminal UI (the differentiators live here)

- **Frame**: alternate screen, streaming markdown transcript, rail, composer. An idle frame is never drawn.
- **Pulse**: while a run is live, a braille marker at the head of the newest answer carries elapsed time and settles on `✓`/`✗` with the duration it stopped at.
- **Tool calls as rows**: `✓ exit 0` / `✗ exit N` / `✗ timed out` / `✗ cancelled`, painted in faded ink.
- **Queue**: a prompt typed during a run waits above the composer, whole and in order
  (`3 queued`); `↑` on the composer's first line takes the newest back and anywhere else moves
  the caret; `Ctrl-X` drops it.
- **Rail** — 32 columns, always visible, read as a ledger (labels in a fixed field, dot
  leaders, one hanging edge, hairline between blocks): cache hits, turns vs cap, context meter,
  model, workspace, branch, session GUID, temp path, skills in force, sandbox posture. Below 100
  columns it becomes one strip. Facts are given up in one order — temp path, cache hits, session
  GUID, branch, skills in force, context meter, workspace, turns, model — and sandbox posture
  never goes.
- **Composer** 2–6 rows: `Enter` sends, `Shift+Enter`/`Ctrl-J` newline, `Ctrl-Q` quits,
  `Ctrl-C` cancels and keeps the draft (clears it when idle). A pasted prompt arrives as one
  draft with its line breaks kept.
- **Scroll and copy**: `PgUp`/`PgDn`/wheel, `Home`/`End` with tail-follow; a reader scrolled
  back is told what arrived below (`↓ 4 new rows`); only on-screen rows are laid out.
  A drag **marks exactly the cells the copy will take** (`mark::span` is shared by mark and
  copy, so they cannot disagree); the mark persists so `Ctrl-Y` copies again, or copies the last answer when there is no mark. `Shift+↑/↓` grow the mark a row at a time. Words leave by OSC 52.
- **`@` path picker**: walks the workspace as `git` reads it and honours `.gitignore` — a file
  the run can read is a file the reader may point at. Proximity-ranked, capped at 8 rows.
- **`/` commands**: `/new`, `/skill[:name]`, `/model`, `/settings`, `/help`, `/quit`.
- **`/model`** opens a screen of the names the wire serves, ranked as you type; `Enter`
  writes the chosen name through the same owner-only atomic save; `/model <name>` sets a
  name the wire never offered, by the same write.
- **Settings modal**: arrow/Tab between keys, `Ctrl-U` empties, `Space` flips a toggle, an empty value *unsets* the key so the environment speaks. `Enter` writes at once and the next prompt runs on it; `Esc` discards; only touched fields are written, once, atomically.
- **Chrome degrades by priority, never by truncation**: footer shows 7 hints from 120 cols,
  5 from 100, never fewer than `Enter send` and `? keys`; the rail gives facts up in the
  order above and keeps a row saying how many went (`+7 more`); a key or a fact is whole or it is
  not drawn.
- **Markdown rendering**: headings, tables, code with syntax highlighting, and **pictures** —
  an image `src` resolved inside the workspace, or a **mermaid fence** rasterised — measured without spending pixels and refused with a reason the caption can print. A picture's block
  copies as its words. 
- **Thought rows**: reasoning streams as its own row in its own ink; nothing to fold, nothing to open.

## 13. Records and sessions

- Every run writes a session under `$XDG_STATE_HOME/sindri/sessions/<guid>/`, **as it goes**, so a cancelled or `kill -9`ed run still reads:
  - `session.json` — provider, model, enforcement, the policy handed to the model, workspace,
    branch, commit, dials obeyed, start, end, turns, usage, why it stopped.
  - `transcript.jsonl` — one entry per thing that happened, in order.
  - `status.json` — the sandbox's own status documents, one per line.
  - `tmp/` — the session temp; emptied at the end, the directory and `spill/` kept.
- The GUID is the `session` field of the JSON and the last line of `-v`.
- `sessions list` newest first; `sessions show [ID]` reads one back (whole id or unambiguous
  prefix — an ambiguous prefix is a usage error listing the candidates); `show --json` prints
  the files in their own shape, so `jq` and the transcript cannot disagree.
- A session `/new` superseded keeps saying `running` — it did not stop, it was left.

## 14. Status reporting (OSC 7501)

- The program says what it is doing over `OSC 7501`: `idle`, `working`, `done`, `blocked`,
  `error`, with one coarse line of its own words, the turn count and the workspace's name.
- Written **unqueried** — the detection query is refused (it needs a stdin read a filter
  does not do, and it false-negatives on the terminals we run in). Unsupported terminals
  ignore an unknown OSC, which costs what a pipe costs.
- Never `progress`: turn 12 of 250 is not 5% of anything, and absent progress already means indeterminate. Never `9;4`.
- `msg` is never the command, the prompt or the answer — the rule is "nothing worth
  redacting is ever put in it", because a tty is a channel the record's redaction cannot
  reach.
- Batch mode writes them to **stderr only when stderr is a terminal**; a file or a pipe gets
  nothing. One switch: `status` / `SINDRI_STATUS`, on by default.
- Pure encoder in the core, the writing left with whoever owns the tty. Rate-limited: a state
  already told is not repeated, nothing twice inside a second, a withheld report is said late
  rather than lost.

## 15. Output contracts

- **Exit codes**: `0` answer, `1` model/provider error, `2` usage error, `3` sandbox/setup failure, `100` turn cap, `101` budget (command time or output), `130` cancelled with
  nothing on stdout.
- **JSON**: one compact object on one line. Fixed field order; later fields only append; a
  field the run knows nothing about is **left out, never `null`**.
  Fields: `answer`, `session`, `turns`, `stopped`, `model`, `duration_ms`, `tool_calls`,
  `usage`, `enforcement` (`bubblewrap` | `unsafe`), `command_ms`, `output_bytes`.
- **Verbose** (`-v`) puts the run on stderr, ending with the session GUID.

## 16. Quality machinery (this is itself a capability)

- `make check` is the single definition of done: fmt, clippy with warnings denied, four
  audits, tests, evals, a no-relink proof, and the release scripts run as processes.
- **Four fast audits** (`make audit`, milliseconds, also wired into a commit hook):
  - every module opens with a `//!` paragraph of at least eight words;
  - every `mod` in a crate root trailed by one clause cut from that module's paragraph;
  - the reverse: a file under a tests tree that no declaration names is reported where it lies;
  - every `#[test]` name states the behaviour, with no `test_`/`it_`/`works`/`check_` opener.
- **No-relink proof**: the binary embeds `git describe` + commit date, never the wall clock, so an unchanged tree must not relink.

## 17. Assumptions and what makes Sindri different

- **A rich terminal**: we assume a terminal that speaks the Kitty graphics protocol. That is
  what buys pictures and diagram rasterisation.
- **A filter and a TUI over one core** — the same run, scriptable or watchable.
- **Three tools** — composition over enumeration. A shell is the universal tool; the other two
  exist only for what a shell cannot decide.
- **Descriptions derived from enforcement** — the model is told the boundary in force, and
  cannot be told a stale one.
- **Skills as a tool with a catalog, permissions and dedup** — built-in skills compiled into the
  binary, so instructions and enforcement ship together.
- **Cancellation, budgets and stop reasons as product features**, not error paths, over records
  written as the run goes and read back by the same `Entry` values.
- **Safety posture with no persisted unsafe mode**, and a repo-local config that is never read.
- **Evals that assert on artefacts, not prose** — and benches that decide wording.
