//! The overlays that take the screen while they are open: `?`'s list of every
//! binding, and the untruncated output a spill affordance offers (ADR-0011).
//! Both close with `Esc`, and neither is the only way to do what it does.

use std::path::{Path, PathBuf};

use crate::settings::Settings;

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
        keys: "/new, /settings, /help, /quit",
        mouse: None,
        what: "start a fresh session, edit the config, show this, leave",
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

#[derive(Debug, Clone)]
pub enum Overlay {
    Help,
    Spill(Spill),
    Settings(Settings),
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

/// A spill file, open in the frame. The file is read once, when it is opened,
/// and only the rows on screen are ever copied out of it (ADR-0011).
#[derive(Debug, Clone)]
pub struct Spill {
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub scroll: usize,
    pub visible: usize,
}

impl Spill {
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Spill> {
        let path = path.as_ref();
        // A command's output is bytes, not text: a stream that is not UTF-8
        // is still shown, with its invalid sequences replaced (ADR-0007).
        let bytes = std::fs::read(path)?;
        let lines: Vec<String> = String::from_utf8_lossy(&bytes)
            .lines()
            .map(|line| line.to_string())
            .collect();
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

    pub fn settled(&mut self, visible: usize) {
        self.visible = visible.max(1);
    }

    /// The lines a window of `visible` rows shows, from where the reader is.
    /// A slice, not a copy: a megabyte of output is a window per frame.
    pub fn window(&mut self) -> &[String] {
        let height = self.visible.min(self.lines.len().max(1));
        let last = self.lines.len().saturating_sub(height);
        self.scroll = self.scroll.min(last);
        let start = self.scroll;
        let end = (start + height).min(self.lines.len());
        &self.lines[start..end]
    }

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
    use super::{BINDINGS, Spill, help_rows};

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
        "Ctrl-Q",
        "?",
        "@",
        "/new, /settings, /help, /quit",
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
