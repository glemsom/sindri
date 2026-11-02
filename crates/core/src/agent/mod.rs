//! The agent: the turn loop and the tools it drives. It emits events on a
//! channel and returns a report, so it holds no terminal concerns, and it owns
//! the conversation itself: the messages the model was shown, kept across
//! prompts so a second question builds on the first answer, together with the
//! skills those messages hold. One launch is one thread; `/new` forgets it.

mod budget;
mod calls;
mod event;
mod hand;
mod limits;
mod open;
mod report;
mod skill;
mod tool;
mod tools;

pub use budget::{Budget, Spent};
pub use event::Event;
pub use limits::Limits;
pub use open::{OpenTool, Opened, Opener};
pub use report::{Report, Stopped};
pub use skill::{Activated, Loaded, SkillTool, UserSkills};
pub use tool::BashTool;
pub use tools::Tools;

use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Instant;

use crate::cancel::Cancel;
use crate::provider::{Asked, Call, Delta, DeltaSink, Error, Message, Model, ToolOutcome};
use crate::record::{Cost, Recorder};

/// What one conversation runs on: the model to ask, the tool to run it with,
/// how many turns it may take, and whether it pays for reasoning. A front end
/// replaces the whole set between prompts, because a saved setting can change
/// any of them and none of them is the conversation.
pub struct Dials {
    pub model: Box<dyn Model>,
    pub tool: Tools,
    pub max_turns: u32,
    pub thinking: bool,
}

pub struct Agent {
    model: Box<dyn Model>,
    tool: Tools,
    max_turns: u32,
    /// The bound on what the commands may spend, which a batched script can
    /// reach where the turn cap cannot see it.
    budget: Budget,
    thinking: bool,
    events: Option<Sender<Event>>,
    cancel: Cancel,
    recorder: Option<Arc<Recorder>>,
    /// The skills this conversation has been told, cleared with the history:
    /// a body in the transcript is in force, and one in a forgotten transcript
    /// is not.
    loaded: Arc<Loaded>,
    history: Vec<Message>,
    /// How many skills a human has loaded by hand, so each of those calls has
    /// an id of its own and two loads in one conversation cannot share one.
    by_hand: u32,
    /// The skills a human may reach for: the half of the catalog the model is
    /// never handed, which is what a load by hand is read from.
    hand: UserSkills,
}

impl Agent {
    pub fn new(model: Box<dyn Model>, tool: Tools, max_turns: u32, thinking: bool) -> Self {
        Agent {
            model,
            tool,
            max_turns,
            budget: Budget::default(),
            thinking,
            events: None,
            cancel: Cancel::new(),
            recorder: None,
            loaded: Arc::new(Loaded::default()),
            history: Vec::new(),
            by_hand: 0,
            hand: UserSkills::default(),
        }
    }

    /// The dials this conversation runs on, replaced between prompts: the
    /// thread is the conversation's, not the model's, so a saved setting is a
    /// change of dials and not a change of subject.
    pub fn set_dials(&mut self, dials: Dials) {
        self.model = dials.model;
        self.tool = dials.tool;
        self.max_turns = dials.max_turns;
        self.thinking = dials.thinking;
    }

    /// The bound this conversation runs under, replaced only by a test or a
    /// front end that has measured what a task may cost.
    pub fn set_budget(&mut self, budget: Budget) {
        self.budget = budget;
    }

    /// The skills a human may reach for, handed in by the boot that read the
    /// catalog: the agent never scans a tree of its own.
    pub fn set_hand(&mut self, hand: UserSkills) {
        self.hand = hand;
    }

    /// A new history, and no skill in force: what `/new` promises.
    pub fn forget(&mut self) {
        self.history.clear();
        self.loaded.clear();
    }

    /// The skills this conversation has been told, in name order: what a frame
    /// says is in force, read from the set the tool wrote.
    pub fn loaded_skills(&self) -> Vec<String> {
        self.loaded.names()
    }

    pub fn set_recorder(&mut self, recorder: Arc<Recorder>) {
        self.recorder = Some(recorder);
    }

    pub fn set_events(&mut self, events: Sender<Event>) {
        self.events = Some(events);
    }

    /// Stops saying anything: dropping the channel is how the thread reading it
    /// learns the run it was carrying is over. The conversation stays.
    pub fn close_events(&mut self) {
        self.events = None;
    }

    /// The flag that stops the run dead. The executor must share it, or a
    /// cancel would only be noticed once the command had finished on its own.
    pub fn set_cancel(&mut self, cancel: Cancel) {
        self.cancel = cancel;
    }

    pub fn run(&mut self, prompt: &str) -> Result<Report, Error> {
        let started = Instant::now();
        if let Some(recorder) = &self.recorder {
            recorder.user(prompt);
        }
        self.history.push(Message::User(prompt.to_string()));
        let mut report = Report {
            answer: String::new(),
            turns: 0,
            stopped: Stopped::Final,
            model: None,
            tool_calls: 0,
            spent: Spent::default(),
            usage: crate::provider::Usage::default(),
            duration: std::time::Duration::ZERO,
        };
        let tools = self.tool.specs();

        loop {
            if self.cancel.is_triggered() {
                report.stopped = Stopped::Cancelled;
                break;
            }
            if report.turns >= self.max_turns {
                report.stopped = Stopped::MaxTurns;
                break;
            }
            if self.budget.reached(report.spent) {
                report.stopped = Stopped::Budget;
                break;
            }
            report.turns += 1;
            // The turn's own clock, from the first byte of the request to the
            // reply: it is written with the reply, before the calls the reply
            // asks for have run, because a run killed mid-command must still
            // hold what the model said. Each call carries its own duration.
            let turn_started = Instant::now();
            let mut sink = StreamSink {
                events: self.events.as_ref(),
                cancel: &self.cancel,
                turn: report.turns,
            };
            let turn = match self
                .model
                .turn(&self.history, &tools, self.thinking, &mut sink)
            {
                Ok(turn) => turn,
                // A request cut short by a cancel is a cancellation, not a
                // provider failure: the shell must see 130, not 1.
                Err(_) if self.cancel.is_triggered() => {
                    report.stopped = Stopped::Cancelled;
                    break;
                }
                Err(error) => return Err(error),
            };
            report.usage.accumulate(&turn.usage);
            if let Some(model) = turn.model {
                report.model.get_or_insert(model);
            }
            self.reasoning(report.turns, &turn.reasoning);
            if let Some(recorder) = &self.recorder {
                recorder.assistant(
                    report.turns,
                    &turn.text,
                    turn.reasoning.as_deref(),
                    Cost::measured(turn.usage.clone(), turn_started.elapsed()),
                );
            }
            if !turn.text.trim().is_empty() {
                report.answer.clone_from(&turn.text);
            }
            self.history.push(Message::Assistant {
                text: turn.text,
                calls: turn.calls.clone(),
            });
            if turn.calls.is_empty() {
                break;
            }
            for call in &turn.calls {
                self.announce(report.turns, call);
                if let Some(recorder) = &self.recorder {
                    recorder.tool_call(report.turns, call);
                }
            }
            let mut outcomes = Vec::with_capacity(turn.calls.len());
            let mut stopped = None;
            for wave in turn.calls.chunks(calls::PARALLEL_CALLS) {
                if self.cancel.is_triggered() {
                    stopped = Some(Stopped::Cancelled);
                    break;
                }
                if self.budget.reached(report.spent) {
                    stopped = Some(Stopped::Budget);
                    break;
                }
                let mut killed = false;
                let results = calls::wave(&self.tool, &self.loaded, wave);
                for (call, outcome) in wave.iter().zip(results) {
                    // Counted before it is read: a call the cancel killed was
                    // started, and the record says so even with no result.
                    report.tool_calls += 1;
                    let Some(outcome) = outcome else {
                        killed = true;
                        break;
                    };
                    if let Some(recorder) = &self.recorder {
                        recorder.tool_result(report.turns, &outcome);
                    }
                    self.loaded_skill(report.turns, call, &outcome);
                    report.spent.add(
                        outcome.facts.duration_ms.unwrap_or_default(),
                        outcome.content.len(),
                    );
                    outcomes.push(outcome);
                }
                if killed {
                    stopped = Some(Stopped::Cancelled);
                    break;
                }
            }
            if let Some(stopped) = stopped {
                report.stopped = stopped;
                break;
            }
            self.history.push(Message::ToolResults(outcomes));
        }

        report.duration = started.elapsed();
        self.settle();
        Ok(report)
    }

    /// Ends the thread where a model turn ended: a prompt cut short by a cap, a
    /// cancel or a failure would otherwise leave an exchange with no reply, and
    /// the next request would carry a question nobody answered.
    fn settle(&mut self) {
        let answered = self
            .history
            .iter()
            .rposition(
                |message| matches!(message, Message::Assistant { calls, .. } if calls.is_empty()),
            )
            .map_or(0, |last| last + 1);
        self.history.truncate(answered);
    }

    fn reasoning(&self, turn: u32, reasoning: &Option<String>) {
        let Some(text) = reasoning.as_ref().filter(|text| !text.trim().is_empty()) else {
            return;
        };
        self.emit(Event::Reasoning {
            turn,
            text: text.clone(),
        });
    }

    /// A call that put a skill in force says so once, with the result the
    /// conversation now carries: a frame's chip, a check's assertion and a
    /// record's column all read the load here rather than inferring it from a
    /// call's name.
    fn loaded_skill(&self, turn: u32, call: &Call, outcome: &ToolOutcome) {
        let Asked::Skill { name } = call.asked() else {
            return;
        };
        self.emit(Event::SkillLoaded {
            turn,
            name,
            outcome: outcome.facts.outcome.clone(),
            body: outcome.content.clone(),
        });
    }

    fn announce(&self, turn: u32, call: &Call) {
        self.emit(Event::ToolCall {
            turn,
            asked: call.asked(),
        });
    }

    fn emit(&self, event: Event) {
        if let Some(events) = &self.events {
            let _ = events.send(event);
        }
    }
}

/// The loop's side of a streamed turn: each fragment becomes an event at once,
/// and a raised cancel flag ends the stream rather than buffering the rest.
struct StreamSink<'a> {
    events: Option<&'a Sender<Event>>,
    cancel: &'a Cancel,
    turn: u32,
}

impl DeltaSink for StreamSink<'_> {
    fn delta(&mut self, delta: Delta) -> bool {
        if self.cancel.is_triggered() {
            return false;
        }
        let event = match delta {
            Delta::Text(text) => Event::TextDelta {
                turn: self.turn,
                text,
            },
            Delta::Reasoning(text) => Event::ReasoningDelta {
                turn: self.turn,
                text,
            },
        };
        if let Some(events) = self.events {
            let _ = events.send(event);
        }
        true
    }
}
