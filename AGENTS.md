# Sindri

Sindri is an agent for the terminal, and the tree is at v0.1.0. No tag is cut yet, so
nothing is published and `docs/install.md` is the path that installs: a checkout and a
toolchain, then `sindri doctor`.

## Where to look

- **Issues** live in this repository's GitHub Issues (`glemsom/sindri`), driven by the
  `gh` CLI: `docs/agents/issue-tracker.md`.
- **Labels**: the five roles triage speaks in, one label string each:
  `docs/agents/triage-labels.md`.
- **Vocabulary**: `GLOSSARY.md` at the root. Use its words exactly; never its avoided
  synonyms.
- **Decisions**: a new decision belongs in the tree beside the code, not in a new record
  (`docs/agents/domain.md`). The tool set's record is `docs/adr/0002-three-tools.md`.

## Proving a change

`make check` is the gate: fmt, clippy with warnings denied, the four audits, the tests, the
evals, the no-relink proof and the release scripts. `make audit` runs the four fast audits
alone, and `make hooks` puts them in front of every commit.

A commit subject is a lowercase area prefix, a sentence saying what changed, and the ticket
that asked for it — `(#123)`. `scripts/release-changelog.sh` derives the changelog from
those subjects, so a release note is made of them.
