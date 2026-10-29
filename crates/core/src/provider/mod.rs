//! The model, behind one seam: ask, and get an answer or a failure that is
//! safe to print. Retries and redaction live here; the provider library lives
//! only in [`rig_wire`], so a library rewrite is one file's problem (ADR-0003).

mod rig_wire;

use std::time::Duration;

use crate::config::Config;

/// How many times one request is attempted before the failure is reported.
const MAX_ATTEMPTS: u32 = 3;
/// The first wait between attempts; each retry doubles it.
const BASE_BACKOFF: Duration = Duration::from_millis(200);
/// What a redacted credential is replaced by, everywhere.
pub const REDACTED: &str = "[redacted]";

/// What one completion cost and produced.
#[derive(Debug, Clone)]
pub struct Answer {
    /// The assistant's text: the product (ADR-0004).
    pub text: String,
    /// The model the provider says answered, when it says.
    pub model: Option<String>,
    /// The tokens the turn was billed for.
    pub usage: Usage,
}

/// The token counters a provider reports. Absent means unreported, never zero.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    /// Every input token, cached or not.
    pub input_tokens: Option<u64>,
    /// Every output token, reasoning included.
    pub output_tokens: Option<u64>,
    /// The input tokens a provider served from its cache.
    pub cached_input_tokens: Option<u64>,
    /// Input plus output, when the provider reports both.
    pub total_tokens: Option<u64>,
}

/// A provider or transport failure. Its message is the provider's own, with
/// the credential redacted, because that is what a user needs to act on.
#[derive(Debug, Clone)]
pub struct Error {
    message: String,
    status: Option<u16>,
}

impl Error {
    /// The failure as one line: no credential, and no stack.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The HTTP status the provider answered with, when it answered at all.
    pub fn status(&self) -> Option<u16> {
        self.status
    }

    /// Whether another attempt could succeed. Only rate limits and server
    /// faults qualify: a client error repeats identically forever (ADR-0003).
    fn retryable(&self) -> bool {
        matches!(self.status, Some(429) | Some(500..=599))
    }

    /// A transport failure: no reply, so no status.
    fn transport(message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            status: None,
        }
    }

    /// A failure with the status the provider answered with.
    fn answered(status: Option<u16>, message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            status,
        }
    }
}

/// One configured provider endpoint: the wire, the credential and the model.
#[derive(Debug, Clone)]
pub struct Client {
    provider: crate::config::Provider,
    api_key: String,
    base_url: Option<String>,
    model: String,
}

impl Client {
    /// The client a resolved config describes.
    pub fn new(config: &Config) -> Client {
        Client {
            provider: config.provider,
            api_key: config.api_key.clone(),
            base_url: config.base_url.clone(),
            model: config.model.clone(),
        }
    }

    /// The provider this client speaks to.
    pub fn provider(&self) -> crate::config::Provider {
        self.provider
    }

    /// The model this client asks.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// One question, retried while retrying can help and reported otherwise.
    pub fn ask(&self, prompt: &str) -> Result<Answer, Error> {
        let mut attempt = 1;
        loop {
            match rig_wire::complete(self, prompt) {
                Ok(answer) => return Ok(answer),
                Err(error) if error.retryable() && attempt < MAX_ATTEMPTS => {
                    std::thread::sleep(BASE_BACKOFF * 2u32.pow(attempt - 1));
                    attempt += 1;
                }
                Err(error) => return Err(self.redact(error)),
            }
        }
    }

    /// The credential never leaves the process in a message or a record.
    fn redact(&self, error: Error) -> Error {
        if self.api_key.is_empty() {
            return error;
        }
        Error {
            message: error.message.replace(&self.api_key, REDACTED),
            status: error.status,
        }
    }

    /// The pieces the wire needs, named for it so `rig_wire` holds no policy.
    pub(crate) fn parts(&self) -> (&str, Option<&str>, &str) {
        (&self.api_key, self.base_url.as_deref(), &self.model)
    }
}
