# Sindri

An AI assistant for GNU/Linux: a Unix filter for scripts, a TUI for humans.

```console
$ git diff | sindri -b "summarise this"
```

stdout is the answer and exactly one newline; stderr is silent unless `-v`, and stdout
stays empty when a run fails or is cancelled.

**Three tools.** `bash` runs everything behind the sandbox: search, fetch, patch and HTTP
all go through the shell, and pipelines or a short script do in one turn what a
tool-per-task agent needs several. `skill` loads instructions the workspace ships —
once per conversation, in a wrapper that lists the skill's own files — and it does not
exist when there is nothing to load, so a run with no skills pays nothing for them.
`open` puts one `http://` or `https://` URL in front of you, in the browser of the
machine running sindri, and says only that the host was asked — never that you looked.
No environment inside the sandbox has a display to open a page on, so the browser is the
host's; batch mode, which has no one to look, is offered no `open` at all.
Why a skill is not a file the model reads through `bash`:
[`docs/adr/0002-two-tools.md`](docs/adr/0002-two-tools.md), and why the browser is not
one either: [`docs/adr/0003-a-third-tool-open.md`](docs/adr/0003-a-third-tool-open.md).

**Sandboxed by default.** Every `bash` call runs in unprivileged bubblewrap, host root
read-only (a boundary on writes, not reads) plus a writable set you chose. The escape
hatch is `--yolo-unsafe`: it runs one invocation's commands with your own privileges,
outside bubblewrap, inheriting your environment and working directory — and it costs the
boundary entirely, which is why the run's record, its JSON and its rail all say `unsafe`.

**Linux only.** Static `x86_64` and `aarch64` releases; an unprivileged `bwrap` that can
build a namespace is the precondition for running at all.

## Install

A static binary, checksum-verified, into `~/.local/bin` — no Rust toolchain.

```console
$ curl -fsSL https://raw.githubusercontent.com/glemsom/sindri/main/install.sh | sh
$ sindri doctor
```

The installer verifies the checksum before writing; `doctor` proves `bwrap`, the binary
set and the mount plan, and lists every skill under `~/.agents/skills` and the
repository's `.agents/skills`. Flags and the runbook: [`docs/install.md`](docs/install.md).

## Batch mode

```console
$ sindri -b "why does this test fail?" < failing.log
$ cat notes.md | sindri -b
```

Piped stdin is context appended after the prompt; with no prompt argument stdin is the
prompt, and a pipe without `-b` is refused. `-v` puts the run on stderr.

| Exit | Meaning | Exit | Meaning |
| --- | --- | --- | --- |
| `0` | an answer | `100` | turn cap |
| `1` | model or provider error | `101` | spent its command time or its output |
| `2` | usage error | `130` | cancelled, nothing on stdout |
| `3` | sandbox or setup failure | | |

`sindri sandbox exec` reports the *sandbox* instead: `0` for any outcome, `3` when the
sandbox failed, the command's own status only in the JSON.

## JSON for scripts

`--format json` prints one compact object on one line instead of the answer:

```console
$ sindri -b "summarise this diff" --format json | jq -r .session
{"answer":"…","session":"…","turns":2,"stopped":"final","model":"claude-sonnet-4-6","duration_ms":4312,"tool_calls":1,"usage":{…},"enforcement":"bubblewrap"}
```

Field order is fixed and later fields only append; `stopped` is `final`, `max_turns`,
`budget`, `cancelled` or `error`, and `enforcement` is `bubblewrap` or `unsafe` — the
last field, after `command_ms` and `output_bytes`, which say what the commands spent.
A field the run knows nothing about is left out, never `null`.

## Commands

| Command | What it does |
| --- | --- |
| `sindri` | the terminal UI, on a terminal |
| `sindri -b PROMPT` | batch: ask the model, print the answer |
| `sindri doctor` | prove the sandbox, print the description the model sees, and list the skills it found |
| `sindri sandbox exec [--timeout N] [--yolo-unsafe] -- CMD` | one command through the sandbox, no model, outcome as JSON |
| `sindri sessions list [--limit N] [--json]` | the records, newest first, one row each |
| `sindri sessions show [ID] [--json]` | one transcript: the newest record, or the one `ID` or a prefix names |
| `sindri --help`, `--version` | the surface; the build's `git describe` and commit date |

`--format` and `-v` are global, so a TUI launch accepts and ignores them.

## The terminal UI

Bare `sindri` on a terminal opens the frame — alternate screen, streaming markdown
transcript, rail, composer; off a terminal it is a refusal at exit 2.

- **First frame:** an empty thread is not a blank page — the wire, the model, the
  workspace and the sandbox, one dim sentence of the contract, and two prompts that can
  be typed as they read (with `/help` beside them). The composer holds
  the placeholder `Ask, or drop a log here · @ path · / command` until there is a draft.
- **Pulse:** while a run is live, a braille marker at the head of the newest answer
  carries the elapsed time (`⠸ 0.8s`) and settles on `✓` or `✗` with the duration it
  stopped at; each call's duration counts up and freezes with the call; the rail's output
  and context figures move on what streams and are trued up by the provider's report. The
  tick exists only while something is moving — an idle frame is never drawn.
- **Tool calls:** each call appears as it starts with the last five output lines, then
  collapses to `✓ exit 0`, `✗ exit N`, `✗ timed out`, `✗ cancelled` or `✗ sandbox: …` and
  the count of what is behind it (`· 400 lines`). `Enter` on an empty composer, or a
  click on the row, opens the call whole in place — a call that never spilled included;
  `Ctrl-O`, or a click, opens a spill file.
- **Queue:** a prompt typed during a run waits on a panel above the composer, whole and
  in order, titled `3 queued` (`queued` for one); `↑` takes the newest back into the
  composer, `Ctrl-X` drops it. `Ctrl-C` is answered at once — `cancelling…` in the footer, draft kept — and
  the run stops when the request in flight does.
- **Rail:** 32 columns, always visible — cache hits, turns against the cap, tokens, a
  context meter, session GUID, temp path, branch, sandbox posture, why the last run
  stopped — read as a ledger: labels in a nine-column field two cells in from the rail's
  first column, so no word sits against the seam, dot leaders across the gap, every value
  hanging off one edge and a `─` hairline between the blocks. Below 100 columns it becomes
  one strip, inset the same two cells; the face takes the rows left over.
- **Composer,** two to six rows: `Enter` sends, `Shift+Enter` breaks the line, `Ctrl-Q`
  leaves, `Ctrl-C` cancels a run and keeps the draft — or clears it when nothing is
  running.
- **Scroll and copy:** `PgUp`/`PgDn` and the wheel move the transcript, `Home` goes to the
  first turn and `End` back to the tail, which is followed again; a reader who is scrolled
  back is told what arrived below them (`↓ 4 new rows`) and a thread longer than the
  window marks its position in the last column. Only on-screen rows are laid out. A drag
  marks what it will take: the cells it lights from press to release are the rows the copy
  takes, styles stripped, and the mark stands after the button comes up, so `Ctrl-Y`
  copies it again — or the last answer, when no mark does. `Shift`+`↑`/`↓` grow a mark a
  row at a time up from the last row on screen, and shrink it back. A picture's block is
  one thing and copies as its words; `Shift+drag` is handed to the terminal, whose own
  selection it is. The words leave by OSC 52, falling back to `wl-copy` or `xclip`;
  a copy the terminal cannot confirm says nothing, and one a tool took names the tool.
- **`@`** walks the workspace (`.git` skipped, `.gitignore` ignored), proximity-ranked,
  capped at eight rows. `Tab`/`Enter` takes one; `Esc` dismisses, both while one is open.
- **`/`** runs `/new` (fresh session GUID, empty transcript), `/export`, `/skill`,
  `/settings`, `/help` and `/quit`. `/skill` is the one name that takes an argument, and
  the one a colon may end — `/skill:house-style` and `/skill house-style` are the same
  line — where the rest of it is the prompt the skill is put to work on and a bare name
  arms it without starting a turn; the names offered are the ones the catalog says you
  may invoke. `/export [path]` writes the thread as one markdown file, asking twice before
  replacing a file; reading a record back is `sindri sessions show`, in the shell.
  **`?` and `/help` list the same bindings**, each gesture beside its key.
- **Chrome:** a narrow terminal shortens what the frame says and never cuts a key or a
  fact in half. The footer states its seven hints from 120 columns up, five from 100 and
  never fewer than `Enter send` and `? keys`; the rail gives facts up in one priority
  order and keeps a row saying how many went (`+7 more`); the slim rail strip never
  loses the posture, the model or the workspace.
- **One thread:** a second prompt builds on the first, `/new` starts over, and a model
  changed in `/settings` answers the next turn on the same thread. **Boot runs `doctor`
  first**: a sandbox that cannot be created is a card naming the failing check. A first
  run with no API key opens the modal below instead of failing.

Each prompt is written to the session's record; the frame and the hands are this
README's own description of the screen.

### Settings

`/settings` edits the file everything else reads — one schema, one row per key below.

- `↑`/`↓` or `Tab` move between keys, `Ctrl-U` empties the field, `Space` flips
thinking, and an empty value unsets the key so the environment speaks. `Enter` writes at
once and the next prompt runs on it; `Esc` discards, and only touched fields are
written, once, atomically.
- **The API key** starts at *from environment*: nothing is written unless you type one,
where typing warns and names the plaintext file it will land in. It is masked and never
reaches the transcript or the record. The environment is read **above** the file, so an
exported `SINDRI_API_KEY` wins over it.
- **A first run needs no file.** Bare `sindri` with no key opens the frame with this
modal on `api_key` — the row says where a key comes from (`SINDRI_API_KEY` or the
provider's own variable), where it is written, that the file is `0600`, and that the
value is never shown again. `Esc` leaves a frame that still reads, saying the missing
key once and what the composer cannot send. `sindri -b` with no key is still exit 1, one
line on stderr.

## Configuration

One file, `$XDG_CONFIG_HOME/sindri/config.toml` (example:
[`docs/config.example.toml`](docs/config.example.toml)). Precedence: file, then
`SINDRI_*` environment, then flags.

| Setting | File key | Environment | Flag |
| --- | --- | --- | --- |
| Provider | `provider` | `SINDRI_PROVIDER` | `--provider` |
| Theme | `theme` | `SINDRI_THEME` | `--theme` |
| Model | `model` | `SINDRI_MODEL` | `--model` |
| Base URL | `base_url` | `SINDRI_BASE_URL` | `--base-url` |
| Credential | `api_key` | `SINDRI_API_KEY` | — |
| Reasoning | `thinking` | `SINDRI_THINKING` | `--thinking`, `--no-thinking` |
| Extra writable paths | `extra_writable_paths` | `SINDRI_EXTRA_WRITABLE_PATHS` (`:`-separated) | `--writable` (repeatable) |
| Extra request headers | `extra_headers` (table) | `SINDRI_EXTRA_HEADERS` (`Name: value` lines) | `--header` (repeatable) |
| Turn cap | `max_turns` | `SINDRI_MAX_TURNS` | `--max-turns` |
| Command timeout | `timeout` | `SINDRI_TIMEOUT` | `--timeout` |

`--yolo-unsafe` is the one flag with no file key and no environment variable: a run
that leaves the sandbox is asked for in the invocation that makes it and dies with that
invocation, because a persisted unsafe setting is the one a user forgets they turned on.
Batch mode and `sandbox exec` say it once on stderr; the frame's rail says `unsafe · your
own privileges`. Under the flag `--writable` is still validated, it simply is not what
bounds the run.

`--writable` and `--header` are repeatable but **replace** the configured values rather
than adding to them. An unknown key is an error naming key and file; a wrong value is
exit 2. The credential also comes from `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` or a
matching `*_BASE_URL`, and is redacted everywhere; a file carrying one is `0600`.

`.sindri.toml` in the working directory is **never** read: a checked-in config granting
writable paths would turn every `git clone && sindri` into a privilege escalation.

A repository's own house rules are not a setting, and they arrive all the same: the
`AGENTS.md` at the repository root — never a nested one, never an ancestor's — is read
once at boot and travels beside the preamble rather than as a turn, so the transcript
stays your words. The read is capped at **8 KiB**, a byte count rather than a line or
word count because bytes are what the request pays for, and a longer file arrives cut
with a marker naming the bytes left out. A repository without one adds nothing; one that
cannot be read is a diagnostic naming the file and the reason, and the run still starts.
`-v`, `sindri doctor` and the record's start block name the file and the bytes handed
over in one sentence. [`GLOSSARY.md`](GLOSSARY.md) defines the word, under `instructions`.

## Session records

Every run leaves a record under `$XDG_STATE_HOME/sindri/sessions/<guid>/`, written as
the run goes. `sindri sandbox exec` opens one too; `sindri doctor` writes none.

| file | what it holds |
| --- | --- |
| `session.json` | provider, model, enforcement (`bubblewrap`), the policy the model was handed, the workspace and its branch and commit, the dials the run obeyed, start, end, turns, usage, and why it stopped |
| `transcript.jsonl` | one entry per thing that happened — prompt, turns, tool calls, tool results — in order |
| `status.json` | bubblewrap's own status documents, one per line, as they arrived |
| `tmp/`, `tmp/home/` | the session temp, and the empty `HOME` the sandbox mounts inside it |

The GUID is the `session` field of `--format json` and the last line of `-v`;
`sindri sessions list` prints every record newest first, whole GUIDs and all, and
`sindri sessions show` reads one back — the newest when no id is named, or the one a
whole id or a leading prefix of one names, with an ambiguous prefix a usage error that
lists the candidates. `show --json` prints the same record as one object of
`meta`, `entries` and `status`: the files on disk in their own shape, so `jq` is the
debugger and `cat transcript.jsonl` cannot disagree with it.

Written incrementally, so a cancelled or `kill -9`ed run still reads; a session `/new`
supersedes keeps saying `running`, because it did not stop, it was left.

Fields are only ever appended, and every field added after a record was written is
defaulted rather than required: a record from any earlier sindri still reads, and says
nothing about what it never recorded. `version` is the build that wrote the run — per-run
provenance, **not** a schema version; the record has none. The known gap:
nothing is pruned, ever — every run keeps its transcript, spill files and session temp.
Deliberate for v0.1; `sessions prune` is a later issue.

## Evals

Four promises cannot be checked from a run's output: that the sandbox blocks what it
claims, that the turn cap stops a loopy run, that a skill the workspace ships is loaded
before the answer that needed it, and that a repository's own `AGENTS.md` changes what a
run does. Ten scripted batch runs (`make eval`) prove them.

Each eval runs the real binary in its own copy of a small fixture repository and asserts
on exit codes, JSON fields, what the model was shown, the record it left and the files on
disk — never on the model's prose, which drifts. CI replays recorded responses through
the mock provider: no credential, no network, no bill. Eval 7 writes to paths the
read-only root protects, so the suite **refuses to run** when the sandbox is not
enforcing.

Eval 9 is the one case that reads an answer: its rule (*the house marker on a script's
second line*) lives in the fixture's one skill and nowhere else in the tree, so an answer
that states it arrived through the catalog and could not have been guessed. The case
fails on an answer without the skill call, on a first turn already carrying the rule, and
on a script written without it.

Eval 10 is the case that turns on the repository's own rules: its fixture carries an
`AGENTS.md` whose answer heading and forbidden command appear in no other file of the
tree, so a run that obeyed them was handed them. The check fails when that file is gone
from the fixture, when the block the model read arrived truncated at the instructions cap,
and when the record shows the forbidden command was run — the cap and the rules are held
from the run's side, not from the answer's word for it.

`make eval-live` runs the same ten prompts against your own provider and credential,
at your own cost, skipping the assertions that need a recorded conversation. See
[`crates/evals/fixtures/README.md`](crates/evals/fixtures/README.md).

## Working on Sindri

Everything goes through devbox, and `make check` is the definition of done.

Run `make hooks` once per clone. It points `core.hooksPath` at `.githooks`, and every
commit then runs `make audit` — the fast audits, in milliseconds — before it lands, so a
broken commit is refused while it is still the only one. It needs no devbox; nothing is
wired until you ask, so a fresh clone commits freely. `git commit --no-verify` skips it,
and `make check` still is the gate: the hook moves the cheap part earlier, it does not
narrow what is required.

| Target | What it runs |
| --- | --- |
| `make check` | fmt, clippy, the four audits (800 lines, module docs, the module map, test names), tests, evals, the no-relink proof, the release scripts |
| `make audit` | the four fast audits on their own, in milliseconds |
| `make hooks` | enable the pre-commit hook: those audits, before every commit |
| `make build` / `install` / `clean` / `run ARGS=…` | release build / install the tree's binary / `cargo clean` / run it |
| `make eval` / `eval-live` / `scripts-check` | the evals, offline or against your provider; the release scripts, offline |
| `make release TAG=v0.1.0` | both static binaries, `SHA256SUMS`, changelog |

A tag is the only thing a release needs; the workflow builds, verifies and publishes it
(see [`docs/install.md`](docs/install.md)). [`GLOSSARY.md`](GLOSSARY.md) has the
vocabulary, [`docs/adr/0001-llm-friendly-code.md`](docs/adr/0001-llm-friendly-code.md)
how the code is written.
