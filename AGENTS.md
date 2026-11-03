# Sindri

Sindri is an agent for the terminal, and the tree is at v0.1.0. No tag is cut yet, so
nothing is published and `docs/install.md` is the path that installs: a checkout and a
toolchain, then `sindri doctor`.

## Where to look

- **Issues** live in this repository's GitHub Issues (`glemsom/sindri`), driven by the
  `gh` CLI: `docs/agents/issue-tracker.md`.
- **Labels**: the five roles triage speaks in, and the one label string each has, are
  `docs/agents/triage-labels.md`.
- **Vocabulary**: `GLOSSARY.md` at the root. Use its words exactly; do not drift to the
  synonyms it avoids.
- **Decisions**: `docs/agents/domain.md` says which records hold them, and that a new
  decision belongs in the tree rather than in a new record. The tool set's record is
  `docs/adr/0002-three-tools.md`.

## Proving a change

`make check` is the gate: fmt, clippy with warnings denied, the four audits, the tests, the
evals, the no-relink proof and the release scripts. `make audit` runs the four fast audits on
their own, and `make hooks` puts them in front of every commit.

A commit subject is a lowercase area prefix, a sentence saying what changed, and the ticket
that asked for it — `(#123)`. `scripts/release-changelog.sh` derives the changelog from those
subjects, so the convention is what a release note is made of.