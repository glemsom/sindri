use std::fmt;

use rig_core::model::ModelList;
use rig_core::providers::anthropic::AnthropicConfig;
use rig_core::providers::openai::OpenAIConfig;

use super::{Client, Credential, Failure, Verdict, credential_source};
use crate::wire::Wire;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Where {
    wire: Wire,
    base_url: Option<String>,
}

impl Where {
    pub fn own(wire: Wire) -> Self {
        Self {
            wire,
            base_url: None,
        }
    }

    pub fn at(wire: Wire, base_url: impl Into<String>) -> Self {
        Self {
            wire,
            base_url: Some(base_url.into()),
        }
    }

    pub fn wire(&self) -> Wire {
        self.wire
    }

    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }
}

impl fmt::Display for Where {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.base_url {
            Some(base_url) => write!(formatter, "{} at {base_url}", self.wire.word()),
            None => formatter.write_str(self.wire.word()),
        }
    }
}

pub trait Current {
    fn pair(&self) -> Where;
}

impl Current for Where {
    fn pair(&self) -> Where {
        self.clone()
    }
}

impl<F: Fn() -> Where> Current for F {
    fn pair(&self) -> Where {
        self()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Served {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    at: Where,
    outcome: Outcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Outcome {
    Listed(Vec<Served>),
    Silent(String),
}

impl Listing {
    /// An empty catalogue is an answer and reads as `Some`, which is how a wire that
    /// genuinely serves nothing is told apart from one that could not be asked.
    pub fn names(&self) -> Option<&[Served]> {
        match &self.outcome {
            Outcome::Listed(names) => Some(names),
            Outcome::Silent(_) => None,
        }
    }

    pub fn at(&self) -> &Where {
        &self.at
    }

    pub fn hint(&self) -> Option<&str> {
        match &self.outcome {
            Outcome::Silent(hint) => Some(hint),
            Outcome::Listed(_) => None,
        }
    }
}

impl Client {
    /// The answer is kept beside the pair it was asked of, and dropped when `current`
    /// names another pair by the time it lands: names read from one endpoint are not
    /// evidence about another.
    pub fn served(&self, at: &Where, credential: &Credential, current: &dyn Current) -> Listing {
        let read = match credential.expose() {
            Some(key) => self.catalogue(at, key),
            None => Err(Failure::unconfigured(at.wire())),
        };
        let outcome = if current.pair() == *at {
            match read {
                Ok(names) => Outcome::Listed(names),
                Err(failure) => Outcome::Silent(format!(
                    "{at} has no model list: {}; name a model and the run goes on",
                    reason(at, &failure)
                )),
            }
        } else {
            Outcome::Silent(format!(
                "{at} moved while its models were being read, so none were kept: ask again"
            ))
        };
        Listing {
            at: at.clone(),
            outcome,
        }
    }

    fn catalogue(&self, at: &Where, key: &str) -> Result<Vec<Served>, Failure> {
        let asked = match at.wire() {
            Wire::Anthropic => {
                let mut endpoint = AnthropicConfig::new(key);
                if let Some(base_url) = at.base_url() {
                    endpoint = endpoint.with_base_url(base_url);
                }
                futures::executor::block_on(endpoint.connect(self.transport.clone()).list_models())
            }
            Wire::Openai => {
                let mut endpoint = OpenAIConfig::new(key);
                if let Some(base_url) = at.base_url() {
                    endpoint = endpoint.with_base_url(base_url);
                }
                futures::executor::block_on(endpoint.connect(self.transport.clone()).list_models())
            }
            Wire::Copilot => {
                let mut endpoint =
                    OpenAIConfig::with_key(&rig_core::providers::copilot::DIALECT, key);
                if let Some(base_url) = at.base_url() {
                    endpoint = endpoint.with_base_url(base_url);
                }
                futures::executor::block_on(endpoint.connect(self.transport.clone()).list_models())
            }
        };
        asked
            .map(names_of)
            .map_err(|error| Failure::from_wire(error, false, &self.secrets))
    }
}

/// The wire's own reply is never repeated here: a hint is one line, not a body.
fn reason(at: &Where, failure: &Failure) -> String {
    match failure.verdict {
        Verdict::Unconfigured => credential_source(at.wire()),
        verdict => verdict.phrase().to_owned(),
    }
}

fn names_of(catalogue: ModelList) -> Vec<Served> {
    catalogue
        .into_iter()
        .map(|model| {
            let label = model.display_name().to_owned();
            Served {
                id: model.id,
                label,
            }
        })
        .collect()
}
