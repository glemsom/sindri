//! The wire at the address it is asked, and on the route it serves: the one value a
//! run and a listing are both built from, so the names a listing serves are read from the endpoint
//! a run asks.

use std::fmt;

use rig_core::completion::CompletionRequest;
use rig_core::error::ProviderError;
use rig_core::http_client::DynHttpClient;
use rig_core::model::ModelList;
use rig_core::providers::anthropic::AnthropicConfig;
use rig_core::providers::openai::OpenAIConfig;
use rig_core::providers::openai::wire::OpenAiWire;

use super::copilot::{SignIn, Store};
use super::route::Route;
use super::{Answer, Client, Deltas, Failure};
use crate::config::Environment;
use crate::redact::SecretSet;
use crate::wire::Wire;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    wire: Wire,
    base_url: Option<String>,
    route: Option<Route>,
}

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

    /// rig's choice for the dialect and the served name, or the one the run names; `None` for a
    /// dialect that serves one endpoint.
    pub fn route_of(&self, model: &str) -> Option<Route> {
        match self.serving("") {
            Serving::Messages(_) => None,
            Serving::OpenAi(config) => {
                Some(Route::of(&OpenAiWire::new(config, model.to_owned())))
            }
        }
    }

    /// The sign-in this wire asks with: the copilot store when it can be read, and its own words
    /// when it cannot, because a store that will not open is no reason to refuse before a job is
    /// asked.
    pub fn sign_in(&self, environment: &impl Environment) -> Option<Result<Store, SignIn>> {
        self.wire.credential_variable().is_none().then(|| {
            Store::resolve(environment).map_err(|error| {
                SignIn::Unavailable(SecretSet::default().redact(format!(
                    "the copilot sign-in cannot be read: {error}"
                )))
            })
        })
    }

    /// The dialect's client: the one place a wire decides what carries it.
    pub(super) fn serving(&self, key: &str) -> Serving {
        match self.wire {
            Wire::Anthropic => Serving::Messages(self.at_base(AnthropicConfig::new(key))),
            Wire::Openai => Serving::OpenAi(self.dressed(OpenAIConfig::new(key))),
            Wire::Copilot => Serving::OpenAi(
                self.dressed(OpenAIConfig::with_key(
                    &rig_core::providers::copilot::DIALECT,
                    key,
                )),
            ),
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

pub(super) enum Serving {
    Messages(AnthropicConfig),
    OpenAi(OpenAIConfig),
}

impl Serving {
    /// The names this dialect serves, through the same client a run asks.
    pub(super) async fn catalogue(
        self,
        transport: DynHttpClient,
    ) -> Result<ModelList, ProviderError> {
        match self {
            Serving::Messages(config) => config.connect(transport).list_models().await,
            Serving::OpenAi(config) => config.connect(transport).list_models().await,
        }
    }

    /// The run's request carried on this dialect's client, through `client`.
    pub(super) fn carried(
        self,
        client: &Client,
        model: &str,
        request: CompletionRequest,
        deltas: &mut dyn Deltas,
        secrets: &SecretSet,
    ) -> Result<Answer, Failure> {
        let transport = client.transport.clone();
        match self {
            Serving::Messages(config) => client.drive(
                config.connect(transport).completion(model),
                request,
                deltas,
                secrets,
            ),
            Serving::OpenAi(config) => client.drive(
                config.connect(transport).completion(model),
                request,
                deltas,
                secrets,
            ),
        }
    }
}
