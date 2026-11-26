use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::provider::{Assistant, Responses};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub assistant: Assistant,
    pub turn: u32,
    pub prompt: String,
    pub text: String,
    pub calls: Vec<Call>,
    pub results: Vec<Acted>,
    pub reasoning: Option<String>,
    pub responses: Option<Responses>,
    pub usage: Usage,
}

impl Turn {
    pub fn answered(turn: u32, prompt: impl Into<String>, text: impl Into<String>) -> Turn {
        Turn {
            assistant: Assistant::default(),
            turn,
            prompt: prompt.into(),
            text: text.into(),
            calls: Vec::new(),
            results: Vec::new(),
            reasoning: None,
            responses: None,
            usage: Usage::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
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
