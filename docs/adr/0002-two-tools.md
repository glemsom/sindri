---
status: accepted
date: 2026-11-02
---

# 0002 — Two tools: `bash`, and `skill`

The ground rules promised one tool. They now promise two, and the argument lives here so
the third agent to read the agent core finds a decision rather than a hole. They promise
three since ADR 0003, which adds `open`; what follows is unchanged and stands under it —
this record is why `bash` is not the tool a permission is enforced in, and 0003 is that
argument made about a browser instead of a skill.

A skill is not a shell composition. It is instructions written for the model, found in
someone's home directory or in the repository the run happens in, carrying a permission
that says whether the model may see it at all. Nothing about that is a command, so
loading one through `bash` would be the model reading a file and deciding for itself what
the file is worth — and a decision the reading side makes is not an enforcement.

`crates/core/src/skills/` scans and reads skills; `sindri doctor` prints the catalog.
The tool that calls one is `crates/core/src/agent/skill.rs`, built once at boot from that
scan, and `provider::Asked` carries the call it makes — a variant per tool — to the
record.

## 1. A second tool, and not a file the model reads through `bash`

The catalog is a set of known values, in the repository's own precedence order, and the
schema should say so. A tool call takes a name from that set; a `cat` takes whatever path
the model composed, including the one that lost a shadowing contest, sorted differently
on every scan.

The body comes back frontmatter-stripped, inside a `<skill_content>` wrapper, with the
skill's directory and its file listing beside it, so `references/api.md` in the body
resolves. A `cat` hands over the fence and the YAML and leaves both readings to the
model: which fields matter, what the flags mean, whether the file even parsed.

The two invocation flags are the reason the choice is forced rather than merely
convenient. A **user-only skill** must be absent from the schema and from the catalog the
tool carries, and a **model-only skill** must be absent from the frame's completion.
Neither is anything the model can be trusted with, because neither is a statement about
the file: they are refusals made on its behalf. A permission enforced where the call is
made is the only kind that holds, and `sindri doctor` still prints both, because a human
has to be able to see the skill they may run.

## 2. Two scopes, and the nearer one wins

`~/.agents/skills` is the user scope. Every `.agents/skills` between the workspace and
its repository root is the repository scope, nearest first and never above the root: a
directory above it belongs to somebody else's repository.

The Agent Skills specification governs what is inside a skill directory and says nothing
about where one lives, so these two paths are ours. The precedences follow the paths: a
crate's skill takes a name from the repository's, and the repository's from the home
directory's. A name that lost is recorded as **shadowing** rather than dropped, so
`sindri doctor` prints the winner and what it displaced and a reader can see both.

A directory holding a `SKILL.md` is a skill; a directory that holds none is not. Links
to directories are followed, because that is how people share a skill, and a link to
nothing is a **refusal** with the path and line to look at. A file the scan will not load
is a diagnostic, never a silence and never a failure of the whole catalog.

## 3. The catalog in the tool's description, not in the preamble

The preamble is re-sent on every turn of every run, so its words are a tax paid per
request — the measurement in `docs/system-prompt.md` exists because that tax is the
thing being tuned. A catalog changes with the workspace and can be long, and putting it
in a fixed string would make every run pay for every run's skills.

In the tool description it is paid only where it buys something and only for what exists:
with no model-visible skill the tool is not registered at all and the preamble stays
byte-identical to what it was. The description also sits beside the name of the tool it
describes, which is the one place a model reads while it is choosing one; a model that
has to be told in prose that a tool exists has to remember it, and a model that reads it
off the schema does not.

## 4. Activation once per session

A second call for a loaded skill returns `already_loaded` and no body. A tool result is
carried in the conversation for the rest of the run and sent again with every later turn,
so the second copy is not paid once: it is paid on every turn that follows it. The
deduplication is the same economy as the bash description's "filter in the script and
print only what you concluded", applied to the one result whose content never changes.

The loaded set is per run and is cleared whenever the history is, because activation is a
fact about a conversation and not about a process: `/new` starts a history that has not
been told anything, and a set that outlived it would say a skill is in force in a
conversation that never received it. Two identical calls inside one parallel wave are one
load for the same reason — the wave is one turn, and the set is what the turn leaves
behind.

The composer is the other caller. `/skill:house-style` is the same call run from the
human's side — `Agent::activate` — so the claim it makes is the one the model's later
call reads as `already_loaded`, and a skill the model may not see is still a skill a
human can put in force. Neither asks the provider anything: a body in force is a fact
about the next turn, not a question to ask now.

## 5. The two flags are Claude-compatible extensions

`disable-model-invocation: true` and `user-invocable: false` are not in the Agent Skills
specification. They are read anyway, because Claude Skills writes them and a skill
written for one tool should not lose its permissions when it is read by another.

They are read as flags and nothing else. Absent — or present as anything but a plain
YAML boolean, a quoted string included — leaves the permission the specification grants
in place, because a skill the specification would accept is not refused over a key the
specification does not define. A quoted `"true"` is a quotation: the reader who wrote
quotes did not write a switch. Everything else a skill's frontmatter carries is ignored
rather than refused, so the extension stays an extension.

The pair is worth having because it names two skills that are otherwise inexpressible: a
house rule a human may reach for and the model may not see, and an instruction the model
must follow and a human must not be able to type.

## The alternative that lost

Keep one tool and let the model read `SKILL.md` through `bash`. It loses three things:

- **Deduplication.** Nothing in the loop knows what has been read, so a model that cats
  the same file twice pays for it twice, and pays again on every turn after; the set that
  makes the second call free has no home outside the invocation.
- **Frontmatter control.** The model would read the fence as prose and decide for itself
  whether `disable-model-invocation: true` binds it. A permission the model reads and
  honours is a suggestion, and a `user-invocable: false` skill would already be in front
  of it — the catalog it came in was the thing the flag was meant to keep it out of.
- **The indicator.** A check asserting the model loaded a skill, a chip in the frame
  saying which one is in force, a column in the session record: all of them would have to
  infer a load from the shape of a command line — a pattern over
  `cat .agents/skills/*/SKILL.md`. Fragile, and wrong the moment someone writes a script
  that reads a skill. A first-class call event says it once, where it happens.

What the loss buys is one tool instead of two, and one tool is not worth a permission
nobody enforces.
