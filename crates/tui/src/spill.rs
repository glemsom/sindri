//! The box a file too large to show is opened in: read once, scrolled and
//! windowed while it is up, and closed with `Esc`.
//! A reader, not an overlay: the overlay is what opens it.

use std::path::Path;

/// A file too large to show, open in the frame: read once, when it is opened,
/// scrolled while it is up, and only the rows on screen are ever copied out of
/// it, so a megabyte of output is a window per frame and never a copy of itself.
#[derive(Debug, Clone)]
pub struct Spill {
    /// What the box is called: the file it came from.
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
        Ok(Spill::from_lines(
            format!("output: {}", path.display()),
            lines,
        ))
    }

    /// The one place a spill is built, so that "a spill has at least one line"
    /// is stated once: `window()` can then never hand back an empty slice, and
    /// a box opened on nothing still draws its own row.
    fn from_lines(title: impl Into<String>, lines: Vec<String>) -> Spill {
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

    #[test]
    fn an_empty_spill_file_opens_on_one_blank_row() {
        let file = tempfile::tempdir().expect("a temp directory");
        let path = file.path().join("empty");
        std::fs::write(&path, "").expect("write the empty spill");

        let mut spill = Spill::open(&path).expect("the spill opens");
        assert_eq!(
            spill.lines,
            vec![String::new()],
            "one line, and it is blank"
        );
        assert_eq!(
            spill.window().len(),
            1,
            "a window on an empty spill file is still a row, not nothing"
        );
    }
}
