# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read this

- **`GLOSSARY.md`** at the repo root: the vocabulary. Use its words exactly; do not drift to synonyms it explicitly avoids.

If the file is missing, **proceed silently**. Don't flag the absence; don't suggest creating a replacement upfront.

## Decisions live in the tree

A new decision goes where the code is — a module doc comment, a test name, a
`GLOSSARY.md` entry — not in a new record. `docs/adr/0001-llm-friendly-code.md` says how
the code is shaped so an agent can find it.
