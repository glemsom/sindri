//! The overlays that take the screen while they are open: `?`'s list of every
//! binding, and the untruncated output a spill affordance offers (ADR-0011
//! §9, §14). Both close with `Esc`, and neither is the only way to do what it
//! does.

use std::path::{Path, PathBuf};

/// One way into the frame: the keys that reach it, the gesture that also
/// reaches it, and what it does.
pub struct Binding {
    /// The keys, as the help overlay spells them.
    pub keys: &'static str,
    /// The mouse gesture with the same effect, where there is one. Every
    /// gesture here names the key beside it (ADR-0011 §7).
    pub mouse: Option<&'static str>,
    /// What it does.
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
        keys: "/new, /help, /quit",
        mouse: None,
        what: "start a fresh session, show this, leave",
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
        keys: "Ctrl-Y",
        mouse: Some("drag"),
        what: "copy the last answer, ANSI stripped",
    },
    Binding {
        keys: "Shift+drag",
        mouse: Some("Shift+drag"),
        what: "leave the selection to the terminal",
    },
    Binding {
        keys: "Ctrl-O",
        mouse: Some("click the spill row"),
        what: "open a command's untruncated output",
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
];

/// What has the screen, when something does.
#[derive(Debug, Clone)]
pub enum Overlay {
    /// Every binding, from `?` or `/help`.
    Help,
    /// A command's output, untruncated.
    Spill(Spill),
}

impl Overlay {
    /// The rows this overlay draws, before the frame wraps them.
    pub fn rows(&self) -> Vec<(&'static str, String)> {
        match self {
            Overlay::Help => BINDINGS
                .iter()
                .map(|binding| (binding.keys, binding.what.to_string()))
                .collect(),
            Overlay::Spill(spill) => spill.shown(),
        }
    }
}

/// A spill file, open in the frame.
#[derive(Debug, Clone)]
pub struct Spill {
    /// The file the untruncated stream is in (ADR-0007).
    pub path: PathBuf,
    /// Its lines, as it holds them.
    pub lines: Vec<String>,
    /// How far up the file the reader has scrolled.
    pub scroll: usize,
}

impl Spill {
    /// Opens a spill file. A file that cannot be read is not an overlay: the
    /// frame says so instead.
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Spill> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        let lines: Vec<String> = text.lines().map(|line| line.to_string()).collect();
        Ok(Spill {
            path: path.to_path_buf(),
            lines: if lines.is_empty() {
                vec![String::new()]
            } else {
                lines
            },
            scroll: 0,
        })
    }

    /// The rows the overlay shows: its title, then the file, scrolled.
    pub fn shown(&self) -> Vec<(&'static str, String)> {
        let mut rows: Vec<(&'static str, String)> = vec![(
            "file",
            format!(
                "{} · {} lines · Esc closes",
                self.path.display(),
                self.lines.len()
            ),
        )];
        rows.push(("", String::new()));
        for line in &self.lines {
            rows.push(("", line.clone()));
        }
        rows
    }

    /// One screenful up or down, stopping at the ends of the file.
    pub fn scroll_by(&mut self, rows: usize, up: bool) {
        let last = self.lines.len().saturating_sub(1);
        self.scroll = if up {
            self.scroll.saturating_add(rows).min(last)
        } else {
            self.scroll.saturating_sub(rows)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{BINDINGS, Overlay, Spill};

    #[test]
    fn every_mouse_gesture_names_a_key_that_does_the_same() {
        for binding in BINDINGS {
            if binding.mouse.is_some() {
                assert!(
                    !binding.keys.is_empty(),
                    "{} has no keyboard twin",
                    binding.what
                );
            }
        }
    }

    #[test]
    fn the_help_lists_every_binding_the_frame_has() {
        let rows = Overlay::Help.rows();
        assert_eq!(rows.len(), BINDINGS.len());
        let listed: Vec<&str> = rows.iter().map(|(keys, _)| *keys).collect();
        for wanted in ["PgUp, PgDn", "Ctrl-Y", "Ctrl-O", "?", "Ctrl-Q", "Tab"] {
            assert!(
                listed.iter().any(|keys| keys.contains(wanted)),
                "the help lists {wanted}: {listed:?}"
            );
        }
    }

    #[test]
    fn a_spill_file_opens_untruncated_and_scrolls() {
        let file = tempfile::tempdir().expect("a temp directory");
        let path = file.path().join("spill");
        let text: String = (0..50).map(|line| format!("line {line}\n")).collect();
        std::fs::write(&path, text).expect("write the spill");

        let mut spill = Spill::open(&path).expect("the spill opens");
        assert_eq!(spill.lines.len(), 50, "nothing is cut down");
        assert!(spill.shown().iter().any(|(_, row)| row == "line 49"));
        spill.scroll_by(5, true);
        assert_eq!(spill.scroll, 5);
        spill.scroll_by(1000, true);
        assert_eq!(spill.scroll, 49, "scrolling stops at the last line");
        spill.scroll_by(1000, false);
        assert_eq!(spill.scroll, 0, "and at the first");
    }
}
