---
status: accepted
date: 2026-11-02
---

# 0002 — Three tools: `bash`, `skill` and `open`

`bash` runs everything — search, fetch, patch, HTTP — behind the sandbox, so the model
composes shell tools instead of being handed one per job. What a shell cannot do is decide
something on the model's behalf, and that is what the other two tools are for.

## 1. `skill` is a tool, not a file read through `bash`

`crates/core/src/agent/skill.rs` builds the tool at boot from the scan in
`crates/core/src/skills/`; `sindri doctor` prints the catalog it was built from.

A catalog is a set of known values, so a call takes a name from it, while a `cat` takes
whatever path the model composed — including the one that lost a shadowing contest. The
body comes back frontmatter-stripped, inside a `<skill_content>` wrapper, with the skill's
own files listed beside it so a relative reference resolves.

The two flags force the choice rather than recommend it: a **user-only skill** must be
absent from the schema and the catalog, a **model-only skill** from the frame's
completion. Neither is a statement about a file — both are refusals made on the model's
behalf, and a permission enforced where the call is made is the only kind that holds. A
first-class call event is the rest of it: a chip in the frame, a check asserting a load,
and a column in the record read the load where it happened instead of inferring it from a
command line.

## 2. `open` is a tool, and the browser is the host's

`open` puts one thing in front of the human — a `url` or a `path`, exactly one of the two,
read in one place by `crates/core/src/provider/asked.rs`. The core decides, refuses, and
yields one target: the resolved absolute path for a file, the text the model wrote for an
`http` or `https` URL.

### 2.1 `xdg-open` through `bash` is a lie, and it exits 0

Measured, not assumed: `sindri sandbox exec -- xdg-open README.md` returns
`"exit_code":0`. The sandbox's environment is five names and no `DISPLAY`, no
`WAYLAND_DISPLAY`, no session bus, so xdg-open skips the mime handler and walks its own
browser list until `lynx` asks what the terminal is, takes its default, and exits 0. A
screenful of escape sequences goes into the transcript, where a tool result is re-sent
with every later turn.

Nothing was opened, and the lie is the bug: a browser that could not be reached is a fact
the model can report, while a success that opened nothing is a fact it reports as success.

### 2.2 A result says `started`, never that the human looked

`started` means the host was asked and the opener had not refused the target when a
quarter-second probe window closed; `failed` carries the opener's own words for why it
exited inside that window. Whether a browser drew the page is on the far side of the
process boundary. `Outcome::SandboxError` keeps the same discipline: it says the sandbox
never started, never that a command failed.

### 2.3 The refusals live in the call, not the shell

- **Target.** Exactly one of `url` and `path`; empty is absent.
- **Scheme.** `http` or `https`. A `file:` URI is `path` in another spelling —
  percent-decoded, then the path rules — so a reader cannot tell which spelling the model
  chose. Any other scheme is refused, naming it.
- **Authority.** A `file:` URI names no authority, or `localhost`.
- **Extension.** One list in one place — `html`, `htm`, `pdf`, `png`, `jpg`, `jpeg`, `gif`,
  `webp`, `svg`, `txt`, `md`, `csv` — and a refusal names all of it. A mimetype guesser
  decides which program the host launches, so nothing outside the list reaches it.
- **Containment.** The path lies inside what a run may write — the policy's writable set
  and its session temp, never a list written in the tool.
- **Kind.** A regular file: a browser is never given a directory or a device.

Containment and the extension are read twice — on the string the model wrote and on the
path it resolves to — because a link in the workspace pointing at `/` is the escape, and a
check on the model's string is a check on the wrong string. A refusal for such a link says
what it stopped on and not where the link led.

### 2.4 The host spawns, because the sandbox cannot

The display, the session bus and the browser belong to the process the run was started
from. `crates/cli/src/opener.rs` spawns `xdg-open` detached, with the target as one argv
word and never a word a shell reads, all three streams on `/dev/null`. The core can refuse
and cannot act; the host can act and has no business guessing what the model meant.

### 2.5 A front end with no one to look builds no `open`

`Tools::new` (`crates/core/src/agent/tools.rs`) takes the optional tools as options, and
`specs()` is what the model is offered, so a tool a front end does not build is a tool the
schema never mentions. Batch mode builds no `open`, and no `skill` when the workspace
holds none the model may see; `sindri sandbox exec` starts no model at all.

Absence rather than refusal, because batch mode's stdout is a contract read by a script
that has no window: a refusal is a sentence inside a tool result the script never reads. A
model that names the missing tool is answered that there is no such tool here — a fact
about the toolset, not about the target.

## 3. Skill scopes

`~/.agents/skills` is the user scope; every `.agents/skills` between the workspace and its
repository root is the repository scope, nearest first and never above the root. ADR 0003
adds a third below both, compiled into the binary. The Agent Skills specification governs
what is inside a skill directory and says nothing about where one lives, so these paths are
ours. A name that lost is recorded as **shadowing** rather than dropped, so `sindri doctor`
prints the winner and what it displaced.

A directory holding a `SKILL.md` is a skill; links to directories are followed, because
that is how people share one, and a link to nothing is a refusal naming the path and line.
A file the scan will not load is a diagnostic, never a silence and never a failure of the
whole catalog.

## 4. The catalog lives in the tool's description

The preamble is re-sent on every turn, so its every word is a per-request tax, while a
catalog changes with the workspace and can be long. In the tool's description it is paid
only where it buys something and only for what exists: with no model-visible skill the
tool is not registered and the preamble stays byte-identical. It also sits beside the name
of the tool it describes, where a model reads while choosing one. `open`'s description is
written the same way, and paid for only where there is a browser to hand a target to.

## 5. Activation once per session

A second call for a loaded skill returns `already_loaded` and no body, because a tool
result is carried in the conversation and re-sent with every later turn. The loaded set is
per run and cleared whenever the history is, because activation is a fact about a
conversation and not about a process; two identical calls in one wave are one load for the
same reason.

`/skill:house-style` is the same call from the human's side (`Agent::activate`), so the
claim it makes is the one the model's later call reads as `already_loaded`.

## 6. The two flags are Claude-compatible extensions

`disable-model-invocation` and `user-invocable` are not in the specification, and are read
anyway because Claude Skills writes them: a skill written for one tool should not lose its
permissions in another. They are read as plain YAML booleans and nothing else — absent, or
quoted, leaves the permission the specification grants in place — and the rest of a
skill's frontmatter is ignored rather than refused.

## The alternatives that lost

**One tool, with `SKILL.md` read through `bash`.** It loses deduplication (nothing in the
loop knows what has been read), frontmatter control (a permission the model honours is a
suggestion) and the indicator (a load inferred from the shape of a command line).

**A `sindri-open` on the sandbox `PATH`.** The effect is still on the host, at the cost of
the schema: whatever string the model composes is what the host opens, there is no argument
to refuse on, and the read-only root is a boundary on writes and not on reads — so a host
socket is reachable from inside, which is exactly why the bridge would work and exactly why
it is the wrong shape.
