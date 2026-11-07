//! Which completion endpoint an OpenAI-shaped wire serves: `chat` for
//! `POST /chat/completions` and `responses` for `POST /responses`. The choice is
//! rig's, read off the wire it builds from the dialect and the served name, and
//! a run may name the other one when a route misbehaves (Appendix A10).
//!
//! Nothing here decides the route a second time. `OpenAiWire` is the one place
//! rig routes, so the route kept here is the route the request takes, and the
//! dialect's own habit — Copilot answers Chat Completions unless the identifier
//! asks for Responses — stays the library's knowledge rather than becoming a
//! table someone has to keep in step.
//!
//! A route is the word the config file holds and the session document keeps, so
//! a run that answered oddly says which endpoint answered it, and a reader of
//! the record never has to guess which of the two reply shapes arrived.

use std::fmt;

use rig_core::providers::openai::wire::OpenAiWire;

use super::Ask;
use crate::wire::Wire;

/// Which completion endpoint an ask is served by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Route {
    /// `POST /chat/completions`: the endpoint every OpenAI-shaped dialect serves.
    Chat,
    /// `POST /responses`: the flagship route for a name that demands it.
    Responses,
}

impl Route {
    /// The words the config file, the environment and the flag accept, in the
    /// order a listing offers them.
    pub const WORDS: [&'static str; 2] = ["chat", "responses"];

    /// The word for this route: what the config file holds and the record keeps.
    pub fn word(self) -> &'static str {
        match self {
            Route::Chat => "chat",
            Route::Responses => "responses",
        }
    }

    /// The route a word names, or `None` when the word is not one of the two.
    pub fn from_word(word: &str) -> Option<Route> {
        match word {
            "chat" => Some(Route::Chat),
            "responses" => Some(Route::Responses),
            _ => None,
        }
    }

    /// The route rig chose for `wire`, read off the very wire the request is
    /// encoded by rather than predicted from a table here.
    fn of(wire: &OpenAiWire) -> Route {
        match wire {
            OpenAiWire::Chat(_) => Route::Chat,
            OpenAiWire::Responses(_) => Route::Responses,
        }
    }

    /// rig's own route, for the one place that has to hand rig its type.
    pub(super) fn chosen(self) -> rig_core::providers::openai::Route {
        match self {
            Route::Chat => rig_core::providers::openai::Route::Chat,
            Route::Responses => rig_core::providers::openai::Route::Responses,
        }
    }
}

impl fmt::Display for Route {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.word())
    }
}

impl<'de> serde::Deserialize<'de> for Route {
    /// Reads the word a person wrote, refusing anything else by naming the two
    /// that exist, which is the one error §11 gives a bad value.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Route, D::Error> {
        let word = String::deserialize(deserializer)?;
        Route::from_word(&word).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "`{word}` is not a route; expected one of {}",
                Route::WORDS.join(", ")
            ))
        })
    }
}

impl Ask {
    /// The route this ask is sent on: rig's choice for the dialect and the
    /// served name, or the one the ask's override names, and `None` for a
    /// dialect that serves a single endpoint.
    ///
    /// Read here before anything is sent, so a run that dies on its first
    /// request still says which endpoint it was on. The wire is built with no
    /// credential because routing reads the dialect and the name and never the
    /// key, and nothing is sent to ask this question.
    pub fn route_of(&self) -> Option<Route> {
        match self.wire {
            Wire::Anthropic => None,
            Wire::Openai | Wire::Copilot => Some(Route::of(&OpenAiWire::new(
                self.endpoint(""),
                self.model.clone(),
            ))),
        }
    }
}
