---
name: sindri
description: How a run works here — the order to do things in, and why that order.
---

# How a run works here

This is a placeholder body. The skill that ships in the binary with the real
procedure is ticket #298; this one proves the scope end to end.

Read before you write. One `bash` call can look at the whole workspace, and a
run that changed a file before reading it has guessed at what was in it.

1. Look: `rg --files | head -50`, and the `AGENTS.md` where the repository has one.
2. Change one thing at a time, and read the result of the command you just ran.
3. Say what you did, and what you did not.

`references/working.md` holds the long form: what a turn costs, and why a loaded
body is not loaded twice.
