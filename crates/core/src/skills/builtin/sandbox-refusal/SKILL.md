---
name: sandbox-refusal
description: "A write is denied or a fetch has no credential: retrying, `sudo` and giving up are the wrong defaults — the refusal names the one thing to change, and so does the body."
---

# A command was refused

A refusal is information, not a wall: the environment has said one true thing
about itself, and the failure is that this run asked for something it does not
have. The three things a model does by default are all wrong here. Retrying a
refusal buys the same refusal one turn later. Escalating it — `sudo`, `chmod`,
`chown`, `--privileged` — asks a machine that already answered. And reporting
the task blocked is the last resort wearing the first resort's clothes: the task
is usually solvable one path or one request over.

## Read the refusal, and spend no call guessing

The message is the diagnosis and the exit code is not. `Read-only file system`
is this environment refusing a write. `Permission denied` on a file you may
write is that file's own mode, which is a different problem. `No such file or
directory` is a third, and none of the three is a wall. `Could not resolve host`
and `Connection refused` are a fourth class and `401` a fifth, and the last two
want opposite moves. `references/refusals.md` holds the table; read it before
the second attempt, not after the third.

## A denied write: move the artefact, not the boundary

1. **Ask where you may write instead of naming one from memory.** The `bash`
   description states this run's writable roots in their own words; `echo
   "$TMPDIR"` prints the scratch one from inside a command. Both are cheap and
   neither is a guess.
2. **Choose by what the file is for.** A change to the project, or any artefact
   the project's own suite will read, belongs in the workspace. Scratch that no
   later turn will miss belongs in `$TMPDIR`. Something a second call must still
   find belongs there too, under a name you choose rather than one you inherit.
3. **Hand the path in where the tool already takes one.** Most things that write
   outside a directory have a flag or a variable because packagers asked them
   to: `--prefix`, `--target-dir`, `DESTDIR`, `-o`, `XDG_*`, `PREFIX`,
   `CARGO_TARGET_DIR`. An option is better than an edit, because it is the
   tool's own answer and it stays right after the run.
4. **When nothing takes a path, make the path a parameter.** Change the source
   so the destination is read from the environment or an argument and defaults
   inside a writable root. That is the change worth keeping: the workaround gets
   *this* run through, and the parameter gets the suite through the next one.
   `references/worked-example.md` is this step end to end, on a repository whose
   tests fail because a state directory is hard-coded outside the sandbox.

Never make a refused command succeed by weakening what it was for. `2>/dev/null`
on the writer, `|| true` after it, a `mkdir` that was expected to fail, an
assertion deleted or a test skipped — each of them turns a boundary into a
silence, and the silenced check is the one that would have noticed the real
breakage later.

## A failed fetch: the other side is not yours to fix

No credential is created by asking again, and no retry finds a proxy that was
not there. So prove reachability **once**, with the smallest request that
answers — `getent hosts`, one `curl -sS -o /dev/null -w '%{http_code}'` — and
then stop asking, because the answer will not change within the run.

What replaces the fetch is one of two things. Either it is already on disk: read
a lockfile, a `vendor/` directory, a cached archive, or `rg` for the name the
dependency would have given you. Or it is a command for the human, whose machine
holds the credential this one was never handed: write that command into the
answer, say what it unblocks, and finish the part of the work that does not
depend on it.

A `401` or a `403` is the one case where more attempts cannot help: the request
was understood and refused, and nothing in a loop changes what the server was
told.

## Say what you moved

The retargeting is the part of the run nobody can read back from the diff. The
answer names the path that was denied, the path used instead, and anything left
undone — a human copying your command hits the same wall, and that sentence is
what gets them past it without paying for the same three turns.
