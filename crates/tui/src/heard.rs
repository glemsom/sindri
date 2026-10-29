//! What the run says to the frame: a prompt started, a fragment streamed, a
//! command's line, a run ended. The frame is told; it never asks (ADR-0011 §1).

use sindri_core::agent::Event;
use sindri_core::cancel::Cancel;

use crate::app::App;
use crate::driver::{CallOver, Finished, Job, Side, Spilled, Stop};
use crate::entry::{Call, Entry, Note};

impl App {
    /// Enter: send now, or queue when a run is in flight.
    pub(crate) fn submit(&mut self) {
        if self.composer.is_blank() {
            return;
        }
        if self.running.is_some() {
            self.queue.push_back(self.composer.take());
        } else {
            self.sent = true;
        }
    }

    /// Starts the next prompt — the queue first, then a draft Enter sent —
    /// when nothing is running.
    pub fn start(&mut self) -> Option<Job> {
        if self.running.is_some() {
            return None;
        }
        let prompt = match self.queue.pop_front() {
            Some(queued) => queued,
            None if !self.sent || self.composer.is_blank() => return None,
            None => self.composer.take(),
        };
        self.sent = false;
        let cancel = Cancel::new();
        self.running = Some(cancel.clone());
        self.entries.push(Entry::Prompt(prompt.clone()));
        self.dirty();
        Some(Job { prompt, cancel })
    }

    /// One fragment of the run.
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

    /// One fragment of a streamed turn, appended to the entry it belongs to or
    /// opening a new one: text and reasoning differ only in which entry.
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

    /// One line of a running command's output, with the stream it came from.
    pub fn output(&mut self, side: Side, text: &str) {
        match self.entries.last_mut() {
            Some(Entry::Call(call)) if call.running() => call.push(side, text.to_string()),
            // Output can arrive a moment before the call that produced it was
            // announced, because the two travel different threads.
            _ => self.early.push((side, text.to_string())),
        }
        self.dirty();
    }

    /// How one command ended, and where the rest of its output went.
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

    /// The run is over: the rail settles and a failure is said where it
    /// happened (ADR-0011 §15).
    pub fn finished(&mut self, finished: Finished) {
        self.running = None;
        self.rail
            .settled(finished.turns, finished.tokens, finished.stopped);
        self.close_answers();
        if let Some(error) = finished.error {
            self.entries.push(Entry::Note(Note::error(error)));
        }
        if finished.stopped == Stop::Cancelled {
            self.entries.push(Entry::Note(Note::info(
                "cancelled — the draft is kept, and a prompt typed now queues",
            )));
        }
        self.dirty();
    }

    /// An answer whose turn has ended is folded away.
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
