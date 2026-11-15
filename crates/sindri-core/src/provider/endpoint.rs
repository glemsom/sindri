//! The wire at the address it is asked and on the route it serves. Which client carries it is
//! decided here and nowhere else, so a fourth wire is one arm of one match, and a listing cannot
//! learn what a run asks from a client of its own.

use std::fmt;

use rig_core::error::ProviderError;
use rig_core::http_client::DynHttpClient;
use rig_core::model::ModelList;
use rig_core::providers::anthropic::AnthropicConfig;
use rig_core::providers::openai::OpenAIConfig;
use rig_core::providers::openai::wire::OpenAiWire;

use super::route::Route;
use crate::wire::Wire;

#[derive(Clone, Debug)]
pub struct Endpoint {
    wire: Wire,
    base_url: Option<String>,
    route: Option<Route>,
}

/// Two endpoints are the same when they name one wire at one address: the route is how a run asks
/// it, and cannot change which names a wire serves.
impl PartialEq for Endpoint {
    fn eq(&self, other: &Endpoint) -> bool {
        self.wire == other.wire && self.base_url == other.base_url
    }
}

impl Eq for Endpoint {}

impl Endpoint {
    pub fn own(wire: Wire) -> Endpoint {
        Endpoint {
            wire,
            base_url: None,
            route: None,
        }
    }

    pub fn at(wire: Wire, base_url: impl Into<String>) -> Endpoint {
        Endpoint::own(wire).based(base_url)
    }

    pub fn based(mut self, base_url: impl Into<String>) -> Endpoint {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn routed(mut self, route: Option<Route>) -> Endpoint {
        self.route = route;
        self
    }

    pub fn wire(&self) -> Wire {
        self.wire
    }

    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    /// rig reads the route off the served name unless the run named one, and a dialect that serves
    /// one endpoint names none.
    pub fn route_of(&self, model: &str) -> Option<Route> {
        // The client is built with no key: routing reads the served name, never a credential.
        match self.serving("") {
            Serving::Messages(_) => None,
            Serving::OpenAi(config) => Some(Route::of(&OpenAiWire::new(config, model.to_owned()))),
        }
    }

    /// The dialect's client: the one place a wire decides what carries it.
    pub(super) fn serving(&self, key: &str) -> Serving {
        match self.wire {
            Wire::Anthropic => Serving::Messages(self.at_base(AnthropicConfig::new(key))),
            Wire::Openai => Serving::OpenAi(self.dressed(OpenAIConfig::new(key))),
            Wire::Copilot => Serving::OpenAi(self.dressed(OpenAIConfig::with_key(
                &rig_core::providers::copilot::DIALECT,
                key,
            ))),
        }
    }

    fn at_base(&self, config: AnthropicConfig) -> AnthropicConfig {
        match &self.base_url {
            Some(base_url) => config.with_base_url(base_url),
            None => config,
        }
    }

    fn dressed(&self, config: OpenAIConfig) -> OpenAIConfig {
        let mut config = match &self.base_url {
            Some(base_url) => config.with_base_url(base_url),
            None => config,
        };
        if let Some(route) = self.route {
            config = config.with_route(route.chosen());
        }
        config
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.base_url {
            Some(base_url) => write!(formatter, "{} at {base_url}", self.wire.word()),
            None => formatter.write_str(self.wire.word()),
        }
    }
}

// rig's two configurations are far apart in size and one of each is built per job: the box clippy
// asks for is an allocation bought with nothing.
#[allow(clippy::large_enum_variant)]
pub(super) enum Serving {
    Messages(AnthropicConfig),
    OpenAi(OpenAIConfig),
}

impl Serving {
    /// The names this dialect serves, through the same client a run asks with.
    #[allow(clippy::result_large_err)]
    pub(super) async fn names(self, transport: DynHttpClient) -> Result<ModelList, ProviderError> {
        match self {
            Serving::Messages(config) => config.connect(transport).list_models().await,
            Serving::OpenAi(config) => config.connect(transport).list_models().await,
        }
    }
}
