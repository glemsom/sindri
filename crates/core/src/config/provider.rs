//! The wire a run speaks: which provider, and what each one needs — its
//! credential variable, its base URL variable, and whether it needs a
//! credential at all. One file, because a wire's needs are one decision.
//!
//! A wire's name is read in two directions and written once: `parse` is the
//! one place a word becomes a wire, `ALL` is every wire there is, and the
//! settings modal's provider row offers `ALL` rather than a list of its own, so
//! a fourth wire is a word the run and the row learn together or neither does.

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
    /// Every wire there is, in the order `parse` reads their words: the row
    /// that offers a wire offers these and nothing else, so the modal cannot
    /// name one the run would refuse.
    pub const ALL: [Provider; 3] = [Provider::Anthropic, Provider::Openai, Provider::Copilot];

    /// The wire `name` speaks, or the run's own refusal — the sentence the
    /// settings modal states, because a value the modal refuses is a value the
    /// run refuses, and the modal has no words of its own for it.
    pub fn parse(name: &str) -> Result<Self, ConfigError> {
        match name {
            "anthropic" => Ok(Provider::Anthropic),
            "openai" => Ok(Provider::Openai),
            "copilot" => Ok(Provider::Copilot),
            other => Err(ConfigError::Usage(format!(
                "unknown provider `{other}`: the providers are {}",
                Self::named()
            ))),
        }
    }

    /// The wires as the phrase a refusal names them with — every word `parse`
    /// takes, listed from `ALL` rather than typed a second time.
    fn named() -> String {
        let last = Provider::ALL.len().saturating_sub(1);
        Provider::ALL
            .iter()
            .enumerate()
            .map(|(at, wire)| match at {
                0 => format!("`{}`", wire.as_str()),
                at if at == last => format!(" and `{}`", wire.as_str()),
                _ => format!(", `{}`", wire.as_str()),
            })
            .collect()
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
    fn every_wire_the_row_offers_is_a_word_the_run_parses_and_no_other() {
        for wire in Provider::ALL {
            assert_eq!(
                Provider::parse(wire.as_str()),
                Ok(wire),
                "`{}` is a word the run accepts, and the row that chooses a wire offers it",
                wire.as_str()
            );
        }
        let mut words: Vec<&str> = Provider::ALL.iter().map(|wire| wire.as_str()).collect();
        words.sort_unstable();
        words.dedup();
        assert_eq!(
            words.len(),
            Provider::ALL.len(),
            "and offers each of them once: {words:?}"
        );
    }

    #[test]
    fn the_refusal_names_every_wire_the_row_offers() {
        let said = Provider::parse("gemini")
            .expect_err("a fourth wire is no wire the run speaks")
            .message()
            .to_string();
        assert!(
            said.starts_with("unknown provider `gemini`:"),
            "the refusal is about the word that was read: {said}"
        );
        for wire in Provider::ALL {
            assert!(
                said.contains(&format!("`{}`", wire.as_str())),
                "and names `{}` among the wires it offers: {said}",
                wire.as_str()
            );
        }
    }

    #[test]
    fn copilot_is_a_provider_that_signs_in_instead_of_taking_an_api_key() {
        let provider = Provider::parse("copilot").expect("Copilot provider name");
        assert_eq!(provider.as_str(), "copilot");
        assert!(!provider.needs_credential());
        assert_eq!(provider.base_url_env(), "");
    }
}
