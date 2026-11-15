use rig_core::model::ModelList;

use super::endpoint::Endpoint;
use super::{Client, Credential, Failure, Verdict, credential_source};
use crate::redact::SecretSet;

pub trait Current {
    fn pair(&self) -> Endpoint;
}

impl Current for Endpoint {
    fn pair(&self) -> Endpoint {
        self.clone()
    }
}

impl<F: Fn() -> Endpoint> Current for F {
    fn pair(&self) -> Endpoint {
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
    at: Endpoint,
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

    pub fn at(&self) -> &Endpoint {
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
    /// The answer is kept beside the endpoint it was asked of, and dropped when `current`
    /// names another endpoint by the time it lands: names read from one endpoint are not
    /// evidence about another.
    pub fn served(&self, at: &Endpoint, credential: &Credential, current: &dyn Current) -> Listing {
        let mut secrets = self.secrets.clone();
        let read = self
            .credential(at.wire(), credential, &mut secrets)
            .map_err(|failure| failure.message.to_string())
            .and_then(|key| match key {
                Some(key) => self
                    .catalogue(at, &key, &secrets)
                    .map_err(|failure| reason(at, &failure)),
                None => Err(credential_source(at.wire())),
            });
        let outcome = if current.pair() == *at {
            match read {
                Ok(names) => Outcome::Listed(names),
                Err(reason) => Outcome::Silent(format!(
                    "{at} has no model list: {reason}; name a model and the run goes on"
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

    fn catalogue(
        &self,
        at: &Endpoint,
        key: &str,
        secrets: &SecretSet,
    ) -> Result<Vec<Served>, Failure> {
        futures::executor::block_on(at.serving(key).catalogue(self.transport.clone()))
            .map(names_of)
            .map_err(|error| Failure::from_wire(error, false, secrets))
    }
}

/// The wire's own reply is never repeated here: a hint is one line, not a body.
fn reason(at: &Endpoint, failure: &Failure) -> String {
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
