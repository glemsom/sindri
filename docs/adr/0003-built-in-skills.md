---
status: accepted
date: 2026-11-02
---

# 0003 — A built-in skill: the instructions ship inside the binary

ADR 0002 §3 names two scopes, both on disk: the home directory and the repository. A skill
compiled into the binary makes that false, so the third scope is recorded here.

## 1. The instructions ship in the artefact that enforces them

A built-in is `include_str!` of a `SKILL.md` beside the module that reads it, with no path,
because a path is a thing that can be absent, stale or unwritable. A file written into
`~/.agents/skills` at install time drifts from the binary: an upgrade replaces one and
leaves last release's instructions behind, so the model is told one boundary and the
sandbox applies another, and nothing reports the disagreement.

One directory per built-in under `builtin/`, named for the skill inside it, so the body, the
files beside it and the tests written about it are all found by the name the catalog routes
a task to, and a second skill is a second directory rather than a second file beside the
first. What one of those directories may spend is a ceiling, and the ceilings are the
module's own (`crates/core/src/skills/builtin.rs`): they bound one load, not the tree.

## 2. A body holds the procedure, never the boundary

The `bash` description already holds the invariant, derived from the policy, so the words
the model reads and the mounts it runs under cannot disagree. A built-in body holds the
procedure — what a run does in this workspace, in what order, with which tool, and why —
and may not carry a fact the enforcement can generate.

## 3. The untested risk, named

A built-in is needed *before* the first `bash` call, unlike a task skill, which a prompt
triggers once the task is recognised as covered. Nothing here measures whether a model
calls `skill` before its first `bash`, or whether one catalog line per skill is enough to
make it do so. `scripts/bench-skill-ablation` settles that by measurement.

The words that ask for the load are as much a part of it as the body, so they are a text of
their own: `doctrine`, compiled in from `crates/core/src/agent/skill/doctrine.md` beside the
module that assembles the tool's description. That is the seam the bench's third arm uses —
the shipped arm, the same arm one sentence shorter, and the arm with no built-in at all —
and which wording earns its place is decided by whether a run loaded a skill before its
first command, which the run's own record says.

Making the trigger a sentence of its own meant repunctuating and rewrapping the prose and
changing no word of it, so an arm is the absence of one line; the words the model reads, and
the shape they are read in, are `scripts/bench-skill-ablation/README.md`'s to state.

A body can be worth two different things, and the bench scores both. A body that buys an
*outcome* is scored on whether the work held afterwards — the axis that measured recovering
from a refused write, and that went with the skill it was measuring. A body that buys a
*cheaper read* cannot buy an outcome a model would reach anyway: `search-and-read-files` is
scored on the input tokens the same answer cost, on an inspect-shaped task whose fact is
cheap to find and expensive to read whole.

The task, both axes and the trigger sentence's rate run behind one command. What
the bench decides, when it is run, is a recorded verdict: a body that comes out unused,
useless or dearer than the reads it replaces is dropped by the sentence the report writes
into the committed results file, rather than kept on the argument that the doctrine reads
well. Until those sweeps exist in `scripts/bench-skill-ablation/results/`, none of this is
measured — the harness, its offline pass and the commands that would fill them are what the
tree holds today.

## The alternative that lost

Ship the built-ins as files in this repository's `.agents/skills`. A run outside the
repository would be told nothing, and a run inside it would read the same body as house
rules: a user overriding it would be overriding a repository's file, not Sindri's.
