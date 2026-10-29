//! The wire a run speaks: which provider, and what each one needs — its
//! credential variable, its base URL variable, and whether it needs a
//! credential at all. One file, because a wire's needs are one decision.

use super::ConfigError;

/// The wire a run speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// Anthropic's Messages API.
    Anthropic,
    /// The OpenAI-compatible Chat Completions wire: OpenAI itself, and every
    /// server that speaks its shape — Ollama, vLLM, LM Studio, OpenRouter,
    /// Groq. Which one is the base URL's business, not ours (ADR-0003).
    Openai,
}

impl Provider {
    /// The provider a name selects.
    pub fn parse(name: &str) -> Result<Self, ConfigError> {
        match name {
            "anthropic" => Ok(Provider::Anthropic),
            "openai" => Ok(Provider::Openai),
            other => Err(ConfigError::Usage(format!(
                "unknown provider `{other}`: the providers are `anthropic` and `openai`"
            ))),
        }
    }

    /// The name this provider is configured and recorded as.
    pub const fn as_str(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::Openai => "openai",
        }
    }

    /// The variable this provider's credential normally lives in.
    pub const fn api_key_env(self) -> &'static str {
        match self {
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::Openai => "OPENAI_API_KEY",
        }
    }

    /// The variable this provider's own client would read its base URL from.
    pub const fn base_url_env(self) -> &'static str {
        match self {
            Provider::Anthropic => "ANTHROPIC_BASE_URL",
            Provider::Openai => "OPENAI_BASE_URL",
        }
    }

    /// Whether the run cannot start without a credential. A local server has
    /// none to give, so demanding one would make the user invent a fake; a
    /// hosted server that wants one answers 401, which is reported as itself.
    pub const fn needs_credential(self) -> bool {
        match self {
            Provider::Anthropic => true,
            Provider::Openai => false,
        }
    }

    /// Where a model this run has none comes from, in this wire's own terms: a
    /// server names the models it serves, an API sells them by name.
    pub(super) fn model_hint(self) -> &'static str {
        match self {
            Provider::Anthropic => "",
            Provider::Openai => {
                "; the server names its own, e.g. `curl <base-url>/models`, or the aggregator's model list"
            }
        }
    }
}
