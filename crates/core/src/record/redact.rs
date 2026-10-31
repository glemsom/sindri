//! Credentials never reach the disk. One value holds the secrets a
//! run knows, and every string the record writes passes through it first, so a
//! new field cannot forget.

use std::borrow::Cow;

use crate::provider::REDACTED;

#[derive(Debug, Clone, Default)]
pub struct Redaction {
    secrets: Vec<String>,
}

impl Redaction {
    pub fn new(secrets: impl IntoIterator<Item = String>) -> Self {
        Redaction {
            secrets: secrets
                .into_iter()
                .filter(|secret| !secret.is_empty())
                .collect(),
        }
    }

    /// The caller's own slice when nothing in it is secret, which is nearly
    /// every string a run writes; a copy only when a replacement is due.
    pub fn apply<'a>(&self, text: &'a str) -> Cow<'a, str> {
        let mut redacted = Cow::Borrowed(text);
        for secret in &self.secrets {
            if !redacted.contains(secret.as_str()) {
                continue;
            }
            redacted = Cow::Owned(redacted.replace(secret.as_str(), REDACTED));
        }
        redacted
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::Redaction;
    use crate::provider::REDACTED;

    const KEY: &str = "sk-ant-test-not-a-real-key";

    /// The usual string on a transcript: a secret appears in one roughly never,
    /// so text that holds none is borrowed from the caller instead of copied on
    /// the thread that is already writing.
    #[test]
    fn text_holding_no_secret_is_borrowed_from_the_caller() {
        let redaction = Redaction::new([KEY.to_string()]);
        let text = "the answer streams through here";
        let applied = redaction.apply(text);
        assert!(
            matches!(applied, Cow::Borrowed(borrowed) if std::ptr::eq(borrowed, text)),
            "unchanged text must be the caller's own slice: {applied:?}"
        );
    }

    /// A redaction that holds nothing has nothing to look for, so every string
    /// comes back borrowed — the empty case must not allocate either.
    #[test]
    fn a_redaction_holding_no_secret_borrows_every_string() {
        let redaction = Redaction::new(Vec::<String>::new());
        let text = String::from("what is anything");
        let applied = redaction.apply(&text);
        assert!(
            matches!(applied, Cow::Borrowed(borrowed) if std::ptr::eq(borrowed, text.as_str())),
            "nothing to redact, nothing to copy: {applied:?}"
        );
    }

    /// The one string that does hold the key comes back owned and redacted, and
    /// every spelling of it in the text is caught.
    #[test]
    fn text_holding_a_secret_comes_back_owned_and_redacted() {
        let redaction = Redaction::new([KEY.to_string()]);
        let text = format!("{KEY} then {KEY} again");
        let applied = redaction.apply(&text);
        assert_eq!(
            applied.as_ref(),
            format!("{REDACTED} then {REDACTED} again"),
            "the key is gone, the rest of the text is not"
        );
        assert!(
            matches!(applied, Cow::Owned(_)),
            "a redaction the caller can see is one it can own: {applied:?}"
        );
    }

    /// Two secrets, one of them absent: the text is still redacted by the one it
    /// holds, which is what a run holding a key and a token relies on.
    #[test]
    fn every_secret_held_by_the_redaction_is_replaced() {
        let redaction = Redaction::new([KEY.to_string(), "sk-other".to_string()]);
        let text = format!("{KEY} and sk-other");
        assert_eq!(
            redaction.apply(&text).as_ref(),
            format!("{REDACTED} and {REDACTED}")
        );
    }
}
