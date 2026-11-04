//! The wire a run speaks: which provider, and what each one needs — its
//! credential variable, its base URL variable, and whether it needs a
//! credential at all. One file, because a wire's needs are one decision.

use super::ConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    /// The OpenAI-compatible Chat Completions wire: OpenAI itself, and every
    /// server that speaks its shape — Ollama, vLLM, LM Studio, OpenRouter,
    /// Groq. Which one is the base URL's business, not ours.
    Openai,
    /// GitHub Copilot, with device-code sign-in and a stored session.
    Copilot,
}

impl Provider {
    pub fn parse(name: &str) -> Result<Self, ConfigError> {
        match name {
            "anthropic" => Ok(Provider::Anthropic),
            "openai" => Ok(Provider::Openai),
            "copilot" => Ok(Provider::Copilot),
            other => Err(ConfigError::Usage(format!(
                "unknown provider `{other}`: the providers are `anthropic`, `openai` and `copilot`"
            ))),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::Openai => "openai",
            Provider::Copilot => "copilot",
        }
    }

    /// The API-key environment variable, empty when this provider uses OAuth.
    pub const fn api_key_env(self) -> &'static str {
        match self {
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::Openai => "OPENAI_API_KEY",
            Provider::Copilot => "",
        }
    }

    pub const fn base_url_env(self) -> &'static str {
        match self {
            Provider::Anthropic => "ANTHROPIC_BASE_URL",
            Provider::Openai => "OPENAI_BASE_URL",
            // Copilot's authenticated session supplies its service endpoint.
            Provider::Copilot => "",
        }
    }

    /// Whether the run cannot start without an API key. A local server has
    /// none to give, and Copilot resolves OAuth from its token store.
    pub const fn needs_credential(self) -> bool {
        match self {
            Provider::Anthropic => true,
            Provider::Copilot => false,
            Provider::Openai => false,
        }
    }

    /// Where a model this run has none comes from, in this wire's own terms: a
    /// server names the models it serves, an API sells them by name, so only
    /// the wire reached by base URL has anywhere new to point.
    pub(super) fn model_hint(self) -> Option<&'static str> {
        match self {
            Provider::Anthropic | Provider::Copilot => None,
            Provider::Openai => Some(
                "; the server names its own, e.g. `curl <base-url>/models`, or the aggregator's model list",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Provider;

    #[test]
    fn copilot_is_a_provider_that_signs_in_instead_of_taking_an_api_key() {
        let provider = Provider::parse("copilot").expect("Copilot provider name");
        assert_eq!(provider.as_str(), "copilot");
        assert!(!provider.needs_credential());
        assert_eq!(provider.base_url_env(), "");
    }
}
