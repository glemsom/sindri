---
status: accepted
date: 2026-10-29
---

# 0005 — Config surface and its precedence

## Decision

Precedence, lowest to highest: `$XDG_CONFIG_HOME/sindri/config.toml` → `SINDRI_*` → CLI flags.

Keys, all optional, one `#[serde(deny_unknown_fields)]` struct (`crates/core/src/config/file.rs`); an unknown key is an error naming key and file:

```text
provider, model, api_key, base_url, max_turns, timeout, thinking,
extra_writable_paths, extra_headers
```

Environment: `SINDRI_PROVIDER`, `SINDRI_MODEL`, `SINDRI_API_KEY`, `SINDRI_BASE_URL`, `SINDRI_MAX_TURNS`, `SINDRI_TIMEOUT`, `SINDRI_THINKING`, `SINDRI_EXTRA_WRITABLE_PATHS` (`:`-separated), `SINDRI_EXTRA_HEADERS` (`Name: value` lines). Defaults: `max_turns` 250, `timeout` 120 s.

- Credential: `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `SINDRI_API_KEY`, or `api_key`. The OpenAI-compatible wire runs unauthenticated when there is none (local servers); a hosted 401 is reported as itself.
- Header values are never printed and never recorded.
- **No repository-local config.** `.sindri.toml` is probed only so the refusal can be said once on stderr; it is never opened.

## Consequences

- `/settings` edits this same file through one schema and one writer (ADR-0014).
- A run with no credential opens the frame's settings modal on `api_key` rather than exiting, so the first run needs no hand-edited file (ADR-0004); the write it saves is the same explicit one `/settings` always makes.
- The sandbox `HOME` starts empty, so real dotfiles never arrive as `HOME` and are never copied in — they remain readable at their host paths (ADR-0010).
