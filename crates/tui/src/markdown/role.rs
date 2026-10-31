//! Which role a token is painted in: the one fact about a fence that belongs
//! to the language rather than to the theme. `ROLES` is the table, `of` reads a
//! scope stack against it, and the hexes it answers with are `theme.rs`'s.

use ratatui::style::Color;
use syntect::parsing::ScopeStack;

use crate::theme;

/// The scope a token is painted by, as a prefix of the scopes syntect stacks
/// on it: `keyword` covers `keyword.control.rust`, and nothing here is a fact
/// about a colour. `ROLES` is ordered — the first role any scope in the stack
/// answers to wins — because a string's escape and the language a keyword
/// belongs to are both scopes on one token.
const ROLES: &[(&[&str], Color)] = &[
    (&["comment"], theme::COMMENT),
    (
        &["keyword", "storage.type", "storage.modifier"],
        theme::KEYWORD,
    ),
    (&["string"], theme::STRING),
    (
        &[
            "constant.numeric",
            "constant.language",
            "constant.character.escape",
        ],
        theme::NUMBER,
    ),
    (
        &[
            "entity.name.type",
            "entity.name.struct",
            "entity.name.class",
            "support.type",
        ],
        theme::TYPE,
    ),
    (
        &[
            "entity.name.function",
            "support.function",
            "variable.function",
            "meta.function-call",
        ],
        theme::FUNCTION,
    ),
];

/// Which role a token's scope stack answers to, or `PLAIN` where no role's
/// prefix is in the stack: an exotic language loses its colour rather than
/// failing a fence, because a wrong role is worse than no highlight.
pub fn of(scopes: &ScopeStack) -> Color {
    // A scope is a pair of hashes; its name is the only way to read it, so the
    // stack is spelled once and every rule asks the same words.
    let names: Vec<String> = scopes
        .scopes
        .iter()
        .map(|scope| scope.build_string())
        .collect();
    for (prefixes, colour) in ROLES {
        if prefixes
            .iter()
            .any(|prefix| names.iter().any(|name| answers(name, prefix)))
        {
            return *colour;
        }
    }
    theme::PLAIN
}

/// Whether a scope answers to a rule's prefix: the prefix itself, or anything
/// beneath it — `keyword` reaches `keyword.control.rust`, never `keywordish`.
fn answers(scope: &str, prefix: &str) -> bool {
    scope == prefix
        || scope
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::theme;

    /// The role a scope stack is painted in, by the table's rules.
    fn painted(stack: &str) -> Color {
        of(&ScopeStack::from_str(stack).expect("a scope stack"))
    }

    #[test]
    fn a_scope_stack_is_classified_into_one_of_the_themes_seven_roles() {
        assert_eq!(
            painted("source.rust comment.line.double-slash.rust"),
            theme::COMMENT
        );
        for stack in [
            "source.rust keyword.control.rust",
            "source.rust storage.type.rust",
            "source.rust storage.modifier.rust",
        ] {
            assert_eq!(painted(stack), theme::KEYWORD, "{stack}");
        }
        assert_eq!(
            painted("source.rust string.quoted.double.rust"),
            theme::STRING
        );
        for stack in [
            "source.rust constant.numeric.integer.decimal.rust",
            "source.rust constant.language.rust",
            "source.rust constant.character.escape.rust",
        ] {
            assert_eq!(painted(stack), theme::NUMBER, "{stack}");
        }
        for stack in [
            "source.rust entity.name.type.rust",
            "source.rust entity.name.struct.rust",
            "source.python entity.name.class.python",
            "source.rust support.type.rust",
        ] {
            assert_eq!(painted(stack), theme::TYPE, "{stack}");
        }
        for stack in [
            "source.rust entity.name.function.rust",
            "source.rust support.function.rust",
            "source.rust variable.function.rust",
            "source.rust meta.function-call.rust",
        ] {
            assert_eq!(painted(stack), theme::FUNCTION, "{stack}");
        }
        // Anything else is plain ink: a language no rule knows loses its
        // colour rather than failing a fence.
        assert_eq!(painted("source.rust variable.other.rust"), theme::PLAIN);
        assert_eq!(painted("source.weird meta.thing"), theme::PLAIN);
    }

    #[test]
    fn a_role_earlier_in_the_table_wins_and_a_prefix_is_a_whole_segment() {
        // An escape inside a string is painted as the string it is in.
        assert_eq!(
            painted("source.rust string.quoted.double.rust constant.character.escape.rust"),
            theme::STRING
        );
        // A scope that merely starts with a rule's letters is not that rule.
        assert_eq!(painted("source.exotic keywordish.thing"), theme::PLAIN);
    }
}
