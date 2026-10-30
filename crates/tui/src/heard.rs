//! What the run says to the frame: a prompt started, a fragment streamed, a
//! command's line, a run ended. The frame is told; it never asks (ADR-0011).

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
        // is missing instead (ADR-0004).
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
        // One flag per launch, put down before every run: a cancel ends the
        // run it was raised for and never the next (ADR-0015).
        self.cancel.clear();
        let cancel = self.cancel.clone();
        self.running = Some(cancel.clone());
        self.entries.push(Entry::Prompt(Prompt {
            text: prompt.clone(),
            at: crate::clock::now(),
            cwd: self.rail.workspace(),
        }));
        self.dirty();
        Some(Job { prompt, cancel })
    }

    pub fn event(&mut self, event: Event) {
        match event {
            Event::TextDelta { turn, text } => {
                self.rail.turn(turn);
                self.append(turn, text, false);
            }
            Event::ReasoningDelta { turn, text } => {
                self.rail.turn(turn);
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
                });
                self.dirty();
            }
            Event::ToolCall { turn, command } => {
                self.rail.turn(turn);
                self.close_answers();
                let mut call = Call::new(turn, command);
                for (side, line) in self.early.drain(..) {
                    call.push(side, line);
                }
                self.entries.push(Entry::Call(call));
                self.dirty();
            }
        }
    }

    fn append(&mut self, turn: u32, text: String, reasoning: bool) {
        let open = match self.entries.last_mut() {
            Some(Entry::Reasoning {
                turn: open,
                text: streamed,
                done: false,
            }) if reasoning && *open == turn => Some(streamed),
            Some(Entry::Answer {
                turn: open,
                text: streamed,
                done: false,
            }) if !reasoning && *open == turn => Some(streamed),
            _ => None,
        };
        if let Some(streamed) = open {
            streamed.push_str(&text);
            self.dirty();
            return;
        }
        self.close_answers();
        if reasoning {
            self.entries.push(Entry::Reasoning {
                turn,
                text,
                done: false,
            });
        } else {
            self.entries.push(Entry::Answer {
                turn,
                text,
                done: false,
            });
        }
        self.dirty();
    }

    pub fn output(&mut self, side: Side, text: &str) {
        match self.entries.last_mut() {
            Some(Entry::Call(call)) if call.running() => call.push(side, text.to_string()),
            // Output can arrive a moment before the call that produced it was
            // announced, because the two travel different threads.
            _ => self.early.push((side, text.to_string())),
        }
        self.dirty();
    }

    pub fn over(&mut self, over: CallOver, spilled: Spilled) {
        let last = self.entries.iter_mut().rev().find_map(|entry| match entry {
            Entry::Call(call) if call.running() => Some(call),
            _ => None,
        });
        if let Some(call) = last {
            call.over = Some(over);
            call.truncated = spilled.truncated;
            call.spill = spilled.files;
        }
        self.dirty();
    }

    pub fn finished(&mut self, finished: Finished) {
        self.running = None;
        self.rail
            .settled(finished.turns, finished.tokens, finished.stopped);
        // A line that never found the call it belonged to belongs to no
        // later call either.
        self.early.clear();
        self.close_answers();
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

    fn close_answers(&mut self) {
        for entry in &mut self.entries {
            match entry {
                Entry::Answer { done, .. } | Entry::Reasoning { done, .. } => *done = true,
                _ => {}
            }
        }
        self.dirty();
    }
}
