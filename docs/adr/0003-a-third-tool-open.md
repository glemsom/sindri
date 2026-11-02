---
status: accepted
date: 2026-11-02
---

# 0003 — A third tool: `open`

The ground rules said two tools. They now say three, and the third is argued here for the
reason ADR 0002 was written: the next agent to read the agent core should find a decision
rather than a hole. 0002 argued a tool where `bash` could not carry a permission, because
a skill is instructions and its flags are refusals made on the model's behalf. This record
makes the same argument about something else the shell cannot deliver: the browser belongs
to the host, and a command inside the sandbox that says it opened one has said something it
cannot know.

## 1. `xdg-open` through `bash` is a lie, and it exits 0

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

## 2. A result says `started`, and never that the human looked

The tool takes one path and hands it over. The result's word is `started` — the path it
handed over, and that the host was asked for it — or that it started nothing, with the
host's own words for why. It never says the human looked: whether a browser drew the page,
whether the session had a display, whether anybody turned around, are all on the far side
of the process boundary, and a result that guessed at them would be the `"exit_code":0`
above with better manners. `Outcome::SandboxError` keeps the same discipline from the other
end — it says the sandbox never started, so no command ran, and never that a command
failed.

## 3. Four refusals, in the call and not in the shell

A schema is worth having where it can refuse, so the refusals are the tool's and they are
made before the host is asked anything.

- **Scheme.** The argument is a path in the workspace. A string carrying a scheme is
  refused, `file:` and `http:` alike: a browser handed a URL has left the workspace, and a
  run has no claim on where it went.
- **Extension.** A browser is for a known set of files, held in one list in one place, and
  anything outside it is refused rather than handed to the host's mimetype guesser —
  because that guess is what decides which program the host launches.
- **Containment.** The path must lie inside the writable set. A model may put in front of
  a human what it could already write, and nothing else; a path outside it is a file to
  read, not a thing to open.
- **Symlink escape.** Containment is decided on the resolved path, and the resolved path is
  what is handed over. A link in the workspace pointing at `/` is refused, naming the link
  and where it led, because a check on the string the model wrote is a check on the wrong
  string.

**Amendment (issue #288): `file:` is read, not refused.** The scheme bullet above named
`file:` and `http:` alike. A `file:` URI is now a second spelling of `path` and not a second
door: the URI is percent-decoded and its authority read on what decoding left, so an empty
authority and `localhost` reduce to the same path and any other is refused naming it, and the
reduced path then runs the guard the
`path` argument runs and is refused in the identical words — so a reader of the record
cannot tell which spelling the model chose, and nothing a `file:` URI opens may be something
a `path` could not. `http:` and every other scheme stay refused as written here.

## 4. What the core decides, and what only the host can do

The core owns the contract: it reads the argument, applies the four refusals, and yields
one absolute path or a refusal that names the ground it stopped on. That is the whole of
what it can know. The display, the session bus, the user's `PATH` and the browser belong
to the process the run was started from, and the sandbox is the thing that takes them away
— so the opening is the host's, outside bubblewrap, in the process that still holds them.

That split is why this is a tool and not a shell command. The core can refuse and cannot
act; the host can act and has no business guessing what the model meant. A call that names
the tool asks both in one step, with a path the core has already checked, and returns a
sentence about what happened rather than a stream of what a terminal did.

## 5. Batch mode and `sindri sandbox exec` have no `open` by absence

`Tools::new` (`crates/core/src/agent/tools.rs`) takes the optional tools as options, and
`specs()` is what the model is offered, so a tool a front end does not build is a tool the
schema never mentions. Batch mode (`crates/cli/src/batch.rs`) builds its tools with no
`open` at all, and holds no `skill` either when the workspace holds none the model may see;
the frame builds both, because the frame is where a human is.

Absence and not refusal, because batch mode's stdout is a contract: the answer and one
newline, read by a script. A script has no window and no one to look at it, so a browser is
not part of the contract — and a refusal is a sentence inside a tool result, which is a
thing the model pays for and the script never reads. A model that names the tool in a batch
run is answered the way `skill` answers a run that was handed no skill: the run says there
is no such tool here, which is a fact about the run's toolset and not about the path.

`sindri sandbox exec` gets the same absence one step earlier: it starts no model at all
(`crates/cli/src/sandbox.rs`), so no tool list is ever built and there is nothing to refuse.

## The alternative that lost

Keep two tools and put `sindri-open` on the sandbox `PATH` — a small bridge that the model
reaches through `bash` like any other command, which tells the host to run `xdg-open` on
the path it was given. One tool kept, and the effect still happens on the host.

What it costs is the schema. `sindri-open` is a generic host-effect verb: whatever string
the model composes, the host opens, and the only thing between the model and a mimetype
handler is a string that has already been through a shell. There is no argument to refuse
on and nowhere to put the refusal — the four grounds above would live in the bridge's own
parsing or in nothing at all, and a bridge that lives on a `PATH` is reachable by every
script inside the sandbox, including one that was written to be reached that way. The
sandbox would not stop it either: the read-only root is a boundary on writes, not on reads,
so a socket the host already holds is reachable from inside, which is exactly why the
bridge would work and exactly why it is the wrong shape. A permission the model can route
around through the shell is a permission the shell enforces, and ADR 0002 already settled
what that is worth.

What the loss buys is two tools instead of three. Two is not worth a verb with no schema
and no refusal.
