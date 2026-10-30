//! The box a file too large to show, or a record read back, is opened in: read
//! once, windowed while it is up, and closed with `Esc`.
//! A reader, not an overlay: the overlay is what opens it.

use std::path::Path;

/// A file too large to show, or a record read back, open in the frame. It is
/// read once, when it is opened, and only the rows on screen are ever copied
/// out of it, so a megabyte of output is a window per frame and
/// never a copy of itself.
#[derive(Debug, Clone)]
pub struct Spill {
    /// What the box is called: the file it came from, or the session it is.
    pub title: String,
    pub lines: Vec<String>,
    pub scroll: usize,
    pub visible: usize,
}

impl Spill {
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Spill> {
        let path = path.as_ref();
        // A command's output is bytes, not text: a stream that is not UTF-8
        // is still shown, with its invalid sequences replaced.
        let bytes = std::fs::read(path)?;
        let lines: Vec<String> = String::from_utf8_lossy(&bytes)
            .lines()
            .map(|line| line.to_string())
            .collect();
        Ok(Spill {
            title: format!("output: {}", path.display()),
            lines: if lines.is_empty() {
                vec![String::new()]
            } else {
                lines
            },
            scroll: 0,
            visible: 1,
        })
    }

    /// A reader over text the frame already has: the rendered record of a past
    /// session, read-only in the same box a spill opens in (issue #51).
    pub fn text(title: impl Into<String>, text: &str) -> Spill {
        let lines: Vec<String> = text.lines().map(|line| line.to_string()).collect();
        Spill {
            title: title.into(),
            lines: if lines.is_empty() {
                vec![String::new()]
            } else {
                lines
            },
            scroll: 0,
            visible: 1,
        }
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
    use super::Spill;

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
