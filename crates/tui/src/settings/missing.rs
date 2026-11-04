//! What the front door found absent when it opened the frame itself: the run
//! has no credential, or no model, or neither — and the words for that absence,
//! which the transcript, the composer and a refused prompt each read. It holds
//! no value: the modal says where a key comes from, never what one is. It is a
//! module of its own because the absence outlives the modal that answers it —
//! the composer keeps refusing a prompt after the box has closed — and because
//! the words and the rows they describe must be read off one pair of flags.

use sindri_core::config::{Document, Key, Provider};

/// What is absent and how it is said: the wire the run speaks, the two things
/// that can be missing and the words each absence has, so the words a reader
/// sees and the rows they describe are read off one pair of flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    /// The wire the run speaks: its variable is where the credential comes
    /// from, and a local server needs none.
    provider: Provider,
    /// Whether the credential is one of the absent rows.
    key: bool,
    /// Whether the model is.
    model: bool,
    /// What the transcript has the reader type: the row, and every variable
    /// that sets it.
    ways: String,
    /// The same variables in the room the composer has, one row: it leaves the
    /// model to `/settings`.
    composer: String,
    /// What `/settings` does for it: `saves one`, `names one`, `sets both`.
    acted: &'static str,
}

impl Missing {
    /// What a resolved config could not find, when it found all of it: `None`
    /// is a run that can send a prompt, and there is nothing to say. The two
    /// flags are the one `match` on an absence, and they are kept, so the words
    /// a reader sees and the rows they describe are read off the same pair.
    pub fn of(provider: Provider, key: bool, model: bool) -> Option<Missing> {
        if !key && !model {
            return None;
        }
        let key_vars = Self::key_vars_for(provider);
        let (ways, composer, acted) = match (key, model) {
            (true, false) => (
                format!("one in the settings modal, or set {key_vars}"),
                key_vars,
                "saves one",
            ),
            (false, true) => (
                "one in the settings modal, or set SINDRI_MODEL".to_string(),
                "SINDRI_MODEL".to_string(),
                "names one",
            ),
            _ => (
                format!("them in the settings modal, or set {key_vars} and SINDRI_MODEL"),
                key_vars,
                "sets both",
            ),
        };
        Some(Missing {
            provider,
            key,
            model,
            ways,
            composer,
            acted,
        })
    }

    /// The credential's two variables, in the wire's own terms: the one fact
    /// every sentence about a missing key has to say, spelled once.
    pub fn key_vars(&self) -> String {
        Self::key_vars_for(self.provider)
    }

    fn key_vars_for(provider: Provider) -> String {
        format!("SINDRI_API_KEY or {}", provider.api_key_env())
    }

    /// The absence in words: `no key`, `no model`, `no key and no model` —
    /// read off the two flags rather than stored beside them, so the words and
    /// the rows they describe cannot drift apart. `of` never builds the pair
    /// with neither, which is why the last arm is the both-absent one.
    fn what(&self) -> &'static str {
        match (self.key, self.model) {
            (true, false) => "no key",
            (false, true) => "no model",
            _ => "no key and no model",
        }
    }

    /// Whether the credential is one of the absent rows: what the modal's key
    /// row says, and what the composer cannot do.
    pub fn wants_key(&self) -> bool {
        self.key
    }

    /// The row the modal opens on: the key when there is none, because a run
    /// with no credential is the failure a frame answers with a field.
    pub fn row(&self) -> Key {
        if self.key { Key::ApiKey } else { Key::Model }
    }

    /// The transcript's one line: what is absent and every way to set it —
    /// said once, when the frame opens, and never on a frame.
    pub fn said(&self) -> String {
        format!("{}: type {}", self.what(), self.ways)
    }

    /// What the composer says where the prompt would be: a run with no key
    /// cannot send one, and the frame says why rather than pretending.
    pub fn composer(&self) -> String {
        format!(
            "{} · a prompt cannot be sent · /settings, or set {}",
            self.what(),
            self.composer
        )
    }

    /// A prompt sent anyway: the draft stays where it was, and this is the one
    /// line the transcript gets for the keystroke.
    pub fn refused(&self) -> String {
        format!(
            "not sent: this run has {} — /settings {}",
            self.what(),
            self.acted
        )
    }

    /// What is still absent once a save has landed: a row the file now carries
    /// is filled in, and the rest is not. A key in the environment is not this
    /// frame's to see, so the file is what a save can answer for.
    pub fn satisfied(&self, document: &Document) -> Option<Missing> {
        let key = self.key && !document.is_set(Key::ApiKey);
        let model = self.model && !document.is_set(Key::Model);
        Missing::of(self.provider, key, model)
    }
}
