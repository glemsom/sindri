---
status: accepted
date: 2026-11-02
---

# 0004 — Built-in skills: a third scope, and the lowest

ADR 0002 opens by saying a skill is found in someone's home directory or in the
repository, and its §2 says those two paths are ours. A skill shipped inside the binary makes
both statements false, so the third scope is written down here before the code that reads it,
for the reason 0002 and 0003 were written: the next agent finds a decision rather than a hole.

## 1. A `builtin` scope, lowest of three

A built-in skill is bytes compiled into the binary — `include_str!` of a `SKILL.md` that lives
in the tree beside the module that reads it — and not a directory any scan walks. It has no
path, because a path is a thing that can be absent, stale or unwritable, and a skill the binary
ships cannot be any of those.

That is the whole of the argument for embedding rather than installing. A file written into
`~/.agents/skills` at install time drifts from the binary that enforces what it describes: an
upgrade replaces the binary and leaves last release's instructions behind, so the model is
told one boundary and the sandbox applies another, and nothing reports the disagreement.
Embedded, the instructions and the enforcement ship in the same artefact or neither does.
`install.sh` also stays what it is — one binary, checksummed, put in one directory — because a
skill tree is a second thing to download, verify and place, and a thing it does not write
cannot be left half-written.

The scope is the lowest of three, so precedence runs builtin, then user, then repo, nearest
wins: a skill the repository carries takes a name from the home directory's, and the home
directory's takes it from a built-in's. A user who disagrees with a built-in is not left with
an argument — they drop their own skill of the same name in `~/.agents/skills` and it wins,
and the skill that lost is recorded rather than dropped, as it already is between the two
nearer scopes. A built-in is where that order comes from: the sequence in which a run is told
the things it must know before it touches the workspace. A repository that would rather be
told them differently says so in its own skill, and the nearer scope wins that argument as it
wins the rest.

## 2. Provenance is a field, not an inference

A reader has to tell "how Sindri works" from "how this repository works", and the difference
is not a matter of reading the words: the same procedure can ship in either. So where a skill
came from is a value the scan sets, never something a reader derives from a path or guesses
from a body. The code already has that field — `Skill.scope`, whose word `sindri doctor`
prints in a column beside each name — so the third scope is a third value of it, and a
built-in reads `builtin` in that column.

The same holds for what a name displaced. `Shadow` holds the winner and the loser rather than
a boolean, which is what lets `sindri doctor` print which built-in a nearer skill displaced
instead of merely that something was displaced. What it holds of each is a directory today,
and a built-in is the one skill that has none: so what it carries becomes a scope and a name,
and a displaced built-in is printed by its name and its origin, never by a path it does not
have. A doctor that said `shadows /dev/null` for one would be inventing a file to stand for a
fact.

## 3. The duplication rule: one fact, one place

A built-in could restate the sandbox boundary in its body, and that is the failure this rule
exists to prevent. The `bash` description holds the invariant — the boundary a caller is told
about because it is enforced — and it is derived from the sandbox policy itself, so the words
the model reads and the mounts it runs under are one derivation and cannot disagree. A built-in
body holds the procedure: what a run does in this workspace, in what order, with which tool,
and why.

The split follows from what each can know. A description derived from the policy is wrong only
if the policy is, and the policy is what `--yolo-unsafe` and the mounts are read into; a body
is prose about a way of working, and the binary it shipped in says nothing about whether the
procedure is still the one to follow. Anything that fits in one sentence belongs to neither
body but to the policy's own description, which is where the sentence is generated: two places
stating one fact are two places to disagree, and the place that can be generated from the
enforcement is the one that stays true.

## 4. The untested risk, named

A built-in is needed *before* the first `bash` call. That is a different timing from a task
skill, which a prompt triggers when a task is recognised as one it covers: a built-in covers
every task, so the model has to reach for it before it knows what the task is made of, and a
model that has loaded nothing yet is exactly the model deciding whether to load anything.

Nothing in this tree measures that. Whether a model calls `skill` before its first `bash`, and
whether the catalog's one-line-per-skill description is enough to make it, is an open question
this record names rather than answers. It is settled by measurement — the same kind of
measurement the preamble's tax was settled with — and not by argument, and a built-in whose
body is only ever read after the command it governs has failed is a built-in that paid for
itself and bought nothing.

## 5. What did not change

The scope is a place a skill is found, and nothing about loading one moves. A built-in is still
loaded once per conversation and no more: the loaded set claims the name on the first call and
a second call answers `already_loaded` with no body, because a tool result is re-sent on every
later turn. Two identical calls in one turn's wave are still one load. The two flags are still
read as flags, so a built-in may be user-only or model-only exactly as a scanned one may. And a
built-in is reached only through the `skill` tool: ADR 0002 §1 refused to hand the model a
`cat` and let it decide what a file is worth, and a skill compiled into the binary does not
make that refusal easier to keep — it makes it plainer, since there is no file for a `cat` to
find and the body would otherwise be reachable only by the tool that owns the permission.

## The alternative that lost

Ship the skills as files in the repository's own `.agents/skills` — the built-ins committed
under a directory that the repo scope already scans — and let the existing walk find them.

It loses the thing the scope exists for. A run outside this repository would then be told
nothing, and a run inside it would be told "how this repository works" in the repository
scope's own words, which is the inference §2 refuses: the same body would read as house rules
the moment it spent a day in a working tree, and a user overriding it would be overriding a
repository's file, not Sindri's. It also puts the built-ins under the precedence of a nearer
scope and inside a directory a user may edit, so an instruction the binary depends on would be
a file that the next edit could change without the binary changing with it.

What the loss buys is one scan path instead of two, and one path is not worth a body whose
provenance is whatever directory happens to hold it.
