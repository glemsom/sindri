//! The turn in flight: the answer and the thought of the run's newest turn, fed a
//! fragment at a time as the wire streams them (§4, §12).
//!
//! A [`Stream`] is the one thing in this front end that is not a fact. Deltas are
//! deliberately not entries (ADR-0007), so nothing here is written down: a run of 250
//! turns would write a line per fragment for one prompt. What the run writes down is
//! the facts the turn settles into, and [`Stream::said`] is what that costs — one run
//! of words per stretch of one kind of speech, in the order the turn said them.
//!
//! The order is kept rather than collapsed into a thought buffer and an answer buffer,
//! because a wire may reason, answer a line, and reason on, and a row drawn out of order
//! would be a picture of something the run never said. A fragment lands on the end of
//! the newest run when it is of the same kind and opens a run of its own when it is not,
//! which is the whole of what interleaving costs.
//!
//! The rows are [`crate::rows`]': a thought is laid out by the same code whether it is
//! being streamed or read back from the transcript, so a settled thought does not change
//! shape under a reader when the turn ends, and the live rows a cache holds are the same
//! rows. `#43` wires the core's [`Deltas`] to this value; `View` implements it so the
//! frame itself can be handed to the run as the sink.

use sindri_core::provider::Deltas;

use crate::rows::{Row, answer, thought};

/// What a run has said so far of the turn it is in the middle of: the fragments that
/// have landed, in the order they landed, each stretch of them belonging to a thought or
/// to the answer.
#[derive(Clone, Debug, Default)]
pub struct Stream {
    /// The runs of fragments, oldest first: one per stretch of one kind of speech.
    runs: Vec<Run>,
    /// How many fragments have landed, so a cache can tell a stream that has moved from
    /// one that has not without comparing the words it holds.
    revision: u64,
}

/// One stretch of one kind of speech, as the words it accumulated.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Run {
    /// Whether this stretch was reasoned or said.
    said: Said,
    /// The fragments of it, in the order they landed.
    words: String,
}

/// One of the two things a turn says, which is what decides a row's gutter and its ink
/// (§4, §12) and which entry the run writes down for it when the turn ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Said {
    /// A thought: what the model said to itself before answering.
    Reasoning,
    /// The answer: what the run is producing for its reader.
    Answer,
}

impl Stream {
    /// A turn that has said nothing yet.
    pub fn new() -> Stream {
        Stream::default()
    }

    /// How many fragments have landed here: a tick a cache reads to tell whether the
    /// rows it holds for this stream are still the rows it would lay out now.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Whether the turn has said nothing at all, which is the case a frame draws no live
    /// entry for.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// What the turn has said so far, in order: one run per stretch of one kind, which
    /// is what a run writes down as entries when its turn ends (§13, #43) — a thought as
    /// its own entry beside the answer it reached.
    pub fn said(&self) -> impl Iterator<Item = (&str, Said)> {
        self.runs.iter().map(|run| (run.words.as_str(), run.said))
    }

    /// The rows the turn in flight is drawn as at `cols`: every run set in the gutter and
    /// ink of the kind it is, laid out by the code the settled entries are laid out by.
    pub fn rows(&self, cols: u16) -> Vec<Row> {
        let mut rows = Vec::new();
        for run in &self.runs {
            rows.extend(match run.said {
                Said::Reasoning => thought(&run.words, cols),
                Said::Answer => answer(&run.words, cols),
            });
        }
        rows
    }

    /// One fragment landed: written on the end of the newest run when that run is of the
    /// same kind, and as a run of its own when something of the other kind has been said
    /// since. A fragment of nothing is not a tick — a wire that sends an empty one has
    /// said nothing, and a cache that re-laid the rows for it would wrap the same text
    /// again to draw the same picture.
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

/// The wire's own door into the turn in flight: the two fragments §4 lets it stream, and
/// nothing else. `#43` hands the run this value; until then a fixture feeds it a fragment
/// at a time, so the picture of a stream is made the way a stream is made.
impl Deltas for Stream {
    fn text(&mut self, fragment: &str) {
        self.landed(Said::Answer, fragment);
    }

    fn reasoning(&mut self, fragment: &str) {
        self.landed(Said::Reasoning, fragment);
    }
}
