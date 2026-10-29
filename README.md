# Sindri

An AI assistant for GNU/Linux, in two shapes: a Unix filter for scripts and a
terminal application for humans.

```console
$ git diff | sindri -b "summarise this"
```

stdout is the answer and exactly one newline; stderr is silent unless `-v`.
Every `bash` call the model makes runs inside bubblewrap with a read-only host
root and a writable set you chose. See `capabilities-bullets.md` for the whole
idea, `GLOSSARY.md` for the vocabulary, and `docs/adr/` for the decisions.

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

## Working on Sindri

Toolchain and gate through devbox:

```console
$ make check      # fmt, clippy, the 400-line audit, tests, no-relink proof
$ make run ARGS='-b "hello"'
```
