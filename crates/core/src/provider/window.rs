//! How much conversation a model family can hold: the rail's meter needs a
//! denominator, and only the adapter knows the family names.

/// One row of the table: a fragment of a model name, the window the family is
/// advertised with, and the provider page that number was read from.
struct FamilyWindow {
    /// A fragment a model name is matched against, in lower case.
    fragment: &'static str,
    /// The tokens the family is advertised with.
    window: u64,
    /// Where the number was read from: the row's provenance, not part of the
    /// answer, so only the test reads it. A row without one is a guess dressed
    /// as a fact.
    #[cfg_attr(not(test), allow(dead_code, reason = "provenance the test insists on"))]
    source: &'static str,
}

/// The families we know, and only them. The first row a name matches answers,
/// so a narrower family has to sit above any wider fragment it holds — where
/// the window is the same, a mis-ordered pair would answer with the wrong row
/// and say nothing. `no_row_is_shadowed_by_an_earlier_fragment_its_family_holds`
/// is what keeps that true as rows are added.
const FAMILY_WINDOWS: &[FamilyWindow] = &[
    FamilyWindow {
        fragment: "claude",
        window: 200_000,
        source: "https://platform.claude.com/docs/en/about-claude/models/overview — \
                 every current Claude model lists a 200K context window",
    },
    FamilyWindow {
        fragment: "gpt-4o",
        window: 128_000,
        source: "https://platform.openai.com/docs/models/gpt-4o — 128,000 context tokens",
    },
    FamilyWindow {
        fragment: "gpt-4-turbo",
        window: 128_000,
        source: "https://platform.openai.com/docs/models/gpt-4-turbo — 128,000 context tokens",
    },
    FamilyWindow {
        fragment: "llama",
        window: 128_000,
        source: "https://github.com/meta-llama/llama-models — the Llama 3.x model \
                 cards list 128K tokens of context",
    },
    FamilyWindow {
        fragment: "qwen3",
        window: 32_768,
        source: "https://huggingface.co/Qwen/Qwen3-32B — 32,768 tokens natively, \
                 131,072 only with YaRN, so the native window is the honest one",
    },
];

/// The context window a model family is advertised with, where we know it. A
/// family we do not know gets `None`, and the rail counts up without a bar
/// rather than guessing a denominator for a trust fact.
pub fn context_window(model: &str) -> Option<u64> {
    let name = model.to_ascii_lowercase();
    FAMILY_WINDOWS
        .iter()
        .find(|row| name.contains(row.fragment))
        .map(|row| row.window)
}

#[cfg(test)]
mod tests {
    use super::{FAMILY_WINDOWS, context_window};

    #[test]
    fn every_row_of_the_table_is_asserted() {
        assert!(
            !FAMILY_WINDOWS.is_empty(),
            "a table with no rows knows nothing"
        );
        for row in FAMILY_WINDOWS {
            assert_eq!(
                context_window(&format!("{}-a-variant", row.fragment)),
                Some(row.window),
                "the row for {} claims {}: {}",
                row.fragment,
                row.window,
                row.source
            );
        }
    }

    #[test]
    fn a_family_never_seen_before_is_unknown() {
        assert_eq!(
            context_window("a-model-we-have-never-seen"),
            None,
            "an unknown family is unknown, not an invented number"
        );
    }

    #[test]
    fn the_composite_names_this_tree_configures_have_a_denominator() {
        // A vendor prefix or a size suffix must not hide the family: these are
        // the shapes the tests, the fixtures and the docs here run. A stand-in
        // such as `kimi-k3` is nobody's published model, so it is no row here.
        for model in [
            "claude-sonnet-4-6",
            "gpt-4o-mini",
            "gpt-4-turbo",
            "llama3.2",
            "meta-llama/llama-3.2-3b-instruct",
            "qwen3-32b",
        ] {
            assert!(
                context_window(model).is_some(),
                "{model} is a model this repository configures, so its family is known"
            );
        }
    }

    #[test]
    fn every_row_names_where_its_number_comes_from() {
        for row in FAMILY_WINDOWS {
            assert!(
                row.source.starts_with("https://"),
                "the row for {} states {} with no source to check it against: {}",
                row.fragment,
                row.window,
                row.source
            );
        }
    }

    #[test]
    fn no_row_is_shadowed_by_an_earlier_fragment_its_family_holds() {
        for (index, row) in FAMILY_WINDOWS.iter().enumerate() {
            for earlier in &FAMILY_WINDOWS[..index] {
                assert!(
                    !row.fragment.contains(earlier.fragment),
                    "`{}` holds the earlier fragment `{}`, so a name for it answers with the wrong row",
                    row.fragment,
                    earlier.fragment
                );
            }
        }
    }
}
