//! Served names: what a wire says it serves, read off the wire and kept beside
//! the (wire, base_url) pair that produced them, never invented here and never
//! kept once that pair has moved.
//!
//! §10 asks a wire what models it serves and puts the answer beside the pair
//! that gave it, so a listing can never be attributed to a wire that did not
//! say it. A caller reads that pair again when the answer lands: a catalogue
//! that crossed a change of wire or of base URL is dropped, because names read
//! from one endpoint are not evidence about another.
//!
//! A wire that cannot answer has no list. That is not a failure — §10 makes it
//! a hint, so a front end shows one line where the list would go and the run
//! asks for a model named by hand and starts anyway. [`Listing`] therefore has
//! no error case at all: there is nothing here for a caller to refuse over.

use std::fmt;

use rig_core::model::ModelList;
use rig_core::providers::anthropic::AnthropicConfig;
use rig_core::providers::openai::OpenAIConfig;

use super::{Client, Credential, Failure, Verdict, credential_source};
use crate::wire::Wire;

/// Where a catalogue is asked for: the wire, and the endpoint it answers at
/// when that is not the dialect's own. This pair is what a listing is kept
/// beside and what is read again when the listing lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Where {
    /// The dialect that was asked.
    wire: Wire,
    /// The endpoint asked, when it is not the dialect's own.
    base_url: Option<String>,
}

impl Where {
    /// The dialect's own endpoint, as `ANTHROPIC_BASE_URL` / `OPENAI_BASE_URL`
    /// leave it when neither is set.
    pub fn own(wire: Wire) -> Self {
        Self {
            wire,
            base_url: None,
        }
    }

    /// `wire` at `base_url`, as a gateway or a proxy gives.
    pub fn at(wire: Wire, base_url: impl Into<String>) -> Self {
        Self {
            wire,
            base_url: Some(base_url.into()),
        }
    }

    /// The dialect this pair asks.
    pub fn wire(&self) -> Wire {
        self.wire
    }

    /// The endpoint this pair asks, when it is not the dialect's own.
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

/// The pair in force at the instant a listing lands, read then and not before.
/// A [`Where`] is itself such a pair that cannot move, which is what a caller
/// whose wire is fixed for the whole run passes; a front end whose provider row
/// can be edited while a catalogue is in flight passes the cell behind it.
pub trait Current {
    /// The pair in force now.
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

/// One model a wire says it serves, exactly as the wire wrote it: no name here
/// is derived, completed or case-folded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Served {
    /// The identifier to ask for. This is the only place it can come from.
    pub id: String,
    /// What the wire calls it in prose, or the identifier when it named none.
    pub label: String,
}

/// What asking a wire for its served names produced: the names it listed,
/// beside the pair that listed them — or no list at all and one line saying
/// why. There is no third case and no error case: a wire that cannot answer is
/// a hint, never a refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// The pair this listing belongs to, recorded before the request left.
    at: Where,
    /// The names, or the reason there are none.
    outcome: Outcome,
}

/// A listing's two shapes, kept apart so a caller cannot read names off a wire
/// that never gave any.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Outcome {
    /// The wire answered: what it says it serves, in the order it said it.
    Listed(Vec<Served>),
    /// The wire could not answer, in one line a caller may show instead.
    Silent(String),
}

impl Listing {
    /// The names the wire served, in its own order; `None` when it had no list
    /// to give. An empty catalogue is an answer and reads as `Some`, which is
    /// how a wire that genuinely serves nothing is told apart from one that
    /// could not be asked.
    pub fn names(&self) -> Option<&[Served]> {
        match &self.outcome {
            Outcome::Listed(names) => Some(names),
            Outcome::Silent(_) => None,
        }
    }

    /// The pair this listing is kept beside, whether or not it answered.
    pub fn at(&self) -> &Where {
        &self.at
    }

    /// One line a caller shows where the list would go: what was tried and what
    /// to do instead. `None` when the wire answered, so a hint is never shown
    /// beside names.
    pub fn hint(&self) -> Option<&str> {
        match &self.outcome {
            Outcome::Silent(hint) => Some(hint),
            Outcome::Listed(_) => None,
        }
    }
}

impl Client {
    /// Ask `at` what it serves, with `credential`, and keep the answer beside
    /// that pair — unless `current` names another pair by the time it lands, in
    /// which case the answer is dropped and the listing says so.
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

    /// One wire asked for its catalogue, with the credential it needs and at the
    /// endpoint the pair names.
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
            .map_err(|error| Failure::from_wire(error, false))
    }
}

/// Why there is no list, in words a human reads: the wire's credential source
/// when it was never asked, and otherwise the phrase its verdict earns. The
/// wire's own reply is never repeated here — a hint is one line, not a body.
fn reason(at: &Where, failure: &Failure) -> &'static str {
    match failure.verdict {
        Verdict::Unconfigured => credential_source(at.wire()),
        verdict => verdict.phrase(),
    }
}

/// The names a catalogue holds, in the wire's own order, labelled as the wire
/// labelled them.
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
