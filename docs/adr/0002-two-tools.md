---
status: accepted
date: 2026-11-02
---

# 0002 — Three tools: `bash`, `skill` and `open`

The ground rules promised one tool. They promised two when this record was written, and they
promise three now. The argument for both of the tools the shell cannot carry lives here, in
the record whose file name — kept because the links that point at it are in the README and in
the tree — remembers the pair it argued first; ADR 0003 is the third tool's first telling,
superseded by §2, and it stays only as a pointer.

The three are one decision and not three, because the two after `bash` exist for the same
reason: a permission the shell cannot carry. `bash` runs everything — search, fetch, patch,
HTTP — behind the sandbox, so the model composes shell tools instead of being handed one per
job. What it cannot do is decide a thing on the model's behalf, and that is what the other two
tools are for. A skill is instructions written for the model, found in one of the scopes §3
names, carrying a permission that says whether the model may see it at all. `open`
puts a page or a file in front of the human's browser, which belongs to the host and not to
anything inside the sandbox. `crates/core/src/agent/tools.rs` builds the set, `skill.rs` and
`open.rs` are the two optional tools, and each is built only by a front end that has what it
needs to answer it.

## 1. `skill` is a second tool, and not a file the model reads through `bash`

`crates/core/src/skills/` scans and reads skills; `sindri doctor` prints the catalog. The tool
that calls one is `crates/core/src/agent/skill.rs`, built once at boot from that scan, and
`provider::Asked` carries the call it makes — a variant per tool — to the record.

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

## 2. `open` is a third tool, and the browser is the host's

`open` puts one thing in front of the human's eyes: a `url`, an `http` or `https` address
the host's browser fetches itself, or a `path`, a file a run could already have written.
Exactly one of the two is a target, and `crates/core/src/provider/asked.rs` reads both in
one place, so a call that names none, names both, or names a target the host may not be
handed is answered before anything is opened.

### 2.1 `xdg-open` through `bash` is a lie, and it exits 0

The claim is measured, not assumed. One command, run through the run's own sandbox:

```console
$ sindri sandbox exec -- xdg-open README.md
{"outcome":"exited","exit_code":0,
 "stdout":"\n\n  Your Terminal type is unknown!\n\n  Enter a terminal type: [vt100] \n
          TERMINAL TYPE IS SET TO vt100\n\u001b(B\u001b)0\u001b[1;24r\u001b[m ... \
          Getting file://localhost/home/glemsom/Documents/git/sindri/README.md ... \
          text/markdown D)ownload, or C)ancel ...",
 "stderr":"/usr/bin/xdg-open: line 1045: www-browser: command not found\n
          /usr/bin/xdg-open: line 1045: links2: command not found\n
          /usr/bin/xdg-open: line 1045: elinks: command not found\n
          /usr/bin/xdg-open: line 1045: links: command not found\n",
 "truncated":false,"duration_ms":20}
```

The `\` marks where a stream is folded to fit this page and the `...` where it is cut; the
escape sequences are lynx's own screen, with the reverse-video line naming the file, its
mimetype and its `D)ownload, or C)ancel`. It was run with `XDG_STATE_HOME` under a writable
path, because this workspace's own state directory is not writable; nothing else about the
invocation differs from a run's.

The chain is short and every link is in the tree. `crates/core/src/sandbox/argv.rs`
builds the environment the command runs in, and it is five names and nothing else:
`--clearenv --setenv HOME … --setenv TMPDIR … --setenv PATH … --setenv LANG C.UTF-8
--setenv LC_ALL C.UTF-8`. The same command run as `env` inside that sandbox prints those
five, plus `PWD`, `SHLVL=0` and the `_` a shell writes, and no `DISPLAY`, no
`WAYLAND_DISPLAY`, no `DBUS_SESSION_BUS_ADDRESS`; the host that started it has
`DISPLAY=:0`, `WAYLAND_DISPLAY=wayland-0` and a session bus address. xdg-open 1.2.1's
`has_display` reads those two names and nothing else, so `open_generic` skips the mime
handler; `run-mailcap` and `mimeopen` are both absent here, so neither short-circuits the
walk; `BROWSER` is unset, so the script walks its own list,
`www-browser:links2:elinks:links:lynx:w3m`; the first four are absent here; lynx is what
answered, and with no terminal it asked what the terminal was, took the `vt100` it offers,
and exited 0 — so the script reached `exit_success`.

The result that came back says `"exit_code":0`. Nothing was opened, nobody saw anything,
and a screenful of escape sequences went into the transcript, where a tool result is
re-sent with every later turn. The fallback is not a host accident we get to ignore:
`REQUIRED_BINARIES` in `crates/core/src/doctor.rs` holds `xdg-open` and `lynx` both,
resolved through the same `PATH` the sandbox uses, so a run is checked to have exactly the
pair that produces this.

The lie is the bug, not the missing window. A browser that could not be reached is a fact
the model can report; a success that opened nothing is a fact it will report as success,
and the human reads the answer that follows from it.

### 2.2 A result says `started`, and never that the human looked

The result's word is `started` — the host was asked, and the opener had not refused the
target when a quarter-second probe window closed — or it says `failed`, with the opener's own
words for why it exited inside that window. It never says the human looked: whether a browser drew
the page, whether the session had a display, whether anybody turned around, are all on the
far side of the process boundary, and a result that guessed at them would be the
`"exit_code":0` above with better manners. `Outcome::SandboxError` keeps the same discipline
from the other end — it says the sandbox never started, so no command ran, and never that a
command failed.

### 2.3 The refusals, in the call and not in the shell

A schema is worth having where it can refuse, so the refusals are the tool's and they are
made before the host is asked anything. Each names the ground it stopped on.

- **Target.** Exactly one of `url` and `path`. A call naming neither is answered with both
  shapes to send; a call naming both is refused as two targets. Empty is absent.
- **Scheme.** A `url` is `http` or `https`, and the host's browser fetches it. A `file:` URI
  is `path` in another spelling and not a second door: it is percent-decoded and its
  authority read on what decoding left, and the path it names then runs the `path` rules
  below and is refused in their words — so a reader of the record cannot tell which spelling
  the model chose. Every other scheme is refused, naming it.
- **Authority.** A `file:` URI names no authority, or `localhost`; any other host is refused
  naming it, because a run shows a human a file on this machine.
- **Extension.** A browser is for a known set of kinds, held in one list in one place —
  `html`, `htm`, `pdf`, `png`, `jpg`, `jpeg`, `gif`, `webp`, `svg`, `txt`, `md`, `csv`, and
  the refusal names the whole of it. Anything outside it is refused rather than handed to
  the host's mimetype guesser, because that guess is what decides which program the host
  launches.
- **Containment.** The path must lie inside what a run may write — the policy's own writable
  set and its session temp, never a list written in the tool. A model may put in front of a
  human what it could already write, and nothing else; a path outside it is a file to read,
  not a thing to open.
- **Kind.** What is handed over must be a regular file, since the host gives a file to
  whichever handler its desktop registered, and a browser is never given a directory or a
  device.

Containment and the extension are read twice: on the path the model wrote, and again on the
path it resolves to, which is what is handed over — because a link in the workspace pointing
at `/` is the escape, and a check on the string the model wrote is a check on the wrong string.
A link named `report.html` that resolves to a `payload.desktop` underneath is refused for its
kind and not for its spelling. The refusal for a link that escapes says what it stopped on and
not where the link led: naming the target would make the refusal a way to ask the host whether
a file the run cannot read exists.

### 2.4 What the core decides, and what only the host can do

The core owns the contract, and it yields one target. For a `path` that is the absolute,
resolved path, which is what a human could paste; for a `url` it is the text the model wrote,
because nothing resolves a URL and no filesystem is consulted for one. The display, the
session bus, the user's `PATH` and the browser belong to the process the run was started from,
and the sandbox is the thing that takes them away — so the opening is the host's, outside
bubblewrap, in the process that still holds them: `crates/cli/src/opener.rs` spawns `xdg-open`
detached, with the target as one argv word and never a word a shell reads, and all three
streams on `/dev/null` so that nothing an opener prints can reach the frame's terminal.

That split is why this is a tool and not a shell command. The core can refuse and cannot act;
the host can act and has no business guessing what the model meant. A call that names the tool
asks both in one step, with a target the core has already checked, and returns a sentence
about what happened rather than a stream of what a terminal did.

### 2.5 Batch mode and `sindri sandbox exec` have no `open` by absence

`Tools::new` (`crates/core/src/agent/tools.rs`) takes the optional tools as options, and
`specs()` is what the model is offered, so a tool a front end does not build is a tool the
schema never mentions. Batch mode (`crates/cli/src/batch.rs`) builds its tools with no `open`
at all, and holds no `skill` either when the workspace holds none the model may see; the
frame builds both, because the frame is where a human is.

Absence and not refusal, because batch mode's stdout is a contract: the answer and one
newline, read by a script. A script has no window and no one to look at it, so a browser is
not part of the contract — and a refusal is a sentence inside a tool result, which is a
thing the model pays for and the script never reads. A model that names the tool in a batch
run is answered the way `skill` answers a run that was handed no skill: the run says there
is no such tool here, which is a fact about the run's toolset and not about the target.

`sindri sandbox exec` gets the same absence one step earlier: it starts no model at all
(`crates/cli/src/sandbox.rs`), so no tool list is ever built and there is nothing to refuse.

## 3. Skill scopes, and the nearer one wins — three since ADR 0004

ADR 0004 adds a third scope below these two, a **built-in skill** compiled into the binary,
and this heading's count is the whole of the amendment here: its precedence is lowest, so the
order argued below is untouched and the built-in is only what the home directory's skill
shadows in turn.

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

## 4. The catalog in the tool's description, not in the preamble

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

`open`'s description is written the same way: it is in the one place a model reads while it
is choosing a tool, and it is paid for only in a front end that has a browser to hand the
target to.

## 5. Activation once per session

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

## 6. The two flags are Claude-compatible extensions

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

## The alternatives that lost

**One tool, and the model reads `SKILL.md` through `bash`.** It loses three things:

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

**Two tools, and a `sindri-open` on the sandbox `PATH`.** A small bridge that the model
reaches through `bash` like any other command, which tells the host to run `xdg-open` on the
path it was given. Two tools kept, and the effect still happens on the host.

What it costs is the schema. `sindri-open` is a generic host-effect verb: whatever string
the model composes, the host opens, and the only thing between the model and a mimetype
handler is a string that has already been through a shell. There is no argument to refuse
on and nowhere to put the refusal — the grounds above would live in the bridge's own
parsing or in nothing at all, and a bridge that lives on a `PATH` is reachable by every
script inside the sandbox, including one that was written to be reached that way. The
sandbox would not stop it either: the read-only root is a boundary on writes, not on reads,
so a socket the host already holds is reachable from inside, which is exactly why the
bridge would work and exactly why it is the wrong shape. A permission the model can route
around through the shell is a permission the shell enforces, and §1 already settled what
that is worth.

What the loss buys is two tools instead of three. Two is not worth a verb with no schema
and no refusal.
