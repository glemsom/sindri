use std::fmt;

use rig_core::providers::openai::wire::OpenAiWire;

use super::Ask;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Route {
    Chat,
    Responses,
}

impl Route {
    pub const WORDS: [&'static str; 2] = ["chat", "responses"];

    pub fn word(self) -> &'static str {
        match self {
            Route::Chat => "chat",
            Route::Responses => "responses",
        }
    }

    pub fn from_word(word: &str) -> Option<Route> {
        match word {
            "chat" => Some(Route::Chat),
            "responses" => Some(Route::Responses),
            _ => None,
        }
    }

    pub(super) fn of(wire: &OpenAiWire) -> Route {
        match wire {
            OpenAiWire::Chat(_) => Route::Chat,
            OpenAiWire::Responses(_) => Route::Responses,
        }
    }

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
    pub fn route_of(&self) -> Option<Route> {
        self.endpoint.route_of(&self.model)
    }
}
