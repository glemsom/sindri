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
    /// gesture named here has keys beside it (ADR-0011 §7).
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

/// What has the screen, when something does.
#[derive(Debug, Clone)]
pub enum Overlay {
    /// Every binding, from `?` or `/help`.
    Help,
    /// A command's output, untruncated.
    Spill(Spill),
}

impl Overlay {
    /// The rows this overlay draws, before the frame wraps them: the keys and
    /// the gesture that reaches the same thing.
    pub fn rows(&self) -> Vec<(&'static str, String)> {
        match self {
            Overlay::Help => BINDINGS
                .iter()
                .map(|binding| {
                    let gesture = match binding.mouse {
                        Some(mouse) => format!(" · {mouse}"),
                        None => String::new(),
                    };
                    (binding.keys, format!("{}{gesture}", binding.what))
                })
                .collect(),
            Overlay::Spill(spill) => spill.title(),
        }
    }
}

/// A spill file, open in the frame. The file is read once, when it is opened,
/// and only the rows on screen are ever copied out of it (ADR-0011 §9).
#[derive(Debug, Clone)]
pub struct Spill {
    /// The file the untruncated stream is in (ADR-0007).
    pub path: PathBuf,
    /// Its lines, as it holds them.
    pub lines: Vec<String>,
    /// How far down the file the reader has scrolled.
    pub scroll: usize,
    /// How many rows the overlay last had room for.
    pub visible: usize,
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
            visible: 1,
        })
    }

    /// The overlay's title row: which file, how long, and that `Esc` closes it.
    pub fn title(&self) -> Vec<(&'static str, String)> {
        vec![(
            "file",
            format!(
                "{} · {} lines · Esc closes",
                self.path.display(),
                self.lines.len()
            ),
        )]
    }

    /// Records how tall the overlay is, so a scroll can stop where the file's
    /// last screenful is, rather than at its last line.
    pub fn settled(&mut self, visible: usize) {
        self.visible = visible.max(1);
    }

    /// The lines a window of `visible` rows shows, from where the reader is.
    /// Nothing is cloned: a megabyte of output is a slice per frame, not a copy.
    pub fn window(&mut self) -> &[String] {
        let height = self.visible.min(self.lines.len().max(1));
        let last = self.lines.len().saturating_sub(height);
        self.scroll = self.scroll.min(last);
        let start = self.scroll;
        let end = (start + height).min(self.lines.len());
        &self.lines[start..end]
    }

    /// One screenful up or down, stopping at the ends of the file.
    pub fn scroll_by(&mut self, rows: usize, up: bool) {
        let last = self.lines.len().saturating_sub(self.visible.max(1));
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

    /// The keys the contract names, written here from ADR-0011 §14 and
    /// ADR-0013 rather than read out of `BINDINGS`: a key the frame gained and
    /// the help forgot is a failure, not a tautology.
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
        "Ctrl-Q",
        "?",
        "@",
        "/new, /help, /quit",
    ];

    #[test]
    fn the_help_lists_every_key_the_contract_names() {
        let listed: Vec<&str> = Overlay::Help.rows().iter().map(|(keys, _)| *keys).collect();
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
            gestures, 3,
            "the wheel, a drag and a click on the spill row are the gestures"
        );
    }

    #[test]
    fn a_spill_file_opens_untruncated_and_scrolls_by_the_screen() {
        let file = tempfile::tempdir().expect("a temp directory");
        let path = file.path().join("spill");
        let text: String = (0..50).map(|line| format!("line {line}\n")).collect();
        std::fs::write(&path, text).expect("write the spill");

        let mut spill = Spill::open(&path).expect("the spill opens");
        assert_eq!(spill.lines.len(), 50, "nothing is cut down");
        spill.settled(10);
        assert_eq!(spill.window().first().map(String::as_str), Some("line 0"));
        spill.scroll_by(5, true);
        assert_eq!(spill.scroll, 5);
        assert_eq!(spill.window().first().map(String::as_str), Some("line 5"));
        spill.scroll_by(1000, true);
        assert_eq!(
            spill.scroll, 40,
            "scrolling stops where the last screenful is full, not at the last line"
        );
        assert_eq!(spill.window().last().map(String::as_str), Some("line 49"));
        spill.scroll_by(1000, false);
        assert_eq!(spill.scroll, 0, "and at the first");
    }
}
