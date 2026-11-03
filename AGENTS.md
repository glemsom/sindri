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
  decision belongs in the tree rather than in a new record.

## Definition of done

`make check` is the gate and it is the whole of done: fmt, clippy with warnings denied,
four audits, the tests, the evals, proof that an unchanged tree does not relink, and the
release scripts run as processes. Nothing lands on a subset. `make hooks` points
`core.hooksPath` at `.githooks`, whose commit hook runs `make audit` — the gate's own
fast audits, so a broken commit is refused at the commit rather than minutes into the
gate.

## The commit

One lowercase area prefix, then a sentence saying what changed, then the ticket:
`docs: the glossary keeps the words the code says (#338)`. A release's changelog is
`scripts/release-changelog.sh`, which reads the ticket out of a merge subject; a slice
that landed any other way is written into the notes by hand, as `docs/install.md` says.

## What breaks without a word

- **No sandbox, no agent.** A `bwrap` that cannot start is a failure to report, never to
  paper over: `docs/adr/0002-two-tools.md`.
- **`open` exists because nothing inside the sandbox has a display.** It hands the
  host's browser one target and says `started`, never that a human looked:
  `docs/adr/0002-two-tools.md`.
