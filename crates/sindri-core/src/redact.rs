//! Redaction is a type, not a habit: the secrets a run knows live in one
//! `SecretSet`, and every string that leaves the core is a `Redacted`, whose
//! `Display`, `Debug` and `Serialize` all scrub through that set.
//!
//! The set holds literal strings and no pattern, because a pattern that misses
//! is worse than a literal that matches: what has to be found is a value this
//! process was handed, not a shape someone guessed at (ADR-0011). They are
//! matched longest first, so a secret that is a prefix of another cannot leave
//! its tail behind by being replaced after the longer one.
//!
//! The doors are few and the windows are not: a secret arrives from the config
//! (`api_key`, every `extra_headers` value) or from a token store, and it wants
//! out through the record, the JSON, the verbose stream and whatever a provider
//! says back when it refuses a key. Making the value that leaves core a
//! `Redacted` is what turns forgetting to scrub into a type error rather than a
//! code review, and it is why none of those windows carries a scrubber of its
//! own.
//!
//! Nothing here prints a secret, not even to say it holds one: `Debug` masks,
//! and the set's own `Debug` reports how many secrets it holds and never which.

use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

use serde::Serialize;
use serde::Serializer;

/// What `Display`, `Debug` and `Serialize` write in place of a secret, so the
/// config, the provider seam and every window after them share one mask.
pub const MASK: &str = "«redacted»";

/// Every literal string a run must never let out, longest first.
///
/// Cloning one is cheap, which is what lets every [`Redacted`] hold its own
/// handle to the same secrets rather than a copy of them.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SecretSet {
    /// The secrets, longest first, with duplicates and empties dropped.
    secrets: Arc<Vec<String>>,
}

impl SecretSet {
    /// A set of the literal `secrets`.
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

    /// This set with `secret` added, for a door that arrives as one value rather
    /// than as a list.
    pub fn with(self, secret: impl Into<String>) -> Self {
        let mut set = self;
        set.add(secret);
        set
    }

    /// Adds `secret`, keeping the set longest first.
    ///
    /// An empty secret is not a secret and is dropped: replacing it would put
    /// the mask between every character of every string. A duplicate is dropped
    /// because it could only be replaced once anyway.
    pub fn add(&mut self, secret: impl Into<String>) {
        keep(Arc::make_mut(&mut self.secrets), secret.into());
    }

    /// Whether this set knows no secret at all, in which case redacting a value
    /// leaves it exactly as it was.
    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    /// How many secrets this set holds, which is all `Debug` will ever say.
    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    /// `text` as a value that scrubs itself wherever it goes.
    pub fn redact(&self, text: impl Into<String>) -> Redacted {
        Redacted {
            text: text.into(),
            secrets: self.clone(),
        }
    }

    /// The secrets in the order they are matched: longest first.
    fn iter(&self) -> std::slice::Iter<'_, String> {
        self.secrets.iter()
    }
}

/// One string that cannot leave the core with a secret in it: `Display`,
/// `Debug` and `Serialize` all write the scrubbed form, so a window that prints
/// a value prints a safe one without doing anything to it.
#[derive(Clone, PartialEq, Eq)]
pub struct Redacted {
    /// The text as it arrived, only ever written scrubbed.
    text: String,
    /// The secrets to scrub out of it, held so a value can never be made
    /// without them.
    secrets: SecretSet,
}

impl Redacted {
    /// The text with every secret replaced, which is the only form any of this
    /// type's printed forms ever writes.
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

impl fmt::Debug for SecretSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretSet")
            .field("secrets", &self.secrets.len())
            .finish()
    }
}

/// Keeps `secret` in `held` in the set's own order, dropping what is not a
/// secret and what is already there.
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

/// The order the set keeps its secrets in: longer before shorter, then by the
/// value itself.
fn other_before(held: &String, other: &String) -> Ordering {
    other.len().cmp(&held.len()).then_with(|| held.cmp(other))
}
