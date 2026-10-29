# 0005 — Config surface and its precedence

- **Status:** accepted
- **Date:** 2025-10-29

## Decision

Precedence, lowest to highest: `$XDG_CONFIG_HOME/sindri/config.toml` →
`SINDRI_*` environment variables → CLI flags.

Config holds: provider, model, base URL, whether thinking/reasoning is enabled,
extra writable paths, extra request headers, turn cap, tool timeout.

Extra request headers exist because a gateway may route on a header of its own
(a session id, a referer) and cannot be reached without one. Their values are
never printed and never recorded: a gateway's header is as likely to be a
secret as the credential is.

Credentials come from the provider's environment variable
(`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`) or an `api_key` field in the config.
A wire that needs none is not asked for one: the OpenAI-compatible wire serves
local servers, which have no credential to give, so an absent one leaves the
request unauthenticated rather than blocking the run. A hosted server that wants
one answers 401, and that refusal is reported as itself.

**No repository-local config file.** A checked-in config that grants extra
writable paths turns every `git clone && sindri` into a privilege escalation.
The name such a file would have, `.sindri.toml` in the working directory, is
looked for only so the refusal can be said once on stderr; the file is never
opened.

## Consequences

- The future `/settings` TUI modal edits this same file — one writer, one schema.
- The sandbox `HOME` starts empty, so the user's real dotfiles and git config
  do not reach the sandbox *as `HOME`*, and are never copied in. They are not
  hidden, though: the read-only host root leaves them readable at their host
  paths, credentials included — writes are blocked, reads are not (ADR-0010).
  Opting in to a populated `HOME` is a later, explicit feature.
