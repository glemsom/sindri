# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`GLOSSARY.md`** at the repo root: the vocabulary. Use its words exactly; do not drift to synonyms it explicitly avoids.
- **`docs/adr/0001-code-an-agent-can-maintain.md`**: the one architecture decision record, and it is a rulebook rather than a rationale. It holds the file-size limit, the module-comment rule, the test posture and the gate (`make check`).

If either file is missing, **proceed silently**. Don't flag the absence; don't suggest creating a replacement upfront.

## No new ADRs

This repo keeps exactly one. A decision that changes behaviour is written where the code is: the module doc comment, a test name, or a `GLOSSARY.md` entry. If a term or rule genuinely has nowhere to live, say so in the change rather than opening a record.

If your output contradicts a rule in `0001`, surface it explicitly rather than silently overriding:

> _Contradicts ADR-0001 (code an agent can maintain), but worth reopening because…_
