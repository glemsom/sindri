//! The agent: the turn loop and the one tool it drives. It emits events on a
//! channel and returns a report, so it holds no terminal concerns.

mod event;
mod report;
mod tool;

pub use event::Event;
pub use report::{Report, Stopped};
pub use tool::BashTool;

use std::sync::mpsc::Sender;
use std::time::Instant;

use crate::provider::{Error, Message, Model};

/// Drives the model until it stops asking for tools, or the turn cap is hit.
pub struct Agent {
    model: Box<dyn Model>,
    tool: BashTool,
    max_turns: u32,
    thinking: bool,
    events: Option<Sender<Event>>,
}

impl Agent {
    /// A loop that asks `model`, runs what it asks through `tool`, and stops
    /// after `max_turns` round trips. Reasoning is paid for only when
    /// `thinking` asks (ADR-0004).
    pub fn new(model: Box<dyn Model>, tool: BashTool, max_turns: u32, thinking: bool) -> Self {
        Agent {
            model,
            tool,
            max_turns,
            thinking,
            events: None,
        }
    }

    /// Where events go. Without a receiver the loop still runs; it simply says
    /// nothing, because nothing is listening.
    pub fn set_events(&mut self, events: Sender<Event>) {
        self.events = Some(events);
    }

    /// Runs one prompt to an answer, or reports why the model failed.
    pub fn run(&mut self, prompt: &str) -> Result<Report, Error> {
        let started = Instant::now();
        let mut history = vec![Message::User(prompt.to_string())];
        let mut report = Report {
            answer: String::new(),
            turns: 0,
            stopped: Stopped::Final,
            model: None,
            tool_calls: 0,
            usage: crate::provider::Usage::default(),
            duration: std::time::Duration::ZERO,
        };
        let tools = [self.tool.spec().clone()];

        loop {
            if report.turns >= self.max_turns {
                report.stopped = Stopped::MaxTurns;
                break;
            }
            report.turns += 1;
            let turn = self.model.turn(&history, &tools, self.thinking)?;
            report.usage.accumulate(&turn.usage);
            if let Some(model) = turn.model {
                report.model.get_or_insert(model);
            }
            self.reasoning(report.turns, &turn.reasoning);
            if !turn.text.trim().is_empty() {
                report.answer.clone_from(&turn.text);
            }
            history.push(Message::Assistant {
                text: turn.text,
                calls: turn.calls.clone(),
            });
            if turn.calls.is_empty() {
                break;
            }
            let mut outcomes = Vec::with_capacity(turn.calls.len());
            for call in &turn.calls {
                self.announce(report.turns, call);
                outcomes.push(self.tool.call(call));
                report.tool_calls += 1;
            }
            history.push(Message::ToolResults(outcomes));
        }

        report.duration = started.elapsed();
        Ok(report)
    }

    /// Says one turn's reasoning, when there is any to say.
    fn reasoning(&self, turn: u32, reasoning: &Option<String>) {
        let Some(text) = reasoning.as_ref().filter(|text| !text.trim().is_empty()) else {
            return;
        };
        self.emit(Event::Reasoning {
            turn,
            text: text.clone(),
        });
    }

    /// Says one call before it runs, so a human watching sees the script even
    /// when it fails.
    fn announce(&self, turn: u32, call: &crate::provider::Call) {
        self.emit(Event::ToolCall {
            turn,
            command: call.command().ok(),
        });
    }

    /// Best effort: a front end that stopped listening is not a failed run.
    fn emit(&self, event: Event) {
        if let Some(events) = &self.events {
            let _ = events.send(event);
        }
    }
}
