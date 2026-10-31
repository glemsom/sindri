# Sindri

An AI assistant for GNU/Linux: a Unix filter for scripts, a TUI for humans.

```console
$ git diff | sindri -b "summarise this"
```

stdout is the answer and exactly one newline; stderr is silent unless `-v`, and stdout
stays empty when a run fails or is cancelled.

**One tool.** The model gets `bash` and nothing else: search, fetch, patch and HTTP all
go through the shell, and pipelines or a short script do in one turn what a
tool-per-task agent needs several.

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
set and the mount plan. Flags and the runbook: [`docs/install.md`](docs/install.md).

## Batch mode

```console
$ sindri -b "why does this test fail?" < failing.log
$ cat notes.md | sindri -b
```

Piped stdin is context appended after the prompt; with no prompt argument stdin is the
prompt, and a pipe without `-b` is refused. `-v` puts the run on stderr.

| Exit | Meaning | Exit | Meaning |
| --- | --- | --- | --- |
| `0` | an answer | `3` | sandbox or setup failure |
| `1` | model or provider error | `100` | turn cap |
| `2` | usage error | `130` | cancelled, nothing on stdout |

`sindri sandbox exec` reports the *sandbox* instead: `0` for any outcome, `3` when the
sandbox failed, the command's own status only in the JSON.

## JSON for scripts

`--format json` prints one compact object on one line instead of the answer:

```console
$ sindri -b "summarise this diff" --format json | jq -r .session
{"answer":"…","session":"…","turns":2,"stopped":"final","model":"claude-sonnet-4-6","duration_ms":4312,"tool_calls":1,"usage":{…},"enforcement":"bubblewrap"}
```

Field order is fixed and later fields only append; `stopped` is `final`, `max_turns`,
`cancelled` or `error`, and `enforcement` is `bubblewrap` or `unsafe`. A field the run
knows nothing about is left out, never `null`.

## Commands

| Command | What it does |
| --- | --- |
| `sindri` | the terminal UI, on a terminal |
| `sindri -b PROMPT` | batch: ask the model, print the answer |
| `sindri doctor` | prove the sandbox, print the description the model sees |
| `sindri sandbox exec [--timeout N] [--yolo-unsafe] -- CMD` | one command through the sandbox, no model, outcome as JSON |
| `sindri sessions show ID` | print one session's transcript |
| `sindri --help`, `--version` | the surface; the build's `git describe` and commit date |

`--format` and `-v` are global, so a TUI launch accepts and ignores them.

## The terminal UI

Bare `sindri` on a terminal opens the frame — alternate screen, streaming markdown
transcript, rail, composer; off a terminal it is a refusal at exit 2.

- **First frame:** an empty thread is not a blank page — the wire, the model, the
  workspace and the sandbox, one dim sentence of the contract, and three things that can
  be typed as they read (with `/sessions` and `/help` beside them). The composer holds
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
- **Scroll and copy:** `PgUp`/`PgDn` and the wheel move the transcript, `Home` goes to
  the first turn and `End` back to the tail, which is followed again; a reader who is
  scrolled back is told what arrived below them (`↓ 4 new rows`) and a thread longer than
  the window marks its position in the last column. Only on-screen rows are laid out. Dragging selects rendered rows; releasing copies them, styles
  stripped, through OSC 52, falling back to `wl-copy` or `xclip` — *sent*, since a
  terminal never confirms.
- **`@`** walks the workspace (`.git` skipped, `.gitignore` ignored), proximity-ranked,
  capped at eight rows. `Tab`/`Enter` takes one; `Esc` dismisses, both while one is open.
- **`/`** runs `/new` (fresh session GUID, empty transcript), `/settings`, `/help`,
  `/sessions`, `/export` and `/quit`. `/sessions` reads a record back read-only — reading
  is not resuming; `/export [path]` writes the thread as one markdown file, asking twice
  before replacing a file. **`?` and `/help` list the same bindings**, each gesture beside
  its key.
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

## Session records

Every run leaves a record under `$XDG_STATE_HOME/sindri/sessions/<guid>/`, written as
the run goes. `sindri sandbox exec` opens one too; `sindri doctor` writes none.

| file | what it holds |
| --- | --- |
| `session.json` | provider, model, enforcement (`bubblewrap`), start, end, turns, usage, and why it stopped |
| `transcript.jsonl` | one entry per thing that happened — prompt, turns, tool calls, tool results — in order |
| `status.json` | bubblewrap's own status documents, one per line, as they arrived |
| `tmp/`, `tmp/home/` | the session temp, and the empty `HOME` the sandbox mounts inside it |

The GUID is the `session` field of `--format json` and the last line of `-v`; `sindri
sessions show ID` reads a record back.

Written incrementally, so a cancelled or `kill -9`ed run still reads; a session `/new`
supersedes keeps saying `running`, because it did not stop, it was left. The known gap:
nothing is pruned, ever — every run keeps its transcript, spill files and session temp.
Deliberate for v0.1; `sessions prune` is a later issue.

## Evals

Two promises cannot be checked from a run's output: that the sandbox blocks what it
claims, and that the turn cap stops a loopy run. Eight scripted batch runs (`make eval`)
prove them.

Each eval runs the real binary in its own copy of a small fixture repository and asserts
on exit codes, JSON fields, what the model was shown and the files left on disk — never
on the model's prose, which drifts. CI replays recorded responses through the mock
provider: no credential, no network, no bill. Eval 7 writes to paths the read-only root
protects, so the suite **refuses to run** when the sandbox is not enforcing.

`make eval-live` runs the same eight prompts against your own provider and credential,
at your own cost, skipping the two assertions that need a recorded conversation. See
[`crates/evals/fixtures/README.md`](crates/evals/fixtures/README.md).

## Working on Sindri

Everything goes through devbox, and `make check` is the definition of done.

| Target | What it runs |
| --- | --- |
| `make check` | fmt, clippy, the 400-line audit, tests, evals, the no-relink proof, the release scripts |
| `make build` / `install` / `clean` / `run ARGS=…` | release build / install the tree's binary / `cargo clean` / run it |
| `make eval` / `eval-live` / `scripts-check` | the evals, offline or against your provider; the release scripts, offline |
| `make release TAG=v0.1.0` | both static binaries, `SHA256SUMS`, changelog |

A tag is the only thing a release needs; the workflow builds, verifies and publishes it
(see [`docs/install.md`](docs/install.md)). [`GLOSSARY.md`](GLOSSARY.md) has the
vocabulary, [`docs/adr/0001-llm-friendly-code.md`](docs/adr/0001-llm-friendly-code.md)
how the code is written.
canary-1793386843
