//! The frame's state: the transcript, the composer, the queue and the rail. It
//! knows nothing of the model, the sandbox or the terminal: it is told what
//! happened and it knows how to show it (ADR-0011 §1).

use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sindri_core::agent::Event;
use sindri_core::cancel::Cancel;

use crate::composer::Composer;
use crate::driver::{CallOver, Finished, Job, Side, Stop};
use crate::entry::{Call, Entry, Note};
use crate::face::Face;
use crate::rail::{Launch, Rail};

/// Everything the frame draws, and everything it has been told.
pub struct App {
    rail: Rail,
    entries: Vec<Entry>,
    composer: Composer,
    /// Prompts typed while a run was in flight, in the order they were typed.
    queue: VecDeque<String>,
    /// The flag of the run in flight, if there is one.
    running: Option<Cancel>,
    /// Whether Enter has been pressed on the draft since it was last started:
    /// typing alone never launches a run.
    sent: bool,
    /// Output that arrived before the call it belongs to was announced.
    early: Vec<(Side, String)>,
    quit: bool,
    face: Option<Face>,
}

impl App {
    /// A frame that has run nothing yet, with the face when the terminal can
    /// show it.
    pub fn new(launch: Launch, face: Option<Face>) -> Self {
        App {
            rail: Rail::new(launch),
            entries: Vec::new(),
            composer: Composer::new(),
            queue: VecDeque::new(),
            running: None,
            sent: false,
            early: Vec::new(),
            quit: false,
            face,
        }
    }

    /// The transcript, in the order it happened.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The rail's facts.
    pub fn rail(&self) -> &Rail {
        &self.rail
    }

    /// The composer being typed into.
    pub fn composer(&self) -> &Composer {
        &self.composer
    }

    /// How many prompts are waiting for the run in flight to end.
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// Whether a run is in flight.
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Whether the user asked to leave.
    pub fn wants_quit(&self) -> bool {
        self.quit
    }

    /// The face, when the terminal can show one.
    pub fn face_mut(&mut self) -> Option<&mut Face> {
        self.face.as_mut()
    }

    /// One key. Enter sends or queues, Shift+Enter breaks the line, Ctrl-C
    /// cancels the run and keeps the draft, Ctrl-Q leaves (ADR-0011 §14).
    pub fn key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') if ctrl => self.quit = true,
            KeyCode::Char('c') if ctrl => self.cancel_or_clear(),
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.composer.newline()
            }
            KeyCode::Enter => self.submit(),
            KeyCode::Backspace => self.composer.backspace(),
            KeyCode::Left => self.composer.left(),
            KeyCode::Right => self.composer.right(),
            KeyCode::Up => self.composer.up(),
            KeyCode::Down => self.composer.down(),
            KeyCode::Char(ch) if !ctrl => self.composer.insert(ch),
            _ => {}
        }
    }

    /// Ctrl-C: the run stops and the draft stays; with nothing running, the
    /// draft is cleared.
    fn cancel_or_clear(&mut self) {
        match &self.running {
            Some(cancel) => cancel.trigger(),
            None => {
                self.composer.clear();
                self.sent = false;
            }
        }
    }

    /// Enter: send now, or queue when a run is in flight. With nothing running
    /// the draft stays where it is until the loop collects it, so a run always
    /// begins with the prompt Enter was pressed on.
    fn submit(&mut self) {
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
            }
            Event::ToolCall { turn, command } => {
                self.rail.turn(turn);
                self.close_answers();
                let mut call = Call::new(turn, command);
                for (side, line) in self.early.drain(..) {
                    call.push(side, line);
                }
                self.entries.push(Entry::Call(call));
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
    }

    /// One line of a running command's output, with the stream it came from.
    pub fn output(&mut self, side: Side, text: &str) {
        match self.entries.last_mut() {
            Some(Entry::Call(call)) if call.running() => call.push(side, text.to_string()),
            // Output can arrive a moment before the call that produced it was
            // announced, because the two travel different threads.
            _ => self.early.push((side, text.to_string())),
        }
    }

    /// How one command ended.
    pub fn over(&mut self, over: CallOver) {
        let last = self.entries.iter_mut().rev().find_map(|entry| match entry {
            Entry::Call(call) if call.running() => Some(call),
            _ => None,
        });
        if let Some(call) = last {
            call.over = Some(over);
        }
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
    }

    /// An answer whose turn has ended is folded away.
    fn close_answers(&mut self) {
        for entry in &mut self.entries {
            match entry {
                Entry::Answer { done, .. } | Entry::Reasoning { done, .. } => *done = true,
                _ => {}
            }
        }
    }
}
