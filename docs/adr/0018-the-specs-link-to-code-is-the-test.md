# The spec's link to code is the test, not a comment

A comment carries only what the code cannot show. That one sentence deletes most of what this tree's
comments say: of its nine hundred-odd `//` lines, roughly a sixth name the section, ADR or ticket
that asked for the code underneath. A citation is the one kind of comment a sweep cannot delete on
its own, because it was the road from a function to the claim it answers — and a product with no
road back from code to what must be true is a product nobody can audit.

So each layer keeps its own work, and speaks where it is read. A **spec** says what must be true:
the acceptance criteria of the issue that asked for the work, because that is where this repo's
specs live. An **ADR** says why it is built this way. `GLOSSARY.md` says what the words mean. A
ticket says when. A test states the behaviour in the reader's own words, in its name and in its
assertions — that is the spec's link to code, and the only link a compiler and a suite keep honest.
Git carries the rest: the commit message and the pull request hold the when and the
why-this-over-that, and name the ADR an edit overturns.

What replaces a spec section, an ADR number or a ticket number in a comment is nothing. The sentence
goes with the reference it carried, because the capability it pointed at is named by the test that
proves it, and a test cannot cite a section that has moved or a decision that has been overturned. A
comment that only makes sense after opening another file is not explaining the code; it is
explaining a decision, and decisions live in the ADR and in git. The comments still on that road are
what the batches following this ADR are for: this is the bar they are judged against, not a
description of a tree that already meets it.

A cleanup edit therefore has three **dispositions** for a comment, and no fourth:

- **Delete** — the code already says it. A comment that restates its own signature, restates its
  test's name, or whose only content is a citation, goes with nothing put in its place.
- **Keep** — it carries what the source cannot show: an external constraint (a sandbox that forbids
  a syscall, a wire that answers unlike its documentation, a pty that must be told its size), a
  hidden side effect (raw mode left on, a screen not handed back, a file another process rewrites),
  or a gotcha a reader could not recover by reading the function.
- **Promote to rustdoc** — it is the caller's contract rather than the implementation's hazard: what
  the function promises, what it panics on, what the caller must hold true. That is a `///`, and the
  test for it is a caller's need, not an editor's comfort.

"Fewest lines that still land" is a bar no machine can read, so it is written as a number. A kept
comment block — the run of consecutive `//` lines above one thing, blank `//` lines in it included —
is at most three lines. Past three it has become the paragraph that argues a design, and the
argument is the pull request's. The gate this cleanup ends with must hold the tree to both halves,
that length and no `§N`, `ADR-NNNN` or issue number in a `//` comment; until that gate exists
nothing does, and the comments still carrying a citation are the cleanup's own work rather than a
claim about it.

`///` and `//!` are out of scope for this pass. Rustdoc is delivered: `cargo doc` renders it and a
user reads it, so a doc comment earns its place by what a caller needs, which is a different and
larger question than whether an internal note is worth its lines — a `///` by what the caller must
know before calling, a `//!` by what a reader of the crate must know before calling anything in the
module. A doc comment that restates its signature is a job of its own; sweeping it in here would put
a public API's documentation in the same commit as a hundred deletions of internal prose.

An ADR is a numbered file, `NNNN-a-slug.md`, numbered in the order decisions are taken, and a
retired number is never reused — which is why this one is 0018 while no other file sits beside it.
A document that must name a retired ADR writes `(retired)` after the number, as with
`ADR-0002 (retired)` below, because a reader who follows a number has to know the file is not there
before they look. The reference guard in `crates/sindri-core/tests/doc_references.rs` holds this
file, `AGENTS.md`, `GLOSSARY.md` and every ADR in the directory to both habits, and rejects a live
ADR marked retired as readily as a dead one left unmarked. Nothing guards the `#` comments in the
`Makefile` and the scripts, which is one more reason a citation is not worth carrying.

This ADR supersedes `ADR-0002 (retired)`, which made the spec the acceptance criteria and pointed at
a `docs/SPEC.md` that the documentation cleanup has since retired. It keeps that layering and adds
the one thing the earlier ADR left open: where the spec's link to code lives. The answer is a test,
and a comment that cites anything at all is a second, weaker telling of what the test already says.
