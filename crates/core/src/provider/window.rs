//! How much conversation a model family can hold: the rail's meter needs a
//! denominator, and only the adapter knows the family names.

/// The context window a model family is advertised with, where we know it. A
/// family we do not know gets `None`, and the rail counts up without a bar
/// rather than guessing a denominator for a trust fact.
pub fn context_window(model: &str) -> Option<u64> {
    let name = model.to_ascii_lowercase();
    if name.contains("claude") {
        Some(200_000)
    } else if name.contains("gpt-4o") || name.contains("gpt-4-turbo") {
        Some(128_000)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::context_window;

    #[test]
    fn a_family_we_know_has_a_window_and_a_family_we_do_not_is_unknown() {
        assert_eq!(context_window("claude-sonnet-4-6"), Some(200_000));
        assert_eq!(context_window("gpt-4o-mini"), Some(128_000));
        assert_eq!(
            context_window("a-model-we-have-never-seen"),
            None,
            "an unknown family is unknown, not an invented number"
        );
    }
}
