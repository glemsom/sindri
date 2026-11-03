# Domain Docs

How the engineering skills consume this repo's domain documentation.

## Before exploring, read this

`GLOSSARY.md` at the repo root: the vocabulary. Use its words exactly; never its avoided
synonyms. If the file is missing, proceed silently — don't flag it, don't propose a
replacement.

## Decisions live in the tree

A new decision goes where the code is — a module doc comment, a test name, a `GLOSSARY.md`
entry — not in a new record. The records are the exception because they argue for the tree
and not for one module: `0001` how the code is shaped so an agent can find it, `0002` why
there are three tools, `0003` why one skill ships inside the binary, `0004` what a run
tells the terminal about what it is doing.
