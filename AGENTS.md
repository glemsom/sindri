## Comments

Code is written for agents to read, so the default is none.
A comment earns its place only by carrying what the code cannot show: an external constraint, a hidden side effect, a non-obvious gotcha.
Tickets, specs, and why-this-over-that live in git history and the PR, not above the code.

## Agent skills

### Issue tracker

Issues live in this repo's GitHub Issues (`gh` CLI). See `docs/agents/issue-tracker.md`.

### Triage labels

Canonical five: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` + `docs/adr/` at the repo root. See `docs/agents/domain.md`.
