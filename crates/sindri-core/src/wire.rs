//! The wire is a provider dialect and its endpoint: anthropic, openai, copilot.
//! Served names are read off it and kept beside the pair that produced them.

use std::fmt;

use serde::Deserialize;
use serde::Serialize;
use serde::de::Error as _;

/// A provider dialect, from the closed set the parser accepts. A fourth wire
/// cannot be invented in the interface, because there is no word for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Wire {
    /// Anthropic Messages.
    Anthropic,
    /// OpenAI Chat Completions, and Responses where the name demands it.
    Openai,
    /// GitHub Copilot, behind its own OAuth token store.
    Copilot,
}

impl Wire {
    /// The words the parser accepts, in the order the provider row offers them.
    pub const WORDS: [&'static str; 3] = ["anthropic", "openai", "copilot"];

    /// The word for this dialect: what the config file holds and the row shows.
    pub fn word(self) -> &'static str {
        match self {
            Wire::Anthropic => "anthropic",
            Wire::Openai => "openai",
            Wire::Copilot => "copilot",
        }
    }

    /// The variable this dialect's own key is read from, for the dialect that has
    /// one: a Copilot session is never an API key, it is a sign-in (§10).
    pub fn credential_variable(self) -> Option<&'static str> {
        match self {
            Wire::Anthropic => Some("ANTHROPIC_API_KEY"),
            Wire::Openai => Some("OPENAI_API_KEY"),
            Wire::Copilot => None,
        }
    }

    /// The dialect a word names, or `None` when the word is not one of the three.
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
