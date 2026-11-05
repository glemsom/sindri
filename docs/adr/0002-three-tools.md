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
`crates/core/src/skills/`. A catalog is a set of known values, so a call takes a name from
it, while a `cat` takes whatever path the model composed — including the one that lost a
shadowing contest. The body comes back frontmatter-stripped, in a `<skill_content>`
wrapper, with the skill's own files listed so a relative reference resolves.

A **user-only skill** is absent from the schema and the catalog, a **model-only skill**
from the frame's completion: a permission enforced where the call is made is the only kind
that holds. A first-class call event is the rest of it — a chip in the frame, a check
asserting a load, a column in the record that reads the load where it happened.

## 2. `open` is a tool, and the browser is the host's

`open` yields one target from a `url` or a `path`, exactly one, read in one place by
`crates/core/src/provider/asked.rs`: the resolved absolute path for a file, the text the
model wrote for an `http` or `https` URL.

**`xdg-open` through `bash` is a lie, and it exits 0.** Measured: `sindri sandbox exec --
xdg-open README.md` returns `"exit_code":0`. With no `DISPLAY`, no `WAYLAND_DISPLAY` and
no session bus, xdg-open walks its browser list until `lynx` takes a default and exits
clean, putting a screenful of escapes in the transcript, which every later turn re-sends.

**A result says `started`, never that the human looked.** `started` means the host was
asked and the opener had not refused the target when a quarter-second probe closed;
`failed` carries the opener's own words. Whether a page was drawn is on the far side of
the process boundary. `Outcome::SandboxError` keeps the discipline: the sandbox never
started, never that a command failed.

**The refusals live in the call, not the shell.**

- **Target.** Exactly one of `url` and `path`; empty is absent.
- **Scheme.** `http` or `https`. A `file:` URI is `path` in another spelling —
  percent-decoded, then the path rules — so a reader cannot tell which the model chose.
- **Authority.** A `file:` URI names no authority, or `localhost`.
- **Extension.** One list in one place — `html`, `htm`, `pdf`, `png`, `jpg`, `jpeg`, `gif`,
  `webp`, `svg`, `txt`, `md`, `csv` — and a refusal names all of it. A mimetype guesser
  decides which program the host launches, so nothing outside the list reaches it.
- **Containment.** The path lies inside what a run may write: the policy's writable set and
  its session temp, never a list written in the tool.
- **Kind.** A regular file: a browser is never given a directory or a device.

Containment and the extension are read twice — on the string the model wrote and on the
path it resolves to — because a link in the workspace pointing at `/` is the escape.

**The host spawns, because the sandbox cannot.** The display and the browser belong to the
process the run was started from. `crates/cli/src/opener.rs` spawns `xdg-open` detached,
the target one argv word and never a word a shell reads, all three streams on `/dev/null`.

**A front end with no one to look builds no `open`.** `Tools::new`
(`crates/core/src/agent/tools.rs`) takes the optional tools as options and `specs()` is
what the model is offered, so a tool nobody builds is a tool the schema never mentions.
Batch mode builds no `open`, and no `skill` when the workspace holds none the model may
see; `sindri sandbox exec` starts no model at all. Absence rather than refusal, because
batch mode's stdout is a contract read by a script with no window.

## 3. Skill scopes

`~/.agents/skills` is the user scope; every `.agents/skills` between the workspace and its
repository root is the repository scope, nearest first and never above the root. ADR 0003
adds a third below both, compiled into the binary. The Agent Skills specification governs
what is inside a skill directory and says nothing about where one lives, so these paths are
ours. A name that lost is recorded as **shadowing**, so `sindri doctor` prints the winner
and what it displaced. A directory holding a `SKILL.md` is a skill; links to directories
are followed, because that is how people share one. A link that resolves to nothing is
refused where the scan finds it, by its path alone. Only a `SKILL.md` whose frontmatter
was parsed has a line to name, so a `SKILL.md` that will not parse is refused by its path
and the line that broke it. A file the scan will not load is a diagnostic, never a
silence and never a failure of the whole catalog.

## 4. The catalog lives in the tool's description

The preamble is re-sent every turn, so its every word is a tax, while a catalog changes
with the workspace. In the tool's description it is paid only for what exists: with no
model-visible skill the tool is not registered and the preamble stays byte-identical. It
also sits beside the name of the tool it describes, where a model reads while choosing one.

## 5. Activation once per session, and two flags read anyway

A second call for a loaded skill returns `already_loaded` and no body, because a tool
result is re-sent with every later turn. The loaded set is per run, cleared whenever the
history is, and two identical calls in one wave are one load. `/skill:house-style` is the
same call from the human's side (`Agent::activate`).

`disable-model-invocation` and `user-invocable` are not in the specification and are read
because Claude Skills writes them: a skill written for one tool should not lose its
permissions in another. Plain YAML booleans only — absent or quoted leaves the
specification's permission in place — and the rest of the frontmatter is ignored.

## The alternatives that lost

**One tool, with `SKILL.md` read through `bash`.** It loses deduplication, frontmatter
control (a permission the model honours is a suggestion) and the indicator.

**A `sindri-open` on the sandbox `PATH`.** Still an effect on the host, at the cost of the
schema: whatever string the model composes is what the host opens, and the read-only root
bounds writes and not reads, so a host socket is reachable from inside.
