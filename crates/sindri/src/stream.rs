use sindri_core::provider::Deltas;

use crate::picture::Surface;
use crate::pulse::Head;
use crate::rows::{Row, answer_on, head_row, thought};

#[derive(Clone, Debug, Default)]
pub struct Stream {
    runs: Vec<Run>,
    revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Run {
    said: Said,
    words: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Said {
    Reasoning,
    Answer,
}

impl Stream {
    pub fn new() -> Stream {
        Stream::default()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Whether a fragment of `said` opens an entry of its own rather than continuing the newest: a
    /// run that opens an answer over its own thought opens a row as its first word does.
    pub fn opens(&self, said: Said) -> bool {
        self.runs.last().is_none_or(|newest| newest.said != said)
    }

    pub fn said(&self) -> impl Iterator<Item = (&str, Said)> {
        self.runs.iter().map(|run| (run.words.as_str(), run.said))
    }

    /// `head` goes on the newest answer alone, and on a row of its own before that answer
    /// has a fragment: a frame waiting on a wire must not look idle.
    pub fn rows(&self, cols: u16, head: Option<&Head>) -> Vec<Row> {
        self.rows_on(cols, head, &Surface::none())
    }

    pub fn rows_on(&self, cols: u16, head: Option<&Head>, surface: &Surface) -> Vec<Row> {
        let newest = self.runs.iter().rposition(|run| run.said == Said::Answer);
        let mut rows = Vec::new();
        for (at, run) in self.runs.iter().enumerate() {
            let head = if Some(at) == newest { head } else { None };
            rows.extend(match run.said {
                Said::Reasoning => thought(&run.words, cols),
                Said::Answer => answer_on(&run.words, cols, head, surface),
            });
        }
        if let (None, Some(head)) = (newest, head) {
            rows.push(head_row(head, cols));
        }
        rows
    }

    /// A fragment of nothing is not a tick: a wire that sends an empty one has said nothing.
    fn landed(&mut self, said: Said, fragment: &str) {
        if fragment.is_empty() {
            return;
        }
        self.revision += 1;
        match self.runs.last_mut() {
            Some(newest) if newest.said == said => newest.words.push_str(fragment),
            _ => self.runs.push(Run {
                said,
                words: String::from(fragment),
            }),
        }
    }
}

impl Deltas for Stream {
    fn text(&mut self, fragment: &str) {
        self.landed(Said::Answer, fragment);
    }

    fn reasoning(&mut self, fragment: &str) {
        self.landed(Said::Reasoning, fragment);
    }
}
