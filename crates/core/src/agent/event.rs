//! What the loop tells its front end while it runs. Events, not printing: the
//! core holds no terminal concerns, so a TUI renders this stream instead of
//! reimplementing the loop.

use crate::provider::Asked;
use crate::status::Doing;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    TextDelta {
        turn: u32,
        text: String,
    },
    ReasoningDelta {
        turn: u32,
        text: String,
    },
    Reasoning {
        turn: u32,
        text: String,
    },
    ToolCall {
        turn: u32,
        /// What the call asks for, as the loop read it: the script, the skill,
        /// or the reason nothing ran. The variant is the tool, so a frame that
        /// draws one call draws every kind of call.
        asked: Asked,
    },
    /// A skill is in force from here on, and the result the conversation now
    /// carries. The body comes with the event rather than being fetched by
    /// whoever draws it: a frame has no business reading a skill, and a load
    /// this event does not say is a load the frame would have to infer from a
    /// call's name.
    SkillLoaded {
        turn: u32,
        name: String,
        /// The word the result opened with — `loaded`, `already_loaded`,
        /// `unreadable` — so a frame says which of the three it was without
        /// reading the result. Absent where the call never reached the tool and
        /// was answered for it.
        outcome: Option<String>,
        /// The result as the model will read it on every turn from here:
        /// frontmatter stripped, in its named wrapper, with its directory and
        /// the files beside the body.
        body: String,
    },
}

impl Event {
    /// What this event says the run is doing, and in which turn: the stream read
    /// as `Doing`'s words, made here because both front ends read the same stream
    /// and neither may spell it again. A variant added to `Event` is a missing
    /// arm here, where the match is exhaustive, rather than a word a frame
    /// silently stops saying.
    pub fn doing(&self) -> (u32, Doing) {
        match self {
            Event::TextDelta { turn, .. } => (*turn, Doing::Answer),
            Event::ReasoningDelta { turn, .. } | Event::Reasoning { turn, .. } => {
                (*turn, Doing::Thinking)
            }
            Event::ToolCall { turn, asked } => (*turn, Doing::of(asked)),
            Event::SkillLoaded { turn, .. } => (*turn, Doing::Skill),
        }
    }
}
