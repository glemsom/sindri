//! One turn taken: the wire replays it and the run keeps it, so it lives below both.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// A turn already taken: the prompt it was asked with, the words it answered, each call it asked
/// for beside what that call answered, and the usage it spent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub turn: u32,
    pub prompt: String,
    pub text: String,
    pub calls: Vec<Call>,
    pub results: Vec<Acted>,
    pub usage: Usage,
}

impl Turn {
    /// A turn that asked for nothing: the words it was asked with and the words it answered.
    pub fn answered(turn: u32, prompt: impl Into<String>, text: impl Into<String>) -> Turn {
        Turn {
            turn,
            prompt: prompt.into(),
            text: text.into(),
            calls: Vec::new(),
            results: Vec::new(),
            usage: Usage::default(),
        }
    }
}

/// A call the model asked for: the wire's identity for it, the tool's name, and the arguments
/// as JSON text, so a call that does not parse still reaches the executor that judges it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    /// The wire's own identity for this call, which its result is answered against.
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl Call {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Call {
            id: id.into(),
            name: name.into(),
            arguments: arguments.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exit {
    Code(i32),
    Signalled,
    TimedOut,
    Cancelled,
}

impl Exit {
    pub fn word(self) -> String {
        match self {
            Exit::Code(code) => format!("exit {code}"),
            Exit::Signalled => String::from("killed by a signal"),
            Exit::TimedOut => String::from("timed out"),
            Exit::Cancelled => String::from("cancelled"),
        }
    }

    pub fn is_ok(self) -> bool {
        self == Exit::Code(0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acted {
    pub text: String,
    pub bytes: u64,
    pub elapsed: Duration,
    pub exit: Exit,
}

/// What one ask reported spending, and the running total a record keeps: the same five counts,
/// summed, so one turn's spend and a session's are one type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Usage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<u64>,
}

impl Usage {
    pub fn charge(&mut self, spent: &Usage) {
        self.input = added(self.input, spent.input);
        self.output = added(self.output, spent.output);
        self.total = added(self.total, spent.total);
        self.cached_input = added(self.cached_input, spent.cached_input);
        self.reasoning = added(self.reasoning, spent.reasoning);
    }
}

fn added(total: Option<u64>, spent: Option<u64>) -> Option<u64> {
    match (total, spent) {
        (Some(total), Some(spent)) => Some(total.saturating_add(spent)),
        (Some(total), None) => Some(total),
        (None, spent) => spent,
    }
}
