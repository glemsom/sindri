---
status: accepted
date: 2026-11-03
---

# 0005 — The program says what it is doing

`OSC 7501` is a terminal escape sequence by which a program says what it is doing: a state,
a one-line message, an optional `kind`. Sindri runs for minutes while the human is somewhere
else, which is the case the specification was written for. What a **status** is — the five
states, the mapping, the identity, and the three senses of *record* — is `GLOSSARY.md`'s.
This record holds the decisions only it makes. It is a record and not a module paragraph
because it binds the core and both front ends at once.

## 1. The mapping is `Stopped`'s, and three of its five words are `done`

`final`, `max_turns` and `budget` all report `done`, because in each case the answer is the
product and the run delivered one: a run that spent its cap or its budget was told to stop
and did. `error` is `error`, the word for a run that could not deliver, and `cancelled` is
`idle`, which is what the specification asks of an interrupted program — an interrupt leaves
the terminal as it found it, and a record still saying `working` for a process that has gone
is a terminal lying about a machine. `working` is not a `Stopped` at all: a run says it when
a turn begins, so the terminal knows the minutes are being spent. `blocked` is no turn's
word either, and only the frame sends it, with `kind=auth`, while the settings modal is
asking for a key.

No `progress`, ever. Turn 12 of a cap of 250 is not 5% of anything: what a turn costs is
whatever the command inside it costs, and the specification already makes an absent
`progress` mean indeterminate, which is exactly what we are.

## 2. Reports go out unqueried, and never as `9;4`

`OSC 7501 ; ?` is the specification's detection query, and it is refused: answering it
needs a reply read on stdin with a timeout, which is a poll on a stream a filter is
otherwise not reading — and it is a false negative on the terminal we are most likely
running in, whose graphics protocol the face already assumes. Probing would switch the
feature off exactly where it works.

Terminals MUST ignore an unknown OSC sequence, so an unsupported terminal costs what a pipe
costs: nothing. `9;4` is not sent either. The specification tells a terminal to stop mapping
it once a real report arrives, so it would be a second write to be thrown away, and it
carries a percentage — a state like `blocked` or `idle` has no fraction to say it in.

## 3. `msg` is coarse, and it is never the command text

One line of our own words: the turn, what the program is doing, how long it has been doing
it. Never the shell command, never the prompt, never the answer. The record's redaction
(`crates/core/src/record/redact.rs`) guards it by construction, and a report is a second
channel that guard cannot reach — it goes to a tty, while the run is happening. The
rule for a channel nothing guards is not "redact it" but "nothing worth redacting is ever
put in it".

## 4. Identity: `app=sindri`, the workspace's name, a root record only

`app=sindri`, one name and never a version, because a terminal keying a root record wants
the application. `title` is the workspace's own name and never a path, because a window
border has no room for one. No `id=`: record ids are for a program with a task tree, and
several sindri instances are several ptys, which are several root records.

## 5. A pure encoder in the core, and the writing left with the tty

`crates/core/src/status.rs`, the module `sindri_core::status`, is a pure encoder:
`status::report` returns the bytes and writes none. The writing stays with whoever owns the
tty: `crates/tui/src/run.rs` for the frame, after `terminal.draw` has returned and never
from inside a draw closure, and `crates/cli/src/batch.rs` for batch mode, on stderr and only
when stderr is a terminal, because stdout is the answer. A channel that is not a terminal is
written nothing at all.

How often either may be said is the channel's and not a front end's — `status::Channel`,
gated by `status::Told`: a state the terminal has already been told is not said again,
nothing is said twice inside a second, and a report the gate withholds is said late rather
than lost.

## 6. One switch, read where both front ends read the config

`status`, `SINDRI_STATUS` above the file, `true` unless either says otherwise. It is offered
in `/settings` like every other key, and each front end reads it where it stands. Not a
switch per front end, and not a flag: either would spell one decision twice. The setting
is a human's preference about what they see, so it never makes a file a terminal.

## The alternative that lost

Leave the terminal alone, since the frame already draws a **pulse**. The pulse is what a
human watching sees, and it says nothing once the terminal has no window in front of it and
nothing at all in batch mode. The two answer different questions — what is on the screen,
and what the program is doing — so both stay, and neither stands in for the other.
