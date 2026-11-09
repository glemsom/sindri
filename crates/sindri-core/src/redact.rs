//! Redaction is a type rather than a habit: every string that leaves the core is a `Redacted`,
//! whose `Display`, `Debug` and `Serialize` scrub through one `SecretSet` of literals and no
//! pattern, since a pattern that misses is worse than a literal that matches.

use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;
use serde::Serializer;
use serde::de::Deserializer;

pub const MASK: &str = "«redacted»";

#[derive(Clone, Default, PartialEq, Eq)]
pub struct SecretSet {
    /// Longest first: replacing a shorter one first would leave the longer one's tail behind.
    secrets: Arc<Vec<String>>,
}

impl SecretSet {
    pub fn of<I, S>(secrets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut held = Vec::new();
        for secret in secrets {
            keep(&mut held, secret.into());
        }
        Self {
            secrets: Arc::new(held),
        }
    }

    pub fn with(self, secret: impl Into<String>) -> Self {
        let mut set = self;
        set.add(secret);
        set
    }

    /// `secret` in the set's order, dropping an empty one: replacing it would put the mask
    /// between every character of every string.
    pub fn add(&mut self, secret: impl Into<String>) {
        keep(Arc::make_mut(&mut self.secrets), secret.into());
    }

    pub fn union(self, other: SecretSet) -> Self {
        let mut set = self;
        for secret in other.iter() {
            set.add(secret.clone());
        }
        set
    }

    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    pub fn redact(&self, text: impl Into<String>) -> Redacted {
        Redacted {
            text: text.into(),
            secrets: self.clone(),
        }
    }

    fn iter(&self) -> std::slice::Iter<'_, String> {
        self.secrets.iter()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Redacted {
    text: String,
    secrets: SecretSet,
}

impl Redacted {
    fn scrubbed(&self) -> String {
        let mut text = self.text.clone();
        for secret in self.secrets.iter() {
            text = text.replace(secret.as_str(), MASK);
        }
        text
    }
}

impl fmt::Display for Redacted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.scrubbed())
    }
}

impl fmt::Debug for Redacted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Redacted")
            .field(&self.scrubbed())
            .finish()
    }
}

impl Serialize for Redacted {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.scrubbed())
    }
}

impl<'de> Deserialize<'de> for Redacted {
    /// Holds no secrets: the set that scrubbed it belonged to the writer, so what is read
    /// back is already safe.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Redacted, D::Error> {
        Ok(Redacted {
            text: String::deserialize(deserializer)?,
            secrets: SecretSet::default(),
        })
    }
}

impl fmt::Debug for SecretSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretSet")
            .field("secrets", &self.secrets.len())
            .finish()
    }
}

fn keep(held: &mut Vec<String>, secret: String) {
    if secret.is_empty() {
        return;
    }
    let order = |held: &String| other_before(held, &secret);
    match held.binary_search_by(order) {
        Ok(_) => {}
        Err(at) => held.insert(at, secret),
    }
}

fn other_before(held: &String, other: &String) -> Ordering {
    other.len().cmp(&held.len()).then_with(|| held.cmp(other))
}
