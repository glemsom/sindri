//! The wire is a provider dialect and its endpoint: anthropic, openai, copilot. What a
//! wire says it serves is read off it, and kept beside the pair that produced it, never
//! invented here.

use std::fmt;

use serde::Deserialize;
use serde::Serialize;
use serde::de::Error as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Wire {
    Anthropic,
    Openai,
    Copilot,
}

impl Wire {
    pub const WORDS: [&'static str; 3] = ["anthropic", "openai", "copilot"];

    pub fn word(self) -> &'static str {
        match self {
            Wire::Anthropic => "anthropic",
            Wire::Openai => "openai",
            Wire::Copilot => "copilot",
        }
    }

    /// The variable this dialect's own key is read from, or `None`: a Copilot session is
    /// never an API key, it is a sign-in.
    pub fn credential_variable(self) -> Option<&'static str> {
        match self {
            Wire::Anthropic => Some("ANTHROPIC_API_KEY"),
            Wire::Openai => Some("OPENAI_API_KEY"),
            Wire::Copilot => None,
        }
    }

    pub fn from_word(word: &str) -> Option<Wire> {
        match word {
            "anthropic" => Some(Wire::Anthropic),
            "openai" => Some(Wire::Openai),
            "copilot" => Some(Wire::Copilot),
            _ => None,
        }
    }
}

impl fmt::Display for Wire {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.word())
    }
}

impl<'de> Deserialize<'de> for Wire {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Wire, D::Error> {
        let word = String::deserialize(deserializer)?;
        Wire::from_word(&word).ok_or_else(|| {
            D::Error::custom(format!(
                "`{word}` is not a wire; expected one of {}",
                Wire::WORDS.join(", ")
            ))
        })
    }
}
