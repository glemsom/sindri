---
name: commit-message
description: "Write one commit message for the change in front of you: a conventional-commit subject naming what changed, a body only when the reason is not obvious, and nothing the diff does not show. Use when asked to commit, or to write a commit message."
---

# Commit message

Write from the diff, never from memory of what you meant to do.

## Read first

```sh
git diff --staged --stat
git diff --staged
git log --oneline -10
```

The log is where the repository's own conventions live: match its subject style,
its capitalisation and its scope vocabulary, and fall back to `type(scope): what
changed` only when the log shows nothing.

## The subject

One line, imperative, no trailing full stop, 72 characters at most:

    type(scope): what changed

`type` is one of `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`,
`build`, `ci`. `scope` is the area touched, left out when the change is not local
to one. The words complete "this commit will …" — `fix(skills): refuse a dangling
link`, not `fixed dangling links` and not `skill loading changes`.

## The body

Add one only when the subject cannot carry the reason: a trade-off, a rejection, a
subtlety a reader would otherwise undo. Blank line after the subject, wrapped at 72
columns, saying why rather than what — the diff already says what.

Leave out what the repository does not ask for: no generator trailer, no
co-author line, no issue reference the user did not give you.

## Before you commit

- `git status`, to be sure nothing unintended is staged.
- Run the checks the repository names — its `make check`, its CI script — and
  commit a change only once they pass.
