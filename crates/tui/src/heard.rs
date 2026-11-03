//! What the run says to the frame: a prompt started, a fragment streamed, a
//! command's line, a run ended. The frame is told; it never asks.

use std::time::Duration;

use sindri_core::agent::Event;
use sindri_core::provider::Asked;
use sindri_core::status::{Doing, State};

use crate::app::{App, Renew};
use crate::driver::{CallOver, Finished, Job, Side, Spilled, Stop};
use crate::entry::{Call, Entry, Note, Prompt, Skill};

impl App {
    pub(crate) fn submit(&mut self) {
        if self.composer.is_blank() {
            return;
        }
        // A run with no key cannot answer: sending the prompt would spend a
        // round trip to be told 401, so the draft stays and the frame says what
        // is missing instead.
        if let Some(missing) = self.missing.as_ref() {
            self.note_error(missing.refused());
            return;
        }
        if self.running.is_some() {
            self.queue.push_back(self.composer.take());
        } else {
            self.sent = true;
        }
    }

    pub fn start(&mut self) -> Option<Job> {
        if self.running.is_some() || self.renew != Renew::Idle || self.is_loading() {
            return None;
        }
        // What was queued runs first and in the order it was typed; a prompt the
        // reader wrote after a skill comes after it, once the load has landed.
        let prompt = match self.queue.pop_front().or_else(|| self.armed.take()) {
            Some(next) => next,
            None if !self.sent || self.composer.is_blank() => return None,
            None => self.composer.take(),
        };
        self.sent = false;
        // The clock starts with the run: a marker says how long *this* run has
        // been going, and a fresh prompt starts it again.
        self.now = Duration::ZERO;
        self.rail.begun();
        // One flag per launch, put down before every run: a cancel ends the
        // run it was raised for and never the next.
        self.cancel.clear();
        let cancel = self.cancel.clone();
        self.running = Some(cancel.clone());
        // A run begins thinking, and the frame has nothing settled to report:
        // the terminal is told what this run does, not what the last one did.
        self.doing = Doing::Thinking;
        self.settled = None;
        self.thread.push(Entry::Prompt(Prompt {
            text: prompt.clone(),
            at: sindri_core::clock::now(),
            cwd: self.rail.workspace(),
        }));
        Some(Job { prompt, cancel })
    }

    pub fn event(&mut self, event: Event) {
        // What the run is doing is the stream's own reading, made once here and
        // in no arm: the table is `Event::doing`'s, because a batch run reads
        // the same stream and neither front end may spell it again. The arms
        // below keep only what the *frame* does with the event.
        let (turn, doing) = event.doing();
        self.rail.turn(turn, self.now);
        self.doing = doing;
        match event {
            Event::TextDelta { turn, text } => {
                self.append(turn, text, false);
            }
            Event::ReasoningDelta { turn, text } => {
                self.append(turn, text, true);
            }
            Event::Reasoning { turn, text } => {
                self.thread.keep(
                    |entry| !matches!(entry, Entry::Reasoning { turn: open, .. } if *open == turn),
                );
                self.thread.push(Entry::Reasoning {
                    turn,
                    text,
                    done: true,
                    open: false,
                });
            }
            Event::ToolCall { turn, asked } => {
                self.close_answers(false);
                let mut call = Call::new(turn, requested(&asked), self.now);
                // A load runs no command, so there is no exit to wait for: its
                // row is over the moment it was announced, rather than counting
                // up until the next call's own end. What the row says a load
                // did arrives with the load's own event, together with the body
                // the reader can open.
                match &asked {
                    Asked::Skill { name } => {
                        call.skill = Some(Skill::new(name));
                        call.over = Some(CallOver::Exited(0));
                        call.took = Some(std::time::Duration::ZERO);
                    }
                    Asked::Bash { .. } => {}
                    // An `open` spawns nothing this process watches: the row
                    // prints the target and is over the moment it is announced,
                    // because there is no exit for the human to wait for.
                    Asked::Open { .. } => {
                        call.over = Some(CallOver::Announced);
                        call.took = Some(std::time::Duration::ZERO);
                    }
                    Asked::Refused { .. } => {
                        call.over = Some(CallOver::Exited(0));
                        call.took = Some(std::time::Duration::ZERO);
                    }
                }
                for (side, line) in self.early.drain(..) {
                    call.push(side, line);
                }
                self.thread.push(Entry::Call(call));
            }
            Event::SkillLoaded {
                turn,
                name,
                outcome,
                body,
            } => self.skill_loaded(turn, &name, outcome.as_deref(), &body),
        }
    }

    /// A load has been answered: the row that announced it takes the word and
    /// the text it read, and the rail says the skill is in force. A load no turn
    /// announced — one a human asked for, which no model call made — gets its
    /// row here, so every load is one row in the thread and never a silence.
    /// The body is kept behind that row: the thread says what is in force, and
    /// `Enter` is what opens the text.
    fn skill_loaded(&mut self, turn: u32, name: &str, outcome: Option<&str>, body: &str) {
        let announcing = self.thread.position(|entry| {
            matches!(entry, Entry::Call(call) if call
                .skill
                .as_ref()
                .is_some_and(|skill| skill.name == name && skill.outcome.is_none()))
        });
        let index = match announcing {
            Some(index) => index,
            None => {
                let mut call = Call::new(turn, Some(format!("skill {name}")), self.now);
                call.skill = Some(Skill::new(name));
                call.over = Some(CallOver::Exited(0));
                call.took = Some(std::time::Duration::ZERO);
                self.thread.push(Entry::Call(call));
                self.thread.len() - 1
            }
        };
        self.thread.edit(index, |entry| {
            let Entry::Call(call) = entry else {
                return;
            };
            if let Some(skill) = call.skill.as_mut() {
                skill.outcome = outcome.map(str::to_string);
            }
            for line in body.lines() {
                call.push(Side::Stdout, line.to_string());
            }
        });
        if outcome == Some("loaded") {
            self.rail.skill_loaded(name);
        }
    }

    fn append(&mut self, turn: u32, text: String, reasoning: bool) {
        // What has streamed is what the run has spent so far: the meters move
        // on it now, and the report overrules them when the run ends.
        self.rail.streamed(text.chars().count(), self.now);
        let joins = match self.thread.last() {
            Some(Entry::Reasoning {
                turn: open,
                done: false,
                ..
            }) => reasoning && *open == turn,
            Some(Entry::Answer {
                turn: open,
                done: false,
                ..
            }) => !reasoning && *open == turn,
            _ => false,
        };
        if joins {
            self.thread.edit_last(|entry| match entry {
                Entry::Reasoning { text: streamed, .. } | Entry::Answer { text: streamed, .. } => {
                    streamed.push_str(&text)
                }
                _ => {}
            });
            return;
        }
        self.close_answers(false);
        if reasoning {
            self.thread.push(Entry::Reasoning {
                turn,
                text,
                done: false,
                open: false,
            });
        } else {
            self.thread.push(Entry::Answer {
                turn,
                text,
                done: false,
                failed: false,
                took: None,
            });
        }
    }

    pub fn output(&mut self, side: Side, text: &str) {
        // Output can arrive a moment before the call that produced it was
        // announced, because the two travel different threads. It is held for
        // that call and no entry has changed yet.
        let running = self
            .thread
            .last()
            .is_some_and(|entry| matches!(entry, Entry::Call(call) if call.running()));
        if !running {
            self.early.push((side, text.to_string()));
            return;
        }
        // The line joined the newest entry, so nothing above it moved.
        self.thread.edit_last(|entry| {
            if let Entry::Call(call) = entry {
                call.push(side, text.to_string());
            }
        });
    }

    pub fn over(&mut self, over: CallOver, spilled: Spilled) {
        let ended = self
            .thread
            .position(|entry| matches!(entry, Entry::Call(call) if call.running()));
        if let Some(index) = ended {
            let now = self.now;
            self.thread.edit(index, |entry| {
                let Entry::Call(call) = entry else {
                    return;
                };
                // The duration freezes where the call ended: a finished call is
                // not still counting.
                call.took = Some(now.saturating_sub(call.started));
                call.over = Some(over);
                call.truncated = spilled.truncated;
                call.spill = spilled.files;
            });
        }
    }

    pub fn finished(&mut self, finished: Finished) {
        self.running = None;
        // Why the run ended, in the word the terminal reads: `Stopped`'s own
        // mapping, so a cancelled run leaves the terminal `idle` and a cap or a
        // budget is a run that delivered an answer.
        self.settled = Some(State::settled(finished.stopped));
        self.rail
            .settled(finished.turns, finished.tokens, finished.stopped, self.now);
        // A line that never found the call it belonged to belongs to no
        // later call either.
        self.early.clear();
        self.close_answers(finished.stopped == Stop::Error);
        if let Some(error) = finished.error {
            self.thread.push(Entry::Note(Note::error(error)));
        }
        if let Some(note) = finished.note {
            self.thread.push(Entry::Note(Note::error(note)));
        }
        if finished.stopped == Stop::Cancelled {
            self.thread.push(Entry::Note(Note::info(
                "cancelled — the draft is kept, and a prompt typed now queues",
            )));
        }
    }

    /// The newest answer is over — because a call followed it, or because the
    /// run did. Its duration freezes here, and it learns whether the run it
    /// belonged to failed.
    fn close_answers(&mut self, failed: bool) {
        let now = self.now;
        for index in 0..self.thread.len() {
            // Only the entry still open changed: the one already closed is
            // worth the rows it was, and forgetting it would cost a layout.
            let open = matches!(
                self.thread.get(index),
                Some(Entry::Answer { done: false, .. } | Entry::Reasoning { done: false, .. })
            );
            if !open {
                continue;
            }
            self.thread.edit(index, |entry| match entry {
                Entry::Answer {
                    done,
                    failed: bad,
                    took,
                    ..
                } => {
                    *done = true;
                    *bad = failed;
                    *took = Some(now);
                }
                Entry::Reasoning { done, .. } => *done = true,
                _ => {}
            });
        }
    }
}

/// What the call's row says it runs: the script, `skill <name>` for a load, the
/// target for an `open`, and nothing at all for a call no tool could answer,
/// which is a row with no command behind it.
fn requested(asked: &Asked) -> Option<String> {
    match asked {
        Asked::Bash { command } => Some(command.clone()),
        Asked::Skill { name } => Some(format!("skill {name}")),
        Asked::Open { target } => Some(target.to_string()),
        Asked::Refused { .. } => None,
    }
}
