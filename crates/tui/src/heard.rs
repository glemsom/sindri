//! What the run says to the frame: a prompt started, a fragment streamed, a
//! command's line, a run ended. The frame is told; it never asks.

use std::time::Duration;

use sindri_core::agent::Event;
use sindri_core::exec::ToolResult;
use sindri_core::provider::{Asked, Usage};
use sindri_core::status::{Doing, State};

use crate::app::{App, Renew};
use crate::driver::{Finished, Job, Side, Stop};
use crate::entry::{Call, CallOver, Entry, Note, Prompt, Skill};
use crate::thought::Thought;

impl App {
    pub(crate) fn submit(&mut self) -> bool {
        if self.composer.is_blank() {
            return false;
        }
        // A run with no key cannot answer: sending the prompt would spend a
        // round trip to be told 401, so the draft stays and the frame says what
        // is missing instead.
        if let Some(missing) = self.missing.as_ref() {
            self.note_error(missing.refused());
            return true;
        }
        if self.running.is_some() {
            self.queue.push_back(self.composer.take());
        } else {
            self.sent = true;
        }
        true
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

    /// One thing the run said, and — always — something the frame shows: every
    /// arm below writes a row of the thread or a figure of the rail, and the
    /// doing word above them moves the report the terminal is owed.
    pub fn event(&mut self, event: Event) -> bool {
        // What the run is doing is the stream's own reading, made once here and
        // in no arm: the table is `Event::doing`'s, because a batch run reads
        // the same stream and neither front end may spell it again. The arms
        // below keep only what the *frame* does with the event.
        let (turn, doing) = event.doing();
        self.rail.turn(turn, self.now);
        self.doing = doing;
        // What has streamed is what the run has spent so far: the meters move on
        // it now, and the report overrules them when the run ends. Both streams
        // pay it, so it is paid once above the arms rather than once in each.
        if let Event::TextDelta { text, .. } | Event::ReasoningDelta { text, .. } = &event {
            self.rail.streamed(text.chars().count(), self.now);
        }
        match event {
            // An answer beginning, a call announced and the final reasoning all
            // end a thought, as a cancel and `/new` do: the pane's life is the
            // run's, and one door closes it so no arm forgets one.
            Event::TextDelta { turn, text } => {
                self.thought_over();
                self.append(turn, text);
            }
            Event::ReasoningDelta { turn, text } => {
                self.think(turn, text);
            }
            Event::Reasoning { turn, text } => {
                self.thought_done(turn, text);
            }
            Event::ToolCall { turn, asked } => {
                self.thought_over();
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
                        call.announced();
                    }
                    Asked::Bash { .. } => {}
                    // An `open` spawns nothing this process watches: the row
                    // prints the target and is over the moment it is announced,
                    // because there is no exit for the human to wait for.
                    Asked::Open { .. } => call.announced(),
                    Asked::Refused { .. } => {
                        call.over = Some(CallOver::Exited(0));
                        call.took = Some(Duration::ZERO);
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
        true
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
                call.announced();
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

    /// A fragment of the run's thinking: the pane is the one surface it is
    /// drawn on, so nothing of it is an entry of the thread. The first fragment
    /// makes the thought and the ones after it queue behind it, and no fragment
    /// moves a row of what has already been said — nor reaches the pane at the
    /// rate the provider sent it, because the thought is the buffer between the
    /// two and `Thought::drain` is the rate.
    fn think(&mut self, turn: u32, text: String) {
        match self.thought.as_mut() {
            Some(thought) if thought.turn == turn => thought.push(&text),
            _ => self.thought = Some(Thought::new(turn, text, self.now)),
        }
    }

    /// The final reasoning event: a live pane begins its fold, which is the one
    /// door that does not take the pane away at once — the fold is watched, and
    /// the scar is written when the pane is down to one row. A final event with
    /// no pane behind it, from a provider that sent no fragment, leaves the scar
    /// at once: a thought nobody saw has no fold to watch.
    fn thought_done(&mut self, turn: u32, text: String) {
        let live = matches!(&self.thought, Some(thought) if thought.turn == turn);
        if live {
            let now = self.now;
            if let Some(thought) = self.thought.as_mut() {
                // The event carries the whole reasoning of the turn, which is
                // what the scar opens: the deltas are that text arriving, never
                // a second text. What the queue still held is dropped where it
                // stood, so the pane folds on the tick it would have folded
                // anyway rather than holding the frame open to catch up.
                thought.close(text, now);
            }
            return;
        }
        self.thought = None;
        self.thread.push(Entry::Reasoning {
            turn,
            text,
            done: true,
            open: false,
            // No pane ever stood for it, so nothing counted its clock: the row
            // says it was reasoned and says no duration it cannot know.
            took: None,
        });
    }

    /// A thought is over at once: the pane goes, and what the thread holds of it
    /// is nothing. Every arm that ends one comes through here — a call, a text
    /// delta, a cancel and `/new` — so none of them forgets it and the pane
    /// cannot outlive the run's use for it. The final reasoning event does not
    /// come through here: its pane folds, and `thought_done` is that door.
    pub(crate) fn thought_over(&mut self) {
        // A pane taken away mid-fold is still a thought that finished, and the
        // scar it was folding into is owed: the fold is watched only where the
        // frame has the ticks to watch it.
        self.scar();
        self.thought = None;
    }

    fn append(&mut self, turn: u32, text: String) {
        let joins = matches!(
            self.thread.last(),
            Some(Entry::Answer {
                turn: open,
                done: false,
                ..
            }) if *open == turn
        );
        if joins {
            self.thread.edit_last(|entry| {
                if let Entry::Answer { text: streamed, .. } = entry {
                    streamed.push_str(&text);
                }
            });
            return;
        }
        self.close_answers(false);
        self.thread.push(Entry::Answer {
            turn,
            text,
            done: false,
            failed: false,
            took: None,
        });
    }

    /// One line of a call's output, and whether it reached a row: a line that
    /// arrived before the call that produced it is held for that call, and no
    /// entry of the frame holds it yet.
    pub fn output(&mut self, side: Side, text: &str) -> bool {
        // Output can arrive a moment before the call that produced it was
        // announced, because the two travel different threads. It is held for
        // that call and no entry has changed yet.
        let running = self
            .thread
            .last()
            .is_some_and(|entry| matches!(entry, Entry::Call(call) if call.running()));
        if !running {
            self.early.push((side, text.to_string()));
            return false;
        }
        // The line joined the newest entry, so nothing above it moved.
        self.thread.edit_last(|entry| {
            if let Entry::Call(call) = entry {
                call.push(side, text.to_string());
            }
        });
        true
    }

    /// One call's end, and whether a call was still standing to take it: a
    /// result for a call the frame holds no row of closes nothing.
    pub fn over(&mut self, result: Option<ToolResult>) -> bool {
        let ended = self
            .thread
            .position(|entry| matches!(entry, Entry::Call(call) if call.running()));
        if let Some(index) = ended {
            let now = self.now;
            self.thread.edit(index, |entry| {
                let Entry::Call(call) = entry else {
                    return;
                };
                // One result freezes a row's four facts about it, or the one
                // word a call stopped under carries instead.
                match &result {
                    Some(result) => call.ended(result, now),
                    None => call.cancelled(now),
                }
            });
            return true;
        }
        false
    }

    /// The run's end, which is always something the frame shows: the marker
    /// settles, the answers close and the clock stops.
    pub fn finished(&mut self, finished: Finished) -> bool {
        self.running = None;
        // A run that is over is not thinking, whatever door it left by: a
        // thought has no pane once there is nothing left to think it.
        self.thought_over();
        // A run that failed before it had a report is the one shape `Report`
        // cannot hold: the failure in the frame's own words, no turn of it
        // counted, and nothing spent.
        let (stopped, turns, usage) = match &finished.run {
            Ok(report) => (report.stopped, report.turns, report.usage.clone()),
            Err(_) => (Stop::Error, 0, Usage::default()),
        };
        // Why the run ended, in the word the terminal reads: `Stopped`'s own
        // mapping, so a cancelled run leaves the terminal `idle` and a cap or a
        // budget is a run that delivered an answer.
        self.settled = Some(State::settled(stopped));
        self.rail.settled(turns, &usage, stopped, self.now);
        // A line that never found the call it belonged to belongs to no
        // later call either.
        self.early.clear();
        self.close_answers(stopped == Stop::Error);
        if let Err(error) = finished.run {
            self.thread.push(Entry::Note(Note::error(error)));
        }
        if let Some(note) = finished.note {
            self.thread.push(Entry::Note(Note::error(note)));
        }
        if stopped == Stop::Cancelled {
            self.thread.push(Entry::Note(Note::info(
                "cancelled — the draft is kept, and a prompt typed now queues",
            )));
        }
        true
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
                Some(Entry::Answer { done: false, .. })
            );
            if !open {
                continue;
            }
            self.thread.edit(index, |entry| {
                if let Entry::Answer {
                    done,
                    failed: bad,
                    took,
                    ..
                } = entry
                {
                    *done = true;
                    *bad = failed;
                    *took = Some(now);
                }
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
