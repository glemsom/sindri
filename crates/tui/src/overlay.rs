//! The overlays that take the screen while they are open: `?`'s list of every
//! binding, the untruncated output a spill affordance offers, and the records a
//! run left behind (ADR-0011). All close with `Esc`, and none is the only way
//! to do what it does. An open overlay owns its own space: the page keys and
//! the wheel move it, never the frame beneath it (ADR-0013).

use crate::sessions::Sessions;
use crate::settings::Settings;
use crate::spill::Spill;

pub struct Binding {
    pub keys: &'static str,
    pub mouse: Option<&'static str>,
    pub what: &'static str,
}

/// Every binding the frame has. The mouse is an accelerator: nothing here is
/// reachable only with a pointer.
pub const BINDINGS: &[Binding] = &[
    Binding {
        keys: "Enter",
        mouse: None,
        what: "send the prompt, or accept a completion",
    },
    Binding {
        keys: "Shift+Enter",
        mouse: None,
        what: "a newline in the prompt",
    },
    Binding {
        keys: "Tab",
        mouse: None,
        what: "accept the completion that is offered",
    },
    Binding {
        keys: "@",
        mouse: None,
        what: "complete a workspace path",
    },
    Binding {
        keys: "/new, /sessions, /export, /settings, /help, /quit",
        mouse: None,
        what: "start a session, read an old one, export this one, edit the config, show this, leave",
    },
    Binding {
        keys: "Esc",
        mouse: None,
        what: "dismiss a completion or an overlay",
    },
    Binding {
        keys: "PgUp, PgDn",
        mouse: Some("wheel"),
        what: "scroll the transcript",
    },
    Binding {
        keys: "Shift+Up, Shift+Down",
        mouse: None,
        what: "select rendered rows from the newest row upwards",
    },
    Binding {
        keys: "Ctrl-Y",
        mouse: Some("drag"),
        what: "copy the selection, or the last answer, styles stripped",
    },
    Binding {
        keys: "Ctrl-O",
        mouse: Some("click the spill row"),
        what: "open a command's untruncated output",
    },
    Binding {
        keys: "Ctrl-G",
        mouse: Some("click the session row"),
        what: "copy the session id, the one `sindri sessions show` takes",
    },
    Binding {
        keys: "↑",
        mouse: None,
        what: "the row above in the draft, or the newest queued prompt taken back",
    },
    Binding {
        keys: "Ctrl-X",
        mouse: None,
        what: "drop the newest queued prompt",
    },
    Binding {
        keys: "Ctrl-C",
        mouse: None,
        what: "cancel the run and keep the draft",
    },
    Binding {
        keys: "Ctrl-Q",
        mouse: None,
        what: "leave the frame",
    },
    Binding {
        keys: "?",
        mouse: None,
        what: "this list, on an empty prompt",
    },
    Binding {
        keys: "—",
        mouse: None,
        what: "Shift+drag is the terminal's own selection, never the frame's",
    },
];

/// `?`'s list of every binding, and where the reader is in it. The list wraps
/// and scrolls, so the key the overlay was opened to find is a row that can be
/// reached and not the one that fell off the bottom (issue #49).
#[derive(Debug, Clone, Default)]
pub struct Help {
    scroll: usize,
    /// What the box last drew: the rows there are, and the rows it had for
    /// them. The renderer measures; the keys clamp against the same numbers.
    total: usize,
    visible: usize,
}

impl Help {
    /// The rows the box drew, so a list short enough to fit does not scroll at
    /// all and a scrolled one stops where its last screenful is full.
    pub fn settled(&mut self, total: usize, visible: usize) {
        self.total = total;
        self.visible = visible.max(1);
        self.scroll = self.scroll.min(self.last());
    }

    fn last(&self) -> usize {
        self.total.saturating_sub(self.visible)
    }

    /// The rows the box shows, from where the reader is.
    pub fn window(&self) -> std::ops::Range<usize> {
        let start = self.scroll.min(self.last());
        start..(start + self.visible).min(self.total)
    }

    /// `PgUp`/`PgDn` and the wheel: one screenful of this box at a time — the
    /// transcript's page is the transcript's, and a list that covers it pages
    /// by what it covers — from the first row to the last and no further.
    pub fn page(&mut self, rows: usize, up: bool) {
        self.scroll = if up {
            self.scroll.saturating_sub(rows)
        } else {
            self.scroll.saturating_add(rows).min(self.last())
        };
    }

    /// Whether the list runs past the box above or below: what the box marks
    /// so a reader knows there is more without scrolling for it.
    pub fn above(&self) -> bool {
        self.window().start > 0
    }

    pub fn below(&self) -> bool {
        self.window().end < self.total
    }

    /// `keys · 3 of 12`: where the reader is, counted from one, said only when
    /// the list is longer than the box that holds it.
    pub fn title(&self) -> String {
        if self.total <= self.visible {
            return "keys".to_string();
        }
        format!("keys · {} of {}", self.window().start + 1, self.total)
    }
}

/// Where a cursor lands after one step of a list that keeps a chosen row: one
/// answer for the settings modal and the record picker, so neither drifts from
/// the other (issue #49).
pub(crate) fn step(chosen: usize, rows: usize, len: usize, up: bool) -> usize {
    if up {
        chosen.saturating_sub(rows)
    } else {
        chosen.saturating_add(rows).min(len.saturating_sub(1))
    }
}

#[derive(Debug, Clone)]
pub enum Overlay {
    Help(Help),
    Spill(Spill),
    /// Boxed: the modal is the largest thing the frame can overlay, and the
    /// enum is carried in every match on an overlay.
    Settings(Box<Settings>),
    /// The record picker: what the state directory holds, newest first.
    Sessions(Sessions),
}

pub fn help_rows() -> Vec<(&'static str, String)> {
    BINDINGS
        .iter()
        .map(|binding| {
            let gesture = match binding.mouse {
                Some(mouse) => format!(" · {mouse}"),
                None => String::new(),
            };
            (binding.keys, format!("{}{gesture}", binding.what))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{BINDINGS, help_rows};

    /// The keys the contract names, written here from ADR-0011 rather than
    /// read out of `BINDINGS`: a key the frame gained and the help forgot is
    /// a failure, not a tautology.
    const REQUIRED: &[&str] = &[
        "Enter",
        "Shift+Enter",
        "Tab",
        "Esc",
        "PgUp, PgDn",
        "Shift+Up, Shift+Down",
        "Ctrl-Y",
        "Ctrl-O",
        "Ctrl-C",
        "↑",
        "Ctrl-X",
        "Ctrl-Q",
        "?",
        "@",
        "/new, /sessions, /export, /settings, /help, /quit",
        "Ctrl-G",
    ];

    #[test]
    fn the_help_lists_every_key_the_contract_names() {
        let listed: Vec<&str> = help_rows().iter().map(|(keys, _)| *keys).collect();
        for wanted in REQUIRED {
            assert!(
                listed.contains(wanted),
                "the help lists {wanted:?}: {listed:?}"
            );
        }
    }

    #[test]
    fn every_mouse_gesture_names_keys_that_do_the_same() {
        let keyish = [
            "Enter", "Shift+", "Tab", "Esc", "Ctrl-", "Pg", "@", "/", "?",
        ];
        let mut gestures = 0;
        for binding in BINDINGS {
            let Some(mouse) = binding.mouse else {
                continue;
            };
            gestures += 1;
            assert!(
                keyish.iter().any(|key| binding.keys.contains(key)),
                "{} is reachable only by {mouse}: {:?} names no key",
                binding.what,
                binding.keys
            );
            assert!(
                !binding.keys.contains(mouse),
                "{} names the gesture as its own keyboard twin",
                binding.what
            );
        }
        assert_eq!(
            gestures, 4,
            "the wheel, a drag, a click on the spill row and a click on a rail row are the gestures"
        );
    }
}
