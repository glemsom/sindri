## Comments

Code is written for agents to read, so the default is none.
A comment earns its place only by carrying what the code cannot show: an external constraint, a hidden side effect, a non-obvious gotcha.
This holds for every comment — `//`, `///` and `//!` — and a `///` on a private item is a `//`.
A block is at most three lines, and none cites a spec section, an ADR or an issue.
Tickets, specs, and why-this-over-that live in git history and the PR, not above the code.
A test names the behaviour; it is the only link from code to what must be true.
Nothing else in this repo narrows this section.

## Agent skills

### Issue tracker

Issues live in this repo's GitHub Issues (`gh` CLI). See `docs/agents/issue-tracker.md`.

### Triage labels

Canonical five: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` + `docs/adr/` at the repo root. See `docs/agents/domain.md`.
