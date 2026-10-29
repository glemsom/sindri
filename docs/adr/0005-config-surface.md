# 0005 — Config surface and its precedence

- **Status:** accepted
- **Date:** 2025-10-29

## Decision

Precedence, lowest to highest: `$XDG_CONFIG_HOME/sindri/config.toml` →
`SINDRI_*` environment variables → CLI flags.

Config holds: provider, model, base URL, whether thinking/reasoning is enabled,
extra writable paths, turn cap, tool timeout.

Credentials come from the provider's environment variable
(`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`) or an `api_key` field in the config.

**No repository-local config file.** A checked-in config that grants extra
writable paths turns every `git clone && sindri` into a privilege escalation.
The name such a file would have, `.sindri.toml` in the working directory, is
looked for only so the refusal can be said once on stderr; the file is never
opened.

## Consequences

- The future `/settings` TUI modal edits this same file — one writer, one schema.
- The sandbox `HOME` starts empty: the user's real dotfiles, git config and
  credentials are never mounted or copied in. Opting in is a later, explicit
  feature.
