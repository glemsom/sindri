---
status: accepted
date: 2026-11-02
---

# 0002 — Three tools: `bash`, `skill` and `open`

The three are one decision, not three: each tool after `bash` exists for a permission the
shell cannot carry. `bash` runs everything — search, fetch, patch, HTTP — behind the
sandbox, so the model composes shell tools instead of being handed one per job. What a
shell cannot do is decide something on the model's behalf, and that is what the other two
are for.

The file name remembers the pair it argued first, because the links that point here are in
the README and in the tree. ADR 0003, the third tool's first telling, is superseded by §2
and stays only as a pointer.

## 1. `skill` is a tool, and not a file the model reads through `bash`

`crates/core/src/agent/skill.rs` builds the tool at boot from the scan in
`crates/core/src/skills/`; `sindri doctor` prints the catalog it was built from.

The catalog is a set of known values, so a tool call takes a name from it while a `cat`
takes whatever path the model composed — including the one that lost a shadowing contest,
sorted differently on every scan. The body comes back frontmatter-stripped, inside a
`<skill_content>` wrapper, with the skill's own files listed beside it, so a relative
reference in the body resolves; a `cat` hands over the fence and the YAML and leaves every
reading of them to the model.

The two flags are why the choice is forced rather than convenient. A **user-only skill**
must be absent from the schema and the catalog; a **model-only skill** must be absent from
the frame's completion. Neither is a statement about the file — they are refusals made on
the model's behalf, and a permission enforced where the call is made is the only kind that
holds. A first-class call event is the rest of it: a chip in the frame, a check asserting a
load, and a column in the record all get to read the load where it happened instead of
inferring it from the shape of a command line.

## 2. `open` is a tool, and the browser is the host's

`open` puts one thing in front of the human — a `url` or a `path`, exactly one of the two,
read in one place by `crates/core/src/provider/asked.rs`. The core decides, refuses, and
yields one target: the resolved absolute path for a file, the text the model wrote for an
`http` or `https` URL.

### 2.1 `xdg-open` through `bash` is a lie, and it exits 0

Measured, not assumed: `sindri sandbox exec -- xdg-open README.md` returns
`"exit_code":0`. The sandbox's environment is five names and no `DISPLAY`, no
`WAYLAND_DISPLAY`, no session bus, so xdg-open skips the mime handler and walks its own
browser list until the one browser that is present — `lynx` — asks what the terminal is,
takes its default, and exits 0. A screenful of escape sequences goes into the transcript,
where a tool result is re-sent with every later turn.

Nothing was opened. The lie is the bug, not the missing window: a browser that could not be
reached is a fact the model can report, and a success that opened nothing is a fact it will
report as success.

### 2.2 A result says `started`, and never that the human looked

`started` means the host was asked and the opener had not refused the target when a
quarter-second probe window closed; `failed` carries the opener's own words for why it
exited inside that window. Whether a browser drew the page, whether the session had a
display, whether anybody turned around, are all on the far side of the process boundary.
`Outcome::SandboxError` keeps the same discipline from the other end: it says the sandbox
never started, never that a command failed.

### 2.3 The refusals, in the call and not in the shell

Each names the ground it stopped on.

- **Target.** Exactly one of `url` and `path`; empty is absent.
- **Scheme.** `http` or `https`. A `file:` URI is `path` in another spelling: it is
  percent-decoded and then runs the path rules, so a reader cannot tell which spelling the
  model chose. Every other scheme is refused, naming it.
- **Authority.** A `file:` URI names no authority, or `localhost`.
- **Extension.** One list in one place — `html`, `htm`, `pdf`, `png`, `jpg`, `jpeg`, `gif`,
  `webp`, `svg`, `txt`, `md`, `csv` — and the refusal names all of it. A mimetype guesser
  decides which program the host launches, so nothing outside the list reaches it.
- **Containment.** The path lies inside what a run may write — the policy's writable set and
  its session temp, never a list written in the tool.
- **Kind.** A regular file: a browser is never given a directory or a device.

Containment and the extension are read twice — on the path the model wrote and on the path
it resolves to — because a link in the workspace pointing at `/` is the escape, and a check
on the string the model wrote is a check on the wrong string. A refusal for a link that
escapes says what it stopped on and not where the link led.

### 2.4 What the host does, because the sandbox cannot

The display, the session bus and the browser belong to the process the run was started
from. `crates/cli/src/opener.rs` spawns `xdg-open` detached, with the target as one argv
word and never a word a shell reads, and all three streams on `/dev/null`. The core can
refuse and cannot act; the host can act and has no business guessing what the model meant.

### 2.5 Absence, where a front end has no one to look

`Tools::new` (`crates/core/src/agent/tools.rs`) takes the optional tools as options, and
`specs()` is what the model is offered, so a tool a front end does not build is a tool the
schema never mentions. Batch mode builds no `open`, and no `skill` when the workspace holds
none the model may see; `sindri sandbox exec` starts no model at all.

Absence and not refusal, because batch mode's stdout is a contract read by a script: a
script has no window, and a refusal is a sentence inside a tool result the script never
reads. A model that names the tool is answered that there is no such tool here, which is a
fact about the run's toolset and not about the target.

## 3. Skill scopes

`~/.agents/skills` is the user scope; every `.agents/skills` between the workspace and its
repository root is the repository scope, nearest first and never above the root. ADR 0004
adds a third below both — a **built-in skill** compiled into the binary, and so the lowest
precedence. The Agent Skills specification governs what is inside a skill directory and
says nothing about where one lives, so these paths are ours. A name that lost is recorded
as **shadowing** rather than dropped, so `sindri doctor` prints the winner and what it
displaced.

A directory holding a `SKILL.md` is a skill. Links to directories are followed, because
that is how people share one, and a link to nothing is a refusal with the path and line to
look at. A file the scan will not load is a diagnostic, never a silence and never a failure
of the whole catalog.

## 4. The catalog lives in the tool's description, not in the preamble

The preamble is re-sent on every turn, so its every word is a per-request tax; a catalog
changes with the workspace and can be long. In the tool's description it is paid only where
it buys something and only for what exists — with no model-visible skill the tool is not
registered and the preamble stays byte-identical. It also sits beside the name of the tool
it describes, which is the one place a model reads while it is choosing one. `open`'s
description is written the same way, and paid for only in a front end that has a browser to
hand a target to.

## 5. Activation once per session

A second call for a loaded skill returns `already_loaded` and no body, because a tool result
is carried in the conversation and re-sent with every later turn: the second copy is paid on
every turn that follows it. The loaded set is per run and cleared whenever the history is,
because activation is a fact about a conversation and not about a process. Two identical
calls in one wave are one load for the same reason.

`/skill:house-style` is the same call from the human's side (`Agent::activate`), so the
claim it makes is the one the model's later call reads as `already_loaded`. Neither asks the
provider anything: a body in force is a fact about the next turn.

## 6. The two flags are Claude-compatible extensions

`disable-model-invocation` and `user-invocable` are not in the specification. They are read
anyway, because Claude Skills writes them and a skill written for one tool should not lose
its permissions in another. They are read as plain YAML booleans and nothing else: absent,
or present as a quoted string, leaves the permission the specification grants in place, and
everything else a skill's frontmatter carries is ignored rather than refused.

## The alternatives that lost

**One tool, and the model reads `SKILL.md` through `bash`.** It loses deduplication (nothing
in the loop knows what has been read), frontmatter control (a permission the model reads and
honours is a suggestion), and the indicator (a load would have to be inferred from the shape
of a command line). One tool is not worth a permission nobody enforces.

**A `sindri-open` on the sandbox `PATH`.** Two tools kept and the effect still on the host,
at the cost of the schema: whatever string the model composes is what the host opens, there
is no argument to refuse on, and the read-only root is a boundary on writes and not on reads
— so a host socket is reachable from inside, which is exactly why the bridge would work and
exactly why it is the wrong shape.
