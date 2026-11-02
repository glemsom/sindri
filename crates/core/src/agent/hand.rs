//! The `skill` tool's other half: a skill a human loads by hand. `/skill:name`
//! is the call the model would make, run from the composer, so the body is in
//! the conversation from here on without a turn being asked for — and claimed in
//! the conversation's own loaded set, because a body in the history is in force
//! whoever put it there.

use super::{Activated, Agent};
use crate::provider::{Call, MALFORMED, Message, ToolOutcome};

impl Agent {
    /// A skill a human asked for, loaded without a turn: the same call the
    /// model would make, run where the human asked for it, so the body is in
    /// the conversation from here on and the deduplication counted it. Nothing
    /// is sent to the provider — a skill in force is a fact about the next turn,
    /// not a question to ask now — and a repeat or a name no skill answers
    /// changes neither the conversation nor the record, because an exchange
    /// nobody made is not one the model should read.
    pub fn activate(&mut self, name: &str) -> Activated {
        self.by_hand += 1;
        let call = Call::new(
            format!("user_skill_{}", self.by_hand),
            "skill",
            Ok(serde_json::Map::from_iter([(
                "name".to_string(),
                serde_json::json!(name),
            )])),
        );
        let outcome = self.hand.call(&call, name, &self.loaded);
        let loaded = outcome.facts.outcome.as_deref() == Some("loaded");
        if loaded {
            if let Some(recorder) = &self.recorder {
                // Turn zero: a load by hand happens between turns, so it
                // belongs to no turn of a run.
                recorder.tool_call(0, &call);
                recorder.tool_result(0, &outcome);
            }
            // The exchange the model will read: the call it did not make, and
            // the result it did. A body in the history without the call that
            // asked for it is a conversation no provider will take.
            self.history.push(Message::Assistant {
                text: String::new(),
                calls: vec![call],
            });
            self.history
                .push(Message::ToolResults(vec![outcome.clone()]));
        }
        Activated {
            name: name.to_string(),
            problem: reason(&outcome),
            content: outcome.content,
            is_error: outcome.is_error,
            loaded,
            outcome: outcome.facts.outcome,
        }
    }
}

/// The reason a call no tool could answer was not run, read off the line the
/// result opens with: the prefix is `provider`'s, so a front end says the same
/// words the model would have read without cutting them out of the result.
fn reason(outcome: &ToolOutcome) -> Option<String> {
    outcome
        .content
        .strip_prefix(MALFORMED)
        .map(|reason| reason.trim_end().to_string())
}
