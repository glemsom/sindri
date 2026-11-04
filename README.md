# Sindri

An AI assistant for GNU/Linux: a Unix filter for scripts, a TUI for humans.

```console
$ git diff | sindri -b "summarise this"
```

stdout is the answer and exactly one newline; it stays empty when a run fails or is
cancelled. stderr is silent unless `-v`.

**Three tools.** `bash` runs everything behind the sandbox — search, fetch, patch and
HTTP all go through the shell, so a pipeline or a short script does in one turn what a
tool-per-task agent needs several for. `skill` loads instructions the workspace ships,
once per conversation, in a wrapper that lists the skill's own files; it is absent when
there is nothing to load, so a run with no skills pays nothing for them. `open` hands the
host one target — an `http://` or `https://` URL, or a file in the workspace — for the
browser of the machine running sindri to show you; its result says `started`, that the
host was asked, or `failed:` with the opener's own reason, and never that you looked.
No sandbox has a display to open a page on, so the browser is the host's; batch mode,
which has no one to look, is offered no `open` at all. Why a skill is not a file read
through `bash`, and the browser not a shell command either:
[`docs/adr/0002-three-tools.md`](docs/adr/0002-three-tools.md).

**Sandboxed by default.** Every `bash` call runs in unprivileged bubblewrap, host root
read-only (a boundary on writes, not reads) plus a writable set you chose. The escape
hatch is `--yolo-unsafe`: one invocation's commands run with your own privileges, outside
bubblewrap, inheriting your environment and working directory — and the record, the JSON
and the rail all say `unsafe`.

**Linux only.** `x86_64` and `aarch64`, static musl for a release; an unprivileged `bwrap`
that can build a namespace is the precondition for running at all.

## Install

**Nothing is released yet** — `git tag` is empty — so `install.sh` has nothing to fetch
and a checkout is the path that installs.

```console
$ git clone https://github.com/glemsom/sindri && cd sindri
$ devbox run -- cargo install --path crates/cli
$ sindri doctor
```

`cargo install --path crates/cli` without devbox builds the same binary and lands it in
`~/.cargo/bin`. `doctor` proves `bwrap`, the binary set and the mount plan, then lists
every skill it found with the scope holding it: `builtin` for the ones compiled into the
binary, `user` for `~/.agents/skills`, and `repo` for each `.agents/skills` from the
workspace up to its repository root — a built-in prints `(compiled in)` where a path
would go, and a nearer scope that displaced it is named beside it. The checksum-verified
`curl … | sh` install into `~/.local/bin` is the plan for v0.1.0; the runbook and what is
proven of it: [`docs/install.md`](docs/install.md).

## Batch mode

```console
$ sindri -b "why does this test fail?" < failing.log
$ cat notes.md | sindri -b
```

Piped stdin is context appended after the prompt; with no prompt argument stdin is the
prompt, and a pipe without `-b` is refused. `-v` puts the run on stderr.

Where stderr is a terminal, a batch run also says what it is doing while it works:
`OSC 7501` reports carrying `working` and then `done` — or `error`, or `idle` for an
interrupt — each with one coarse line of its own words, the turn count and the workspace.
They never go to stdout, so a script reads the answer it always did; a stderr that is a
file or a pipe is written nothing. `status = false`, or `SINDRI_STATUS=off`, silences them.

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
last field, after `command_ms` and `output_bytes`, which say what the commands spent. A
field the run knows nothing about is left out, never `null`.

## Commands

| Command | What it does |
| --- | --- |
| `sindri` | the terminal UI, on a terminal |
| `sindri -b PROMPT` | batch: ask the model, print the answer |
| `sindri auth login copilot` | sign in to GitHub Copilot using a device code |
| `sindri doctor` | prove the sandbox, print the description the model sees, list the skills it found |
| `sindri sandbox exec [--timeout N] [--yolo-unsafe] -- CMD` | one command through the sandbox, no model, outcome as JSON |
| `sindri sessions list [--limit N] [--json]` | the records, newest first, one row each |
| `sindri sessions show [ID] [--json]` | one transcript: the newest record, or the one `ID` or a prefix names |
| `sindri --help`, `--version` | the surface; the build's `git describe` and commit date |

`--format` and `-v` are global, so a TUI launch accepts and ignores them.

## The terminal UI

Bare `sindri` on a terminal opens the frame — alternate screen, streaming markdown
transcript, rail, composer; off a terminal it is a refusal at exit 2.

- **First frame:** the wire, the model, the workspace and the sandbox, one dim sentence
  of the contract, and two prompts that can be typed as they read, with `/help` beside
  them. The composer holds the placeholder `Ask, or drop a log here · @ path · / command`
  until there is a draft.
- **Pulse:** while a run is live, a braille marker at the head of the newest answer
  carries the elapsed time (`⠸ 0.8s`) and settles on `✓` or `✗` with the duration it
  stopped at; each call's duration freezes with the call, and the rail's output and
  context figures move on what streams, trued up by the provider's report. An idle frame
  is never drawn.
- **Tool calls:** each appears as it starts with the last five output lines, then
  collapses to `✓ exit 0`, `✗ exit N`, `✗ timed out`, `✗ cancelled` or `✗ sandbox: …` and
  the count behind it (`· 400 lines`). `Enter` on an empty composer, or a click on the
  row, opens the call whole in place — a call that never spilled included; `Ctrl-O`, or a
  click, opens a spill file.
- **Queue:** a prompt typed during a run waits above the composer, whole and in order,
  titled `3 queued` (`queued` for one); `↑` takes the newest back into the composer,
  `Ctrl-X` drops it. `Ctrl-C` is answered at once — `cancelling…` in the footer, draft
  kept — and the run stops when the request in flight does.
- **Rail:** 32 columns, always visible — cache hits, turns against the cap, a context
  meter, session GUID, temp path, branch, sandbox posture, why the last run stopped.
  Read as a ledger: labels in a nine-column field two cells in from the rail's first
  column, dot leaders across the gap, every value hanging off one edge, a `─` hairline
  between blocks. Below 100 columns it becomes one strip, inset the same two cells; the
  face takes the rows left over.
- **Composer,** two to six rows: `Enter` sends, `Shift+Enter` or `Ctrl-J` breaks the line,
  `Ctrl-Q` leaves, `Ctrl-C` cancels a run and keeps the draft — or clears it when nothing
  is running. A pasted prompt arrives as one draft with its line breaks kept.
- **Scroll and copy:** `PgUp`/`PgDn` and the wheel move the transcript, `Home` goes to
  the first turn and `End` back to the tail, which is followed again; a reader scrolled
  back is told what arrived below (`↓ 4 new rows`), and a thread longer than the window
  marks its position in the last column. Only on-screen rows are laid out. A drag marks
  what it will take: the cells lit from press to release are the rows the copy takes,
  styles stripped, and the mark stands after the button comes up, so `Ctrl-Y` copies it
  again — or the last answer, when no mark does. `Shift`+`↑`/`↓` grow a mark a row at a
  time up from the last row on screen, and shrink it back. A picture's block is one thing
  and copies as its words; `Shift+drag` is the terminal's own selection. Words leave by
  OSC 52, falling back to `wl-copy` or `xclip`.
- **`@`** walks the workspace (`.git` skipped, `.gitignore` ignored), proximity-ranked,
  capped at eight rows. `Tab`/`Enter` takes one; `Esc` dismisses.
- **`/`** runs `/new` (fresh session GUID, empty transcript), `/export`, `/skill`,
  `/model`, `/settings`, `/help` and `/quit`. `/skill` is the one name a colon may end —
  `/skill:house-style` and `/skill house-style` are the same line — where the rest of it
  is the prompt the skill is put to work on, and a bare name arms it without starting a
  turn; the names offered are the ones the catalog says you may invoke. `/model` opens a
  screen of the names the wire serves, ranked as you type, with `Enter` writing the one
  you take to the config file the next prompt reads; `/model <name>` sets a name the wire
  never offered. `/export [path]` writes the thread as one markdown file, asking twice
  before replacing a file; reading a record back is `sindri sessions show`. `?` and
  `/help` list the same bindings, each gesture beside its key.
- **Chrome:** a narrow terminal shortens what the frame says and never cuts a key or a
  fact in half. The footer states seven hints from 120 columns up, five from 100 and never
  fewer than `Enter send` and `? keys`; the rail gives facts up in one priority order and
  keeps a row saying how many went (`+7 more`); the slim strip never loses the posture,
  the model or the workspace.
- **One thread:** a second prompt builds on the first, `/new` starts over, and a model
  changed in `/settings` answers the next turn on the same thread. Boot runs `doctor`
  first: a sandbox that cannot be created is a card naming the failing check, and a first
  run with no API key opens the modal below instead of failing.

### Settings

`/settings` edits the one file everything else reads.

- `↑`/`↓` or `Tab` move between keys, `Ctrl-U` empties the field, `Space` flips thinking,
  and an empty value unsets the key so the environment speaks. `Enter` writes at once and
  the next prompt runs on it; `Esc` discards, and only touched fields are written, once,
  atomically.
- **The API key** starts at *from environment*: nothing is written unless you type one,
  where typing warns and names the plaintext file it will land in. It is masked and never
  reaches the transcript or the record. The environment is read **above** the file, so an
  exported `SINDRI_API_KEY` wins.
- **The extra headers** are no secret: the row shows the table the file holds, one
  `Name: value` per line, and a paste of such lines is the table back. Nothing else prints
  them and they are never recorded.
- **A first run needs no file.** Bare `sindri` with no key opens the frame with this modal
  on `api_key` — the row says where a key comes from (`SINDRI_API_KEY` or the provider's
  own variable), where it is written, that the file is `0600`, and that the value is never
  shown again. `Esc` leaves a frame that still reads, saying the missing key once and what
  the composer cannot send. `sindri -b` with no key is still exit 1, one line on stderr.

## Configuration

GitHub Copilot uses Rig’s OAuth device flow and stores credentials with owner-only permissions
under the Sindri state directory. Sign in with `sindri auth login copilot`, then select
`provider = "copilot"`; Copilot does not use a configured API key.

One file, `$XDG_CONFIG_HOME/sindri/config.toml` (example:
[`docs/config.example.toml`](docs/config.example.toml)). Precedence: file, then `SINDRI_*`
environment, then flags.

| Setting | File key | Environment | Flag |
| --- | --- | --- | --- |
| Provider | `provider` | `SINDRI_PROVIDER` | `--provider` |
| Model | `model` | `SINDRI_MODEL` | `--model` |
| Base URL | `base_url` | `SINDRI_BASE_URL` | `--base-url` |
| Credential | `api_key` | `SINDRI_API_KEY` | — |
| Reasoning | `thinking` | `SINDRI_THINKING` | `--thinking`, `--no-thinking` |
| Extra writable paths | `extra_writable_paths` | `SINDRI_EXTRA_WRITABLE_PATHS` (`:`-separated) | `--writable` (repeatable) |
| Extra request headers | [`extra_headers`](#opencode-go) (table) | `SINDRI_EXTRA_HEADERS` (`Name: value` lines) | `--header` (repeatable) |
| Turn cap | `max_turns` | `SINDRI_MAX_TURNS` | `--max-turns` |
| Command timeout | `timeout` | `SINDRI_TIMEOUT` | `--timeout` |
| Terminal reports | `status` | `SINDRI_STATUS` | — |
| ssh agent | `ssh_agent` | `SINDRI_SSH_AGENT` | `--ssh-agent`, `--no-ssh-agent` |

`--yolo-unsafe` is the one flag with no file key and no environment variable: a run that
leaves the sandbox is asked for in the invocation that makes it and dies with it, because
a persisted unsafe setting is the one a user forgets they turned on. Batch mode and
`sandbox exec` say it once on stderr; the rail says `unsafe · your own privileges`. Under
the flag `--writable` is still validated, it simply is not what bounds the run.

`--writable` and `--header` are repeatable but **replace** the configured values. A
header value may hold `{session}`, sent as the id of the session the run writes, so a
gateway that caches per session follows `/new`, and a value with no token is sent as
written — [OpenCode Go](#opencode-go) is the worked one. An unknown key is an error naming
key and file; a wrong value is exit 2. The credential also
comes from `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` or a matching `*_BASE_URL`, and is
redacted everywhere; a file carrying one is `0600`. Copilot’s OAuth token store is
separate from `config.toml` and is not an API key.

`.sindri.toml` in the working directory is **never** read: a checked-in config granting
writable paths would turn every `git clone && sindri` into a privilege escalation.

A repository's own house rules are not a setting, and they arrive all the same: the
`AGENTS.md` at the repository root — never a nested one, never an ancestor's — is read
once at boot and is appended to the preamble rather than sent as a turn, so the transcript
stays your words. The read is capped at **8 KiB**, a byte count rather than a line count
because bytes are what the request pays for, and a longer file arrives cut with a marker
naming the bytes left out. A repository without one adds nothing; one that cannot be read
is a diagnostic naming the file and the reason, and the run still starts. `-v`,
`sindri doctor` and the record's start block name the file and the bytes handed over.
[`GLOSSARY.md`](GLOSSARY.md) defines the word, under `instructions`.

### OpenCode Go

OpenCode Go is a gateway that speaks the `openai` wire and routes on a header of its own: a
request without it is answered `400`, however right the model and the address are. Name the
wire, point `base_url` at the gateway's API root — the address it prints beside the key,
ending in `/v1`, where the run asks `chat/completions` — keep the key it issued in
`SINDRI_API_KEY`, and carry the header:

```toml
provider = "openai"
base_url = "https://<gateway>/v1"
model = "<a model the gateway serves>"

[extra_headers]
x-opencode-session = "{session}"
```

In the frame, `/settings`'s model row lists the names the gateway serves once the wire and
that address are in it, so the model is one of those rather than a guess; its extra headers
row draws the table the file holds and takes those `Name: value` lines back. Without the
file the header is set at either of the two levels that win over it:
`SINDRI_EXTRA_HEADERS` takes the line `x-opencode-session: {session}` for a shell, and
`--header` takes it for one invocation.

The gateway caches per session, so `{session}` in a value is sent as the id of the session
the run writes — one record, one temp, one id naming them — and a prompt after `/new`
arrives under the id of the session it opened rather than the one it replaced.
`sindri sessions show` prints that id back.

## Session records

Every run leaves a record under `$XDG_STATE_HOME/sindri/sessions/<guid>/`, written as the
run goes. `sindri sandbox exec` opens one too; `sindri doctor` writes none.

| file | what it holds |
| --- | --- |
| `session.json` | provider, model, enforcement, the policy the model was handed, the workspace and its branch and commit, the dials the run obeyed, start, end, turns, usage, and why it stopped |
| `transcript.jsonl` | one entry per thing that happened — prompt, turns, tool calls, tool results — in order |
| `status.json` | bubblewrap's own status documents, one per line, as they arrived |
| `tmp/` | the session temp: a run's writable scratch, mapped to `TMPDIR`, removed when the session ends. `spill/` inside it keeps the output a truncated result names by path, and `HOME` is the host's own, read-only |

The GUID is the `session` field of `--format json` and the last line of `-v`;
`sindri sessions list` prints every record newest first, and `sessions show` reads one
back — the newest when no id is named, or the one a whole id or a leading prefix names,
an ambiguous prefix a usage error listing the candidates. `show --json` prints the files
on disk in their own shape, so `jq` is the debugger and `cat transcript.jsonl` cannot
disagree with it.

Written incrementally, so a cancelled or `kill -9`ed run still reads; a session `/new`
supersedes keeps saying `running`, because it did not stop, it was left. Fields are only
ever appended, and every field added after a record was written is defaulted rather than
required, so a record from any earlier sindri still reads. `version` is the build that
wrote the run — per-run provenance, **not** a schema version. Nothing is pruned, ever;
deliberate for v0.1, and `sessions prune` is a later issue.

## Evals

Four promises cannot be checked from a run's output: that the sandbox blocks what it
claims, that the turn cap stops a loopy run, that a skill the workspace ships is loaded
before the answer that needed it, and that a repository's `AGENTS.md` changes what a run
does. Ten scripted batch runs (`make eval`) prove them.

Each runs the real binary in its own copy of a small fixture repository and asserts on
exit codes, JSON fields, what the model was shown, the record it left and the files on
disk — never on the model's prose, which drifts. CI replays recorded responses through the
mock provider: no credential, no network, no bill. Eval 7 writes to paths the read-only
root protects, so the suite **refuses to run** when the sandbox is not enforcing.

Two cases read an answer, because a rule that appears in no other file of the tree can
only have arrived through the channel under test. Eval 9's rule lives in the fixture's one
skill; it fails on an answer without the skill call, on a first turn already carrying the
rule, and on a script written without it. Eval 10's lives in the fixture's `AGENTS.md`; it
fails when that file is gone, when the block the model read arrived truncated at the
instructions cap, and when the record shows the forbidden command was run.

`make eval-live` runs the same ten prompts against your own provider and credential, at
your own cost, skipping the assertions that need a recorded conversation. See
[`crates/evals/fixtures/README.md`](crates/evals/fixtures/README.md).

## Working on Sindri

Everything goes through devbox, and `make check` is the definition of done.

Run `make hooks` once per clone: it points `core.hooksPath` at `.githooks`, and every
commit then runs `make audit` — the fast audits, in milliseconds — so a broken commit is
refused while it is still the only one. `git commit --no-verify` skips it, and `make
check` still is the gate.

| Target | What it runs |
| --- | --- |
| `make check` | fmt, clippy, the four audits (800 lines, module docs, the module map, test names), tests, evals, the no-relink proof, the release scripts |
| `make audit` | the four fast audits on their own |
| `make eval` / `eval-live` | the evals, offline or against your provider |
| `make build` / `install` / `clean` / `run ARGS=…` | release build / install the tree's binary / `cargo clean` / run it |
| `make release TAG=v0.1.0` | both static binaries, `SHA256SUMS`, changelog — inside the release image; devbox has no musl `std`, so this fails on a dev machine |

A tag is the only thing a release needs, and the workflow builds, verifies and publishes
it; none is cut yet, so every release path here is the plan ([`docs/install.md`](docs/install.md)).
[`GLOSSARY.md`](GLOSSARY.md) has the vocabulary and
[`docs/adr/0001-llm-friendly-code.md`](docs/adr/0001-llm-friendly-code.md) how the code is
written.
