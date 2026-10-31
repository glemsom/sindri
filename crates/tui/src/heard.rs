//! What the run says to the frame: a prompt started, a fragment streamed, a
//! command's line, a run ended. The frame is told; it never asks.

use std::time::Duration;

use sindri_core::agent::Event;

use crate::app::{App, Renew};
use crate::driver::{CallOver, Finished, Job, Side, Spilled, Stop};
use crate::entry::{Call, Entry, Note, Prompt};

impl App {
    pub(crate) fn submit(&mut self) {
        if self.composer.is_blank() {
            return;
        }
        // A run with no key cannot answer: sending the prompt would spend a
        // round trip to be told 401, so the draft stays and the frame says what
        // is missing instead.
        if let Some(missing) = self.missing {
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
        if self.running.is_some() || self.renew != Renew::Idle {
            return None;
        }
        let prompt = match self.queue.pop_front() {
            Some(queued) => queued,
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
        self.entries.push(Entry::Prompt(Prompt {
            text: prompt.clone(),
            at: sindri_core::clock::now(),
            cwd: self.rail.workspace(),
        }));
        self.dirty_tail();
        Some(Job { prompt, cancel })
    }

    pub fn event(&mut self, event: Event) {
        match event {
            Event::TextDelta { turn, text } => {
                self.rail.turn(turn, self.now);
                self.append(turn, text, false);
            }
            Event::ReasoningDelta { turn, text } => {
                self.rail.turn(turn, self.now);
                self.append(turn, text, true);
            }
            Event::Reasoning { turn, text } => {
                self.entries.retain(
                    |entry| !matches!(entry, Entry::Reasoning { turn: open, .. } if *open == turn),
                );
                self.entries.push(Entry::Reasoning {
                    turn,
                    text,
                    done: true,
                    open: false,
                });
                self.dirty();
            }
            Event::ToolCall { turn, command } => {
                self.rail.turn(turn, self.now);
                self.close_answers(false);
                let mut call = Call::new(turn, command, self.now);
                for (side, line) in self.early.drain(..) {
                    call.push(side, line);
                }
                self.entries.push(Entry::Call(call));
                self.dirty();
            }
        }
    }

    fn append(&mut self, turn: u32, text: String, reasoning: bool) {
        // What has streamed is what the run has spent so far: the meters move
        // on it now, and the report overrules them when the run ends.
        self.rail.streamed(text.chars().count(), self.now);
        let open = match self.entries.last_mut() {
            Some(Entry::Reasoning {
                turn: open,
                text: streamed,
                done: false,
                ..
            }) if reasoning && *open == turn => Some(streamed),
            Some(Entry::Answer {
                turn: open,
                text: streamed,
                done: false,
                ..
            }) if !reasoning && *open == turn => Some(streamed),
            _ => None,
        };
        if let Some(streamed) = open {
            streamed.push_str(&text);
            self.dirty_tail();
            return;
        }
        self.close_answers(false);
        if reasoning {
            self.entries.push(Entry::Reasoning {
                turn,
                text,
                done: false,
                open: false,
            });
        } else {
            self.entries.push(Entry::Answer {
                turn,
                text,
                done: false,
                failed: false,
                took: None,
            });
        }
        self.dirty_tail();
    }

    pub fn output(&mut self, side: Side, text: &str) {
        match self.entries.last_mut() {
            Some(Entry::Call(call)) if call.running() => {
                call.push(side, text.to_string());
                // The line joined the newest entry, so nothing above it moved.
                self.dirty_tail();
            }
            // Output can arrive a moment before the call that produced it was
            // announced, because the two travel different threads. It is held
            // for that call and no entry has changed yet.
            _ => self.early.push((side, text.to_string())),
        }
    }

    pub fn over(&mut self, over: CallOver, spilled: Spilled) {
        let ended = self
            .entries
            .iter()
            .rposition(|entry| matches!(entry, Entry::Call(call) if call.running()));
        if let Some(index) = ended
            && let Some(Entry::Call(call)) = self.entries.get_mut(index)
        {
            // The duration freezes where the call ended: a finished call is
            // not still counting.
            call.took = Some(self.now.saturating_sub(call.started));
            call.over = Some(over);
            call.truncated = spilled.truncated;
            call.spill = spilled.files;
            self.dirty_from(index);
        }
    }

    pub fn finished(&mut self, finished: Finished) {
        self.running = None;
        self.rail
            .settled(finished.turns, finished.tokens, finished.stopped, self.now);
        // A line that never found the call it belonged to belongs to no
        // later call either.
        self.early.clear();
        self.close_answers(finished.stopped == Stop::Error);
        if let Some(error) = finished.error {
            self.entries.push(Entry::Note(Note::error(error)));
        }
        if let Some(note) = finished.note {
            self.entries.push(Entry::Note(Note::error(note)));
        }
        if finished.stopped == Stop::Cancelled {
            self.entries.push(Entry::Note(Note::info(
                "cancelled — the draft is kept, and a prompt typed now queues",
            )));
        }
        self.dirty();
    }

    /// The newest answer is over — because a call followed it, or because the
    /// run did. Its duration freezes here, and it learns whether the run it
    /// belonged to failed.
    fn close_answers(&mut self, failed: bool) {
        let now = self.now;
        for entry in &mut self.entries {
            match entry {
                Entry::Answer {
                    done,
                    failed: bad,
                    took,
                    ..
                } if !*done => {
                    *done = true;
                    *bad = failed;
                    *took = Some(now);
                }
                Entry::Reasoning { done, .. } => *done = true,
                _ => {}
            }
        }
        self.dirty();
    }
}
