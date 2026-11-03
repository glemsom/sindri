---
status: accepted
date: 2026-11-03
---

# 0004 — The program says what it is doing

`OSC 7501` is the terminal escape sequence by which a program says what it is doing: a
state, a one-line message, an optional `kind`. Sindri runs for minutes while the human is
somewhere else, which is the case the specification was written for. What a **status** is —
the five states, the identity, the three senses of *record* — is `GLOSSARY.md`'s; this
record holds what binds the core and both front ends at once.

## 1. The mapping is `Stopped`'s, and three of its five words are `done`

`final`, `max_turns` and `budget` all report `done`: the answer is the product and the run
delivered one. `error` is `error`, and `cancelled` is `idle`, which is what the
specification asks of an interrupted program — a record still saying `working` for a
process that has gone is a terminal lying about a machine. `working` is no `Stopped` at
all: a run says it when a turn begins. `blocked` is no turn's word either: only the frame
sends it, with `kind=auth`, while the settings modal asks for a key.

No `progress`, ever. Turn 12 of a cap of 250 is not 5% of anything, and an absent
`progress` already means indeterminate, which is what we are.

## 2. Reports go out unqueried, and never as `9;4`

`OSC 7501 ; ?` is the specification's detection query, and it is refused: answering needs
a reply read on stdin with a timeout, a poll on a stream a filter is otherwise not reading,
and it would be a false negative on the terminal we most likely run in. Terminals MUST
ignore an unknown OSC sequence, so an unsupported terminal costs what a pipe costs.

`9;4` is not sent either: the specification tells a terminal to stop mapping it once a real
report arrives, and it carries a percentage `blocked` or `idle` has no fraction to say.

## 3. `msg` is coarse, and never the command text

One line of our own words: the turn, what the program is doing, how long. Never the shell
command, never the prompt, never the answer. The record's redaction
(`crates/core/src/record/redact.rs`) guards it by construction; a report goes to a tty, a
second channel that guard cannot reach, so the rule there is not "redact it" but "nothing
worth redacting is ever put in it".

## 4. Identity: `app=sindri`, the workspace's name, a root record only

`app=sindri`, one name and never a version, because a terminal keying a root record wants
the application. `title` is the workspace's own name and never a path: a window border has
no room for one. No `id=`: several sindri instances are several ptys, which are several
root records.

## 5. A pure encoder in the core, the writing left with the tty

`crates/core/src/status.rs` is a pure encoder: `status::report` returns the bytes and
writes none. Writing stays with whoever owns the tty — `crates/tui/src/run.rs` for the
frame, after `terminal.draw` has returned and never from inside a draw closure, and
`crates/cli/src/batch.rs` for batch mode, on stderr and only when stderr is a terminal,
because stdout is the answer.

How often either may speak is the channel's business: `status::Channel`, gated by
`status::Told` — a state already told is not said again, nothing twice inside a second,
and a report the gate withholds is said late rather than lost.

## 6. One switch, read where both front ends read the config

`status`, `SINDRI_STATUS` above the file, `true` unless either says otherwise. It is
offered in `/settings` like every other key. Not a switch per front end and not a flag:
either would spell one decision twice. It is a human's preference about what they see, so
it never makes a file a terminal.

## The alternative that lost

Leave the terminal alone, since the frame already draws a **pulse**. The pulse is what a
human watching sees, and says nothing once the terminal has no window in front of it, and
nothing at all in batch mode. The two answer different questions, so both stay.
