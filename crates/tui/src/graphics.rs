//! What this terminal can draw, and how big a cell it draws it in: the one
//! question the frame asks it. A picture needs the same answer the face does
//! and asking twice would double the cost — the query spends up
//! to a second and reads stdin, and a second read could eat a keystroke.

use std::io::IsTerminal;

use ratatui_image::picker::{Picker, ProtocolType};

pub mod kitty;

/// The terminal's answer: the protocol it speaks and the size of its cell.
pub struct Graphics {
    picker: Picker,
}

impl Graphics {
    /// Ask the terminal what it can draw. It is asked once per launch, so both
    /// the face and a picture borrow this answer rather than spending their own
    /// query: nothing else may call `Picker::from_query_stdio`.
    pub fn detect() -> Option<Graphics> {
        if !asks(std::io::stdout().is_terminal(), |name| {
            std::env::var(name).ok()
        }) {
            return None;
        }
        Some(Graphics::from_picker(Picker::from_query_stdio().ok()?))
    }

    /// The answer a test makes up rather than asks for: a cell twice as tall as
    /// it is wide, as a terminal's is, speaking Kitty. The layout tests then see
    /// a picture in the shape it really takes, on the protocol it is drawn with.
    pub fn kitty() -> Graphics {
        let mut picker = Picker::from_fontsize((8, 18));
        picker.set_protocol_type(ProtocolType::Kitty);
        Graphics::from_picker(picker)
    }

    pub(crate) fn from_picker(picker: Picker) -> Graphics {
        Graphics { picker }
    }

    pub fn picker(&self) -> &Picker {
        &self.picker
    }
}

/// Whether the terminal is asked at all: it is one, and it has announced that
/// it can speak Kitty graphics. Asking costs a second of startup and reads
/// stdin, so a terminal that says nothing about itself is never asked — silence
/// is the fallback anyway — and a stream that is not a terminal is never asked,
/// because nothing is drawn to one.
fn asks(is_terminal: bool, lookup: impl Fn(&str) -> Option<String>) -> bool {
    is_terminal && announces_kitty(lookup)
}

/// Whether the terminal announced a speaker that can plausibly answer.
fn announces_kitty(lookup: impl Fn(&str) -> Option<String>) -> bool {
    const TELLS: [&str; 3] = ["TERM", "TERM_PROGRAM", "LC_TERMINAL"];
    const SPEAKERS: [&str; 4] = ["kitty", "ghostty", "wezterm", "foot"];
    lookup("KITTY_WINDOW_ID").is_some()
        || TELLS.iter().any(|name| {
            lookup(name).is_some_and(|value| {
                let value = value.to_ascii_lowercase();
                SPEAKERS.iter().any(|speaker| value.contains(speaker))
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A terminal's environment, as the module sees it.
    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        move |name| {
            vars.iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        }
    }

    #[test]
    fn the_test_answer_is_kitty_with_a_cell_twice_as_tall_as_it_is_wide() {
        let graphics = Graphics::kitty();
        assert_eq!(
            graphics.picker().protocol_type(),
            ProtocolType::Kitty,
            "a layout test sees the picture in the shape it really takes"
        );
        assert_eq!(graphics.picker().font_size(), (8, 18));
    }

    #[test]
    fn a_stream_that_is_not_a_terminal_is_never_asked() {
        assert!(
            !asks(false, env(&[("TERM", "xterm-kitty")])),
            "nothing is drawn to a pipe, so nothing is asked of it — even a Kitty one"
        );
    }

    #[test]
    fn a_terminal_that_names_no_speaker_is_never_asked() {
        assert!(
            !asks(true, env(&[("TERM", "xterm-256color")])),
            "an ordinary terminal is not asked: the query reads stdin and costs a second"
        );
        assert!(!asks(true, env(&[])), "and silence is the fallback anyway");
    }

    #[test]
    fn a_terminal_that_announces_a_speaker_is_asked() {
        for announce in [
            ("KITTY_WINDOW_ID", "1"),
            ("TERM", "xterm-kitty"),
            ("TERM_PROGRAM", "WezTerm"),
            ("LC_TERMINAL", "ghostty"),
            ("TERM", "foot"),
        ] {
            assert!(
                asks(true, env(&[announce])),
                "{announce:?} is a terminal that can answer"
            );
        }
    }
}
