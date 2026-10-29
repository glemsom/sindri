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
usage error, `3` sandbox or setup failure, `100` turn cap — so a script can
branch without parsing prose.

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

## Session records

Every run leaves a record under `$XDG_STATE_HOME/sindri/sessions/<guid>/`
(ADR-0006), written as the run goes rather than at the end:

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
started at all. Credentials are redacted before anything is written.

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
errors and records.

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
$ make check      # fmt, clippy, the 400-line audit, tests, evals, no-relink proof
$ make eval       # just the evals, offline against recorded fixtures
$ make run ARGS='-b "hello"'
$ make scripts-check   # the installer and release scripts, offline
$ make release TAG=v0.1.0   # the two static binaries, SHA256SUMS, changelog
```

A tag is the only thing a release needs: `.github/workflows/release.yml` builds
both static binaries in an image generated from `devbox.json`, verifies them and
publishes them with their checksums and a changelog derived from the closed
tickets. See [`docs/install.md`](docs/install.md).
