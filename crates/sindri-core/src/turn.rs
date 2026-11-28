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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_compactions: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_compactions: Option<u64>,
}

impl Usage {
    pub fn is_empty(&self) -> bool {
        *self == Usage::default()
    }

    pub fn since(self, earlier: Self) -> Self {
        Self {
            input: self
                .input
                .map(|value| value.saturating_sub(earlier.input.unwrap_or(0))),
            output: self
                .output
                .map(|value| value.saturating_sub(earlier.output.unwrap_or(0))),
            total: self
                .total
                .map(|value| value.saturating_sub(earlier.total.unwrap_or(0))),
            cached_input: self
                .cached_input
                .map(|value| value.saturating_sub(earlier.cached_input.unwrap_or(0))),
            reasoning: self
                .reasoning
                .map(|value| value.saturating_sub(earlier.reasoning.unwrap_or(0))),
            cache_read: self
                .cache_read
                .map(|value| value.saturating_sub(earlier.cache_read.unwrap_or(0))),
            cache_write: self
                .cache_write
                .map(|value| value.saturating_sub(earlier.cache_write.unwrap_or(0))),
            request_ms: self
                .request_ms
                .map(|value| value.saturating_sub(earlier.request_ms.unwrap_or(0))),
            client_compactions: self
                .client_compactions
                .map(|value| value.saturating_sub(earlier.client_compactions.unwrap_or(0))),
            native_compactions: self
                .native_compactions
                .map(|value| value.saturating_sub(earlier.native_compactions.unwrap_or(0))),
        }
    }

    pub fn charge(&mut self, spent: &Usage) {
        self.input = added(self.input, spent.input);
        self.output = added(self.output, spent.output);
        self.total = added(self.total, spent.total);
        self.cached_input = added(self.cached_input, spent.cached_input);
        self.reasoning = added(self.reasoning, spent.reasoning);
        self.cache_read = added(self.cache_read, spent.cache_read);
        self.cache_write = added(self.cache_write, spent.cache_write);
        self.request_ms = added(self.request_ms, spent.request_ms);
        self.client_compactions = added(self.client_compactions, spent.client_compactions);
        self.native_compactions = added(self.native_compactions, spent.native_compactions);
    }
}

fn added(total: Option<u64>, spent: Option<u64>) -> Option<u64> {
    match (total, spent) {
        (Some(total), Some(spent)) => Some(total.saturating_add(spent)),
        (Some(total), None) => Some(total),
        (None, spent) => spent,
    }
}
