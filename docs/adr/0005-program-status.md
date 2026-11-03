---
status: accepted
date: 2026-11-03
---

# 0005 — The program says what it is doing

`OSC 7501` is a terminal escape sequence by which a program says what it is doing: a state —
`idle`, `working`, `done`, `blocked` or `error` — a one-line message, and an optional `kind`.
Sindri is a run that takes minutes while the human is somewhere else, which is the case the
specification was written for, and today it says nothing to the terminal it draws in.

This is a record and not a module paragraph, which `docs/agents/domain.md` otherwise prefers,
because the decision is not one module's: a pure encoder in the core and a rule about what may
never be put in its output bind the core and both front ends at once, and the words it
introduces are `GLOSSARY.md`'s. What a **status** is — the five states, the mapping, the
identity, and the three senses of *record* — belongs to that entry, so it is not repeated here.
This record holds the seven decisions only it makes.

## 1. The mapping is `Stopped`'s, and three of its five words are `done`

`crates/core/src/agent/report.rs` closes the ways a run can end at five words, and no sixth.
`Final`, `MaxTurns` and `Budget` — `final`, `max_turns` and `budget` in the record — all report
`done`, because in each case the answer is the product and the run delivered one: a run that
spent its turn cap or its budget is a run that was told to stop and did, not a run that failed,
and a terminal told `error` for it would send a human to look at a machine that is working
exactly as asked. `Error` is `error`, the one word for a run that could not deliver, and
`Cancelled` — `cancelled` — is `idle`, because that is what the specification asks of an
interrupted program, and because it is true: an interrupt leaves the terminal as it found it,
holding nothing, and a record still saying `working` for a process that has gone is a terminal
lying about a machine.

`working` is not a `Stopped` at all, since it is not a way to end: a run says it when a turn
begins, so the terminal knows the minutes are being spent and not lost. `blocked` is no turn's
word, and the mapping says so rather than leaving a reader to wonder: nothing in a turn waits on
a human — the sandbox asks no permission, the tools ask no confirmation — so a run is never in the
state the word names. The frame is the one thing that sends it, with `kind=auth`, while the
settings modal it staged is asking for a key: a run with no credential cannot answer, that is the
one genuine blockage sindri has, and a human in another tab is owed knowing that someone has to
type something rather than that the machine is thinking.

No `progress`, ever. Turn 12 of a cap of 250 is not 5% of anything: what a turn costs is
whatever the command inside it costs, so sixty turns that each ran a one-second command and one
turn that took an hour are not a fifth and a hundredth of the same work, and the specification
already makes an absent `progress` mean indeterminate, which is exactly what we are. The
alternative — a figure that climbs on the turn counter — is a promise the code cannot keep, and
a terminal drawing a bar that fills at 250 turns and never arrives is worse than a terminal
drawing nothing.

## 2. Reports go out unqueried

`OSC 7501 ; ?` is the specification's detection query, and it is refused. Answering it needs a
reply read on stdin with a timeout, which is a poll on a stream the run is otherwise not
reading — and a `.5s` timeout on every start of a program built to be a filter. Worse, the
detection is a false negative on the terminal we are most likely running in: Ghostty parses
these reports and answers no query, and its graphics protocol is the one the face already
assumes (`crates/tui/src/face.rs`). Probing would therefore switch the feature off exactly
where it works, in exchange for a start-up cost and an input channel nobody else needed.

Terminals MUST ignore an unknown OSC sequence, so an unsupported terminal costs nothing, which
is exactly what it costs when the report is written into a pipe. That mandatory silence is what
makes reporting unconditionally cheaper than asking, and it is the argument — not optimism —
that sends `working` to a terminal that may not read it.

## 3. Not `OSC 9;4` as well

`9;4` is the older progress escape, and the specification tells a terminal to stop mapping it
onto the root record once a real report arrives. Sending both would therefore be sending the
older one to be thrown away, in a second write where one suffices and a second spelling of one
fact where the glossary exists to keep one.

It also cannot say the thing this record is about. `9;4` carries a percentage, so a program
that is waiting, or one that has stopped, has no way to say so except as a number between two
it has already sent; `blocked` and `idle` are states and not fractions. Emitting it alone would
leave the frame's terminal drawing a bar for a run the human cannot see, which is the failure
the report is written to end.

## 4. `msg` is coarse, and it is never the command text

One line of our own words: the turn, what the program is doing, how long it has been doing it.
Never the shell command, never the prompt, never the answer.

`crates/core/src/record/redact.rs` holds the record's rule by construction — every string the
record writes passes through `Redaction` first, so a new field cannot forget — and a report is
a second output channel that guard cannot reach, because it is written to a tty rather than to
a file the record owns, and it is written while the run is happening rather than at the end.
The rule for a channel nothing guards is therefore not "redact it" but "nothing worth redacting
is ever put in it": a report carries what the program is doing, and a command it is running is
content. The cost is a message a human cannot act on; the alternative is a credential in a
terminal's scrollback and in whatever that terminal logs, which no later redaction reaches.

## 5. Identity: `app=sindri`, the workspace's name, and a root record only

`app=sindri` — one name, the binary's, and never a version or a vendor prefix, since a terminal
keying a root record by the application that made it wants the application and not the release.

`title` is the workspace's own name: the directory a run stands in, spelled as the rail spells
it, so a human with four terminals open reads which repository each is working in. It is a name
and never a path, because a title is a label a terminal may put in a window border, and a
window border has no room for `/home/glemsom/Documents/git/sindri`.

And the root record only, with no `id=`. Record ids are for a program with a task tree: one
process, several concurrent tasks, each with a line under the same application. Sindri runs one
agent, and several sindri instances are several ptys, which are several terminals, which are
several root records. An id scheme would need a naming rule for something that already has a
name — the pty — and an id no terminal can key separately buys nothing for the rule it costs.
This paragraph is here so that nobody adds one later without reading why it was left out.

## 6. One word that collides, and the names the code takes

This glossary's **record** is the files a session leaves. The specification's *record* is what
a terminal stores from a report. `agent::Report` is taken as well: it is what one run did — the
answer, why it stopped, what it cost. So the code's names are `sindri_core::status` for the
module and `sindri_core::status::State` for the five states, and in prose the emitted bytes are
a **status** and never a *report*: one module per word, so a `use` line says which sense a file
means without a reader holding two.

`GLOSSARY.md` carries the disambiguation line, in **record**, because that is where a reader
looking up the older word meets the newer one — and the entry for **status** carries the other
two senses, so the file that exists to stop drift is the file that stops this one.

## 7. A pure encoder in the core, and the writing left with the owner of the tty

`crates/core/src/status.rs`, the module `sindri_core::status`, is a pure encoder:
`status::report` returns the bytes and writes none. The core's own paragraph says it knows
nothing about a terminal, and a module inside it that wrote escapes would be the first thing to
break that — quietly, since nothing in the core's tests owns a tty. Both front ends depend on
the encoder, and neither should inherit the other's terminal.

The writing stays with whoever owns the tty, which is a different file in each front end:
`crates/tui/src/run.rs` for the frame, the one place that crate owns a terminal — raw mode, the
alternate screen, `crossterm::execute!` — and where `crates/tui/src/copy.rs` already shows the
shape, an OSC 52 copy written straight to the stdout the frame drew on;
`crates/cli/src/batch.rs` for batch mode, on stderr and only when stderr is a terminal, because
stdout is the answer and a script parsing it must not find an escape in it, which is
`crates/cli/src/notice.rs`'s rule for the front ends whose stdout is a contract. A front end
whose channel is not a terminal writes nothing at all: an escape handed to a file is bytes in
the file, and bytes in a file are the one thing this feature must never add.

In the frame that means two places and no third. A report goes out after `terminal.draw` has
returned, from the loop, and never from inside a draw closure, or ratatui's diff repaints cells
around a sequence the emulator has already consumed; and `state=clear` goes out where the frame is
leaving, before `ratatui::restore()`, because `done` and `error` are meant to outlive a shell
prompt and the frame's own record is not. How often either may be said is the channel's and is
not left to a front end — `status::Channel`, whose gate is `status::Told`: a state the terminal
has already been told is not said again, and nothing is said twice inside a second — a report the
second withholds is one the loop is told it owes, so it is said late and never lost.

## 8. One switch, read where both front ends read the config

A report is a courtesy to a human, and a human may not want it: a terminal whose own title bar
already names the repository, a capture a report would only litter, a machine whose scrollback is a
log somebody greps. So there is one setting that silences every report, and it is the config's own
— `status`, `SINDRI_STATUS` above the file, `true` unless either says otherwise. It is offered in
`/settings` like every other key of the schema, and each front end reads it where it stands: batch
mode's `Reports` takes it as the switch on its channel, and the frame carries it on its `Launch`,
where a run's own facts already are, so a `Channel` the switch silences holds no gate and says
nothing and clears nothing.

Not a switch per front end. Batch mode's `-b` and the frame's bare invocation would then spell one
decision twice, and the day they disagree is the day a user who silenced the reports finds one of
them still talking. There is no flag either: a flag would be a third spelling, and the precedence
the config already states — file, then environment, then flags — is a rule about the values a run
cannot ask twice, not a place to put a courtesy.

The setting does not decide the channel: stderr that is not a terminal is written nothing at all,
whatever the setting says (§7), because the setting is a human's preference about what they see and
a file is not a human.

## The alternative that lost

Leave the terminal alone, since the frame already draws a **pulse**: a braille marker at the
head of the newest answer with the run's elapsed time, settling on `✓` or `✗`.

The pulse is what a human watching sees, and it is the whole of the case against this record
until the human is somewhere else — which is the case the run is long enough to create, and the
one the specification was written for. A pulse is drawn inside the frame: it says nothing once
the terminal has no window in front of it, nothing when the frame has left the alternate
screen, and nothing at all in batch mode, which draws no frame. A report costs one line to a
terminal that may have no window, and costs nothing when nobody is looking.

They answer different questions — what is on the screen, and what the program is doing — so
both stay, and neither stands in for the other. The pulse is the frame's business and stays
where it is; a report is the run's, which is why its bytes are made in the core.