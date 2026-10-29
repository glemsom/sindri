//! Credentials never reach the disk (ADR-0006). One value holds the secrets a
//! run knows, and every string the record writes passes through it first, so a
//! new field cannot forget.

use crate::provider::REDACTED;

/// The secrets a run must never write down.
#[derive(Debug, Clone, Default)]
pub struct Redaction {
    secrets: Vec<String>,
}

impl Redaction {
    /// The redaction for one run: every secret it knows, empties ignored.
    pub fn new(secrets: impl IntoIterator<Item = String>) -> Self {
        Redaction {
            secrets: secrets
                .into_iter()
                .filter(|secret| !secret.is_empty())
                .collect(),
        }
    }

    /// `text` with every known secret replaced, ready to be written.
    pub fn apply(&self, text: &str) -> String {
        let mut redacted = text.to_string();
        for secret in &self.secrets {
            redacted = redacted.replace(secret.as_str(), REDACTED);
        }
        redacted
    }
}
