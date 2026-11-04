//! The model a test scripts: the turns it answers in order, the conversations
//! it was shown, and the reply values a test composes them from.

#![allow(
    dead_code,
    reason = "each test binary uses a different slice of the fixtures"
)]

use std::sync::{Arc, Mutex};

use sindri_core::cancel::Cancel;
use sindri_core::provider::{Call, Delta, DeltaSink, Error, Message, Model, ToolSpec, Turn, Usage};

pub struct ScriptedModel {
    replies: Mutex<Vec<Turn>>,
    seen: Arc<Mutex<Vec<Vec<Message>>>>,
    pub thinking: Arc<Mutex<Vec<bool>>>,
    /// Raised while the first request is in flight, which then fails: a request
    /// cut short by Ctrl-C looks exactly like this to the loop.
    interrupt: Option<Cancel>,
    stream: bool,
}

impl ScriptedModel {
    pub fn new(
        replies: Vec<Turn>,
        seen: Arc<Mutex<Vec<Vec<Message>>>>,
        thinking: Arc<Mutex<Vec<bool>>>,
    ) -> Self {
        ScriptedModel {
            replies: Mutex::new(replies),
            seen,
            thinking,
            interrupt: None,
            stream: false,
        }
    }

    pub fn interrupting(mut self, interrupt: Option<Cancel>) -> Self {
        self.interrupt = interrupt;
        self
    }

    pub fn streaming(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }
}

fn fragmented(text: &str) -> Vec<String> {
    text.split_inclusive(' ').map(str::to_string).collect()
}

impl Model for ScriptedModel {
    /// No names, said as a failure rather than as an empty list: a double
    /// serves the turns it was handed, and the list a front end offers is a
    /// wire's own, which a double has none of.
    fn models(&self) -> Result<Vec<String>, Error> {
        Err(Error::transport("a scripted model names no models"))
    }

    fn turn(
        &self,
        history: &[Message],
        _tools: &[ToolSpec],
        thinking: bool,
        deltas: &mut dyn DeltaSink,
    ) -> Result<Turn, Error> {
        self.seen.lock().expect("seen").push(history.to_vec());
        self.thinking.lock().expect("thinking").push(thinking);
        if let Some(cancel) = &self.interrupt {
            cancel.trigger();
            return Err(Error::transport("the request was cut short"));
        }
        let mut replies = self.replies.lock().expect("replies");
        assert!(
            !replies.is_empty(),
            "the loop asked for more turns than the script holds"
        );
        let reply = replies.remove(0);
        if self.stream {
            let fragments = reply
                .reasoning
                .iter()
                .flat_map(|reason| fragmented(reason))
                .map(Delta::Reasoning)
                .chain(fragmented(&reply.text).into_iter().map(Delta::Text));
            for delta in fragments {
                assert!(deltas.delta(delta), "the sink stayed open");
            }
        }
        Ok(reply)
    }
}

pub fn usage(input: u64, output: u64) -> Usage {
    Usage {
        input_tokens: Some(input),
        output_tokens: Some(output),
        cached_input_tokens: None,
        reasoning_tokens: None,
        total_tokens: None,
    }
}

pub fn answer(text: &str) -> Turn {
    Turn {
        text: text.to_string(),
        reasoning: None,
        calls: Vec::new(),
        model: Some("claude-mock".to_string()),
        usage: usage(11, 7),
    }
}

pub fn call(id: &str, command: &str) -> Call {
    Call::new(
        id,
        "bash",
        Ok(serde_json::Map::from_iter([(
            "command".to_string(),
            serde_json::json!(command),
        )])),
    )
}

pub fn calls(calls: Vec<Call>) -> Turn {
    Turn {
        text: String::new(),
        reasoning: None,
        calls,
        model: Some("claude-mock".to_string()),
        usage: usage(20, 5),
    }
}
