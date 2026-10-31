//! The palette a run draws with: a closed set of names, not a colour string.
//!
//! It is an enum because the theme is a law, not a value: `crates/tui/tests/theme`
//! can hold every colour to a floor only because the set of colours is finite,
//! and arbitrary hex would move that law into a dotfile. A name that is not one
//! of these is a usage error, the way an unknown provider is.
//!
//! One variant is threaded no further than the config: nothing renders
//! differently yet. The second theme is the change that carries a palette into
//! the renderers; this one is the seam it slots into.

use super::ConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Anvil,
}

impl Theme {
    pub fn parse(name: &str) -> Result<Self, ConfigError> {
        match name {
            "anvil" => Ok(Theme::Anvil),
            other => Err(ConfigError::Usage(format!(
                "unknown theme `{other}`: the themes are `anvil`"
            ))),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Theme::Anvil => "anvil",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_name_round_trips_through_parse_and_as_str() {
        assert_eq!(
            Theme::parse("anvil").expect("the name the file uses parses"),
            Theme::Anvil
        );
        assert_eq!(Theme::Anvil.as_str(), "anvil", "and says itself back");
    }

    #[test]
    fn a_name_that_is_not_a_theme_is_a_usage_error_naming_it_and_the_themes_there_are() {
        let error = Theme::parse("forge").expect_err("forge is not a theme");

        assert!(
            matches!(error, ConfigError::Usage(_)),
            "an unknown theme is a usage error, not a missing value: {error:?}"
        );
        let message = error.message();
        assert!(
            message.contains("forge"),
            "the error names the value: {message}"
        );
        assert!(
            message.contains("anvil"),
            "the error names every theme there is: {message}"
        );
    }
}
