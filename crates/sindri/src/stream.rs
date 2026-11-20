//! The turn in flight as the wire feeds it: the runs of fragments the newest turn was said in, the
//! revision that counts them, and whether each run is a reasoning or an answer.

use sindri_core::provider::Deltas;

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

    pub(crate) fn says_answer(&self) -> bool {
        self.runs.iter().any(|run| run.said == Said::Answer)
    }

    pub fn said(&self) -> impl Iterator<Item = (&str, Said)> {
        self.runs.iter().map(|run| (run.words.as_str(), run.said))
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
