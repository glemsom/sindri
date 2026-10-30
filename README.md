# Sindri

An AI assistant for GNU/Linux, in two shapes: a Unix filter for scripts and a
terminal application for humans.

```console
$ git diff | sindri -b "summarise this"
```

stdout is the answer and exactly one newline; stderr is silent unless `-v`.
Every `bash` call the model makes runs inside bubblewrap with a read-only host
root — a boundary on writes, not on reads: the host root, credentials included,
stays readable in there (ADR-0010) — and a writable set you chose. See
`capabilities-bullets.md` for the whole idea, `GLOSSARY.md` for the vocabulary,
and `docs/adr/` for the decisions.

## Install

No Rust toolchain needed: a static binary, checksum-verified, into
`~/.local/bin`.

```console
$ curl -fsSL https://raw.githubusercontent.com/glemsom/sindri/main/install.sh | sh
```

Releases carry `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`
binaries with a `SHA256SUMS` file and a changelog. The installer checks the
checksum before writing anything and ends by telling you to run
`sindri doctor` — the sandbox is the precondition for the agent, so prove it on
your machine before trusting a run. Installer flags, the release contents and
the maintainer's release runbook: [`docs/install.md`](docs/install.md).

## Batch mode

```console
$ sindri -b "why does this test fail?" < failing.log
$ cat notes.md | sindri -b
```

Piped stdin is context appended after the prompt; with no prompt argument
stdin is the prompt. A pipe without `-b` is refused, so no data is ever
silently swallowed. Exit codes: `0` answer, `1` model or provider error, `2`
usage error, `3` sandbox or setup failure, `100` turn cap, `130` cancelled
with nothing on stdout — so a script can branch without parsing prose.

### For scripts

`--format json` prints one compact object on one line instead of the answer:

```console
$ sindri -b "summarise this diff" --format json | jq -r .session
{"answer":"…","session":"…","turns":2,"stopped":"final","model":"claude-sonnet-4-6","duration_ms":4312,"tool_calls":1,"usage":{…}}
```

The field order is fixed and later fields only append, so a parser written
today keeps working. `stopped` is one of `final`, `max_turns`, `cancelled`,
`error`. Nothing the run has nothing to say about — an unreported model, a
provider that billed nothing — is left out rather than sent as `null`, so a
script can tell "not available" from "empty". Exit codes do not depend on the
format. See [ADR-0004](docs/adr/0004-output-contract-and-exit-codes.md).

## The terminal UI

Bare `sindri` on a terminal opens the frame: the alternate screen, a transcript
column, a rail and a composer.

```console
$ sindri
```

Piped stdin is still refused with exit 2, and `-b` is untouched, so the filter
is exactly where it was. Launching outside a terminal is the refusal, not a
degraded frame.

- **The transcript** is streamed markdown: headings, nested lists, tables,
  blockquotes, inline code, and fenced code with truecolour syntax
  highlighting. Diffs are colour-coded line by line.
- **Each `bash` call** appears the moment it starts — one line for the script
  and the last five lines of its output while it runs — then collapses to
  `✓ exit 0` or `✗ exit N`, `✗ timed out`, `✗ cancelled`, or `✗ sandbox: …`.
  Output too large to show becomes an affordance: `Ctrl-O`, or a click on the
  row, opens the untruncated text from the spill file the executor already
  wrote.
- **The rail** is 32 columns on the right and always visible: cache hit
  percentage, turns against the cap, tokens, a context meter, the session GUID,
  the session temp path, the git branch and the sandbox posture, plus the reason
  the last run stopped. It reads the same at 100 columns as at 160. Below 100 it
  becomes the same facts as one strip under the transcript — never hidden behind
  a keystroke. The face takes only the rows the facts leave, so a terminal too
  short for both keeps the posture and the record and loses the picture.
- **The composer** is at the bottom: `Enter` sends, `Shift+Enter` breaks the
  line, `Ctrl-C` cancels the run and keeps the draft, `Ctrl-Q` leaves. One flag
  serves the whole launch, raised by the key or by `SIGINT` and put down between
  prompts, so a cancel ends the run it was raised for and never the next. A
  prompt typed while a run is in flight queues and is sent when the run ends.
- **Scrolling and selection.** `PgUp`/`PgDn` and the wheel move the transcript,
  and only the rows on screen are ever laid out, so an unbounded thread still
  draws a bounded frame. Dragging over the transcript selects rendered rows;
  releasing copies exactly those rows, styles stripped, through the terminal's
  own OSC 52 — falling back to `wl-copy` or `xclip` when there is no terminal to
  address. A terminal does not confirm a copy, so the frame says *sent*, not
  *copied*; a copy nothing took is reported as a refusal, never as success.
  `Ctrl-Y` copies the last answer for keyboards, and `Shift+drag` hands the
  selection back to your terminal, so `tmux` and old habits still work.
- **`@` completes workspace paths.** Typing `@` starts a background walk of the
  workspace — `.git` skipped, `.gitignore` deliberately not honoured — ranked by
  proximity to what you typed and capped at eight rows. `Tab` or `Enter` takes
  one, `Esc` dismisses the offer, and a dotfile is offered only once you type a
  dot.
- **`/` runs commands.** `/new` starts a fresh session GUID with an empty
  transcript; a `/new` that cannot start says why and leaves the conversation
  where it was. `/settings` opens the modal below, `/help` lists every binding,
  `/quit` leaves.
- **The conversation is one thread.** A second prompt builds on the first: the
  transcript and the model's history grow together, tool calls included, and
  `/new` is what starts over.
- **`?` lists every binding**, on an empty prompt: the keys, the mouse gesture
  beside each, and the key that does the same thing as every gesture.
- **Boot runs `doctor`'s checks first.** A sandbox that cannot be created is a
  full-screen card naming the failing check, before anything else draws.
- **The face** renders where the terminal speaks Kitty graphics and is silent
  everywhere else: there is no placeholder, and the rail simply closes up.

Answers stream as they are written, so the frame is never frozen for the length
of a model call. Each prompt is recorded in the same session record as a batch
run (ADR-0006).

Copying, scrolling, `@` completion, the `/` commands and the help overlay are
[ADR-0013](docs/adr/0013-the-hands-copy-scroll-and-completion.md)'s decisions;
the settings modal, the conversation that survives a change of model, and the
rail's last polish are
[ADR-0014](docs/adr/0014-the-dials-one-schema-one-writer.md)'s.

### Settings

`/settings` edits the file everything else reads — the same
`$XDG_CONFIG_HOME/sindri/config.toml`, the same schema, the same checks — so
there is one source of truth and no second parser:

```text
settings · /home/you/.config/sindri/config.toml
  provider             anthropic
  model                claude-sonnet-4-6
  base_url
  api_key              from environment
  max_turns            250
  timeout              120
  thinking             off (default)
  extra_writable_paths
  extra_headers
```

- **`↑`/`↓` or `Tab`** move between the keys, **`Ctrl-U`** empties the field
  with the cursor, **`Space`** flips the thinking toggle, an empty value unsets
  the key so the environment or the default speaks instead.
- **`Enter` writes the file immediately**, and the next prompt runs on it — no
  restart, no lost session. The front end reads the config again and rebuilds
  the sandbox policy over the session already in flight, so a saved extra
  writable path is in force for the very next command. A model changed here
  answers the next turn **on the same thread**.
- **`Esc` discards** everything you typed. Nothing is written until `Enter`, and
  then only the fields you touched: the modal stages them in memory, validates
  them against the schema, and writes once, atomically. A key the file holds and
  you never touched stays in the file.
- **An unknown key or a bad value** is an error, exactly as it is for a run: the
  modal opens over a file the schema refuses only to say so in the transcript,
  and a value that fails on save keeps the modal open and the file untouched.

**The API key.** The field starts at *from environment*, and that is the one
default: nothing is written to disk unless you type a key into that field. Typing
is the explicit act, and it turns on a warning that names the file the value
will land in — in plaintext. The value is masked while you type, never echoed
into the transcript, and never in the session record: the modal cannot print a
key even by accident, because the config document does not hand one back.
Emptying the field is the explicit act that returns to the environment.

One thing the modal cannot see: the environment is read **above** the file, so
a run with `SINDRI_API_KEY` (or the provider's own variable) exported uses that
credential whatever the file says. The modal reports what the file holds, which
is all it can honestly report.

## Session records

Every run leaves a record under `$XDG_STATE_HOME/sindri/sessions/<guid>/`
(ADR-0006), written as the run goes rather than at the end. `sindri sandbox
exec` opens a session the same way — no model, no turns — while `sindri doctor`
writes no record at all:

| file | what it holds |
| --- | --- |
| `session.json` | provider, model, whether the run was sandboxed or `unsafe`, start, end, turns, usage and why it stopped |
| `transcript.jsonl` | one entry per thing that happened — prompt, assistant turns, tool calls, tool results — in order |
| `status.json` | bubblewrap's own status documents, one per line, as they arrived |
| `tmp/`, `tmp/home/` | the session temp, with the empty `HOME` the sandbox mounted |

The GUID is the `session` field of `--format json` and the last line of `-v`:

```console
$ sindri -b "why does this test fail?" < failing.log -v
sindri: bash (turn 1): cargo test 2>&1 | tail -20
sindri: session 4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77
$ sindri sessions show 4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77
```

Because it is written incrementally, a cancelled or `kill -9`ed run still reads:
the transcript holds everything up to the crash, metadata says `running` (or
`cancelled`), and the sandbox's `child-pid` document says whether the sandbox
started at all. A session `/new` supersedes is left the same way: it did not
stop, it was left, so its metadata keeps saying `running` and no end is written.
Credentials are redacted before anything is written.

**Known gap:** nothing is pruned, ever — every run keeps its transcript, its
spill files and its session temp. That is deliberate for v0.1 (evidence is
cheaper than re-running), and a `sessions prune` command is a later issue; until
then the directory only grows.

## Configuration

One file, `$XDG_CONFIG_HOME/sindri/config.toml`, holding provider, model, base
URL, reasoning, extra writable paths, turn cap and command timeout. A
documented example ships as [`docs/config.example.toml`](docs/config.example.toml).

Precedence, lowest to highest: the file, then `SINDRI_*` environment variables,
then flags. A pipeline can therefore change provider, model, credential or
limits without rewriting a file:

```console
$ SINDRI_MODEL=claude-sonnet-4-6 sindri -b "…"
$ sindri -b "…" --model claude-haiku-4-5 --timeout 30 --writable /tmp/build
```

| Setting | File key | Environment | Flag |
| --- | --- | --- | --- |
| Provider | `provider` | `SINDRI_PROVIDER` | `--provider` |
| Model | `model` | `SINDRI_MODEL` | `--model` |
| Base URL | `base_url` | `SINDRI_BASE_URL` | `--base-url` |
| Reasoning | `thinking` | `SINDRI_THINKING` | `--thinking`, `--no-thinking` |
| Extra writable paths | `extra_writable_paths` | `SINDRI_EXTRA_WRITABLE_PATHS` (`:`-separated) | `--writable` (repeatable) |
| Extra request headers | `extra_headers` (table) | `SINDRI_EXTRA_HEADERS` (`Name: value` lines) | `--header` (repeatable) |
| Turn cap | `max_turns` | `SINDRI_MAX_TURNS` | `--max-turns` |
| Command timeout | `timeout` | `SINDRI_TIMEOUT` | `--timeout` |

An unknown key is an error naming the key and the file. A value that is
present but wrong is a usage error (exit 2), and a value the run cannot do
without — the model, the credential — is reported with a hint naming every way
to set it (exit 1).

Credentials come from the provider's environment variable (`ANTHROPIC_API_KEY`,
`OPENAI_API_KEY`) or the `api_key` field, and are redacted from all output,
errors and records. A file carrying one is written readable by its owner alone
(`0600`), and replaced atomically.

The `sindri` binary never writes that file except when you ask it to — from
`/settings`. Everything else in a TUI launch writes only the session record.

### No repository config

`.sindri.toml` in the working directory is **never** read. A checked-in config
that grants extra writable paths would turn every `git clone && sindri` into a
privilege escalation, so the file is refused and the refusal is said once on
stderr.

## Evals

Two promises cannot be checked by reading a run's output: that the sandbox
blocks what it claims, and that the turn cap stops a loopy run. Eight scripted
batch runs prove them, and `make eval` is the table:

```console
$ make eval
1  summarise a diff                                        PASS
2  explain a file                                          PASS
3  find and explain a symbol                               PASS
4  rename a symbol across files                            PASS
5  run the tests and fix the failure                       PASS
6  add a flag and a test                                   PASS
7  sandbox refuses a write outside the writable set        PASS
8  turn cap stops a loopy prompt                           PASS
eval: 8 8 of 8 passed (recorded fixtures)
```

Each eval runs the real `sindri` binary in its own copy of a small fixture
repository (`crates/evals/fixtures/repo`) and asserts on exit codes, JSON
fields, what the model was shown and the files left on disk — never on the
model's prose, which drifts. `make eval` exits non-zero when any eval fails.
It is part of `make check`, so CI runs it.

**Offline.** CI and `make eval` replay recorded provider responses through the
mock provider; no credential, no network, no bill. Eval 7 deliberately writes
to paths the read-only root protects, so the suite **refuses to run at all**
when the sandbox is not enforcing: `doctor` must report bubblewrap and a canary
write outside the writable set must be refused by the kernel.

**Live.** `make eval-live` runs the same eight prompts against your own
provider, using your own credential (`ANTHROPIC_API_KEY`, or whatever
`sindri`'s config names) and at your own cost:

```console
$ ANTHROPIC_API_KEY=… make eval-live
```

A live run asserts the same structural properties and skips the two that need
a recorded conversation: what the model was shown on an earlier turn, and how
many requests it made. It reads your own configuration and credential, and
keeps its session records in a temporary state directory so evaluating never
litters your history. A model still has to be configured — `SINDRI_MODEL` or
your config file. Nothing is recorded automatically; see
[`crates/evals/fixtures/README.md`](crates/evals/fixtures/README.md).

## Working on Sindri

Toolchain and gate through devbox:

```console
$ make check      # fmt, clippy, the 400-line audit, tests, evals, no-relink
                  # proof, and the release scripts
$ make eval       # just the evals, offline against recorded fixtures
$ make run ARGS='-b "hello"'
$ make scripts-check   # the installer and release scripts, offline
$ make release TAG=v0.1.0   # the two static binaries, SHA256SUMS, changelog
```

A tag is the only thing a release needs: `.github/workflows/release.yml` builds
both static binaries in an image generated from `devbox.json`, verifies them and
publishes them with their checksums and a changelog derived from the closed
tickets. See [`docs/install.md`](docs/install.md).
