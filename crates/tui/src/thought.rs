//! A thought the run is having: the turn it belongs to, the text so far and the
//! moment it began — and, once it is over, the moment the pane began to fold
//! back into the scar the thread keeps of it.
//! It is held beside the thread and never in it, because a thought is a state
//! the run is in and not something that was said — the transcript is a document
//! of what was said, and a thought is what happened first. `render::pane` is the
//! one surface it is drawn on, and the one row it leaves in the thread belongs
//! to `entry::Entry`'s scar.
//!
//! It is the text and nothing else: a fragment is the text the moment it lands,
//! so the pane draws what the provider has sent and never a prefix of it, and
//! nothing a provider sent is dropped.

use std::time::Duration;

/// One thought, as it arrives. `started` is the run's own clock when the first
/// fragment landed, which is what the pane's head row counts from and what its
/// unfurl is measured against; `text` is every fragment of it, appended in the
/// order the provider sent them, whole.
///
/// `closed` is the moment the final reasoning event landed, and the clock the
/// fold runs on: the pane gives a row back a tick until it is gone, and the scar
/// is written on the tick it is one row tall. `scarred` says that row has been
/// written, so a fold that runs past it writes one scar and not several.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thought {
    pub turn: u32,
    pub text: String,
    pub started: Duration,
    pub closed: Option<Duration>,
    pub scarred: bool,
}

impl Thought {
    /// A thought arriving: one fragment in, and the clock it began at.
    pub fn new(turn: u32, text: String, started: Duration) -> Self {
        Thought {
            turn,
            text,
            started,
            closed: None,
            scarred: false,
        }
    }

    /// A fragment in, at the tail of the text the pane draws: it is the pane's
    /// on the tick it lands, because there is nothing between the two.
    pub fn push(&mut self, fragment: &str) {
        self.text.push_str(fragment);
    }

    /// The final reasoning event: the whole of the turn's thinking, which is
    /// what the scar keeps, and the moment the fold begins. It is the text the
    /// deltas were already saying, and it moves the fold's clock and no word.
    pub fn close(&mut self, whole: String, now: Duration) {
        self.text = whole;
        self.closed.get_or_insert(now);
    }

    /// How long the thought ran, from the run's own clock: the moment its final
    /// event landed less the moment its first fragment did. `None` while it is
    /// still arriving, because a thought has no duration until it is over.
    pub fn took(&self) -> Option<Duration> {
        self.closed
            .map(|closed| closed.saturating_sub(self.started))
    }

    /// How long the thought has been closed at `now`: the fold's own clock, and
    /// `None` while the thought is still arriving, which is a pane that has no
    /// folding to do.
    pub fn closed_for(&self, now: Duration) -> Option<Duration> {
        self.closed.map(|at| now.saturating_sub(at))
    }
}

#[cfg(test)]
mod tests {
    use super::Thought;
    use crate::pulse;
    use std::time::Duration;

    /// A thought that has just been told its first fragment at the frame's own
    /// clock of zero, which is what a scripted clock reads from.
    fn thinking() -> Thought {
        Thought::new(1, String::new(), Duration::ZERO)
    }

    /// The clock `nth` ticks of the pulse in.
    fn ticks(nth: u64) -> Duration {
        pulse::TICK * nth as u32
    }

    #[test]
    fn a_fragment_is_the_thoughts_text_the_moment_it_lands() {
        let mut thought = thinking();
        thought.push("a fragment ");
        assert_eq!(
            thought.text, "a fragment ",
            "a thought is the text the provider sent and has no other part"
        );
        thought.push("and another");
        assert_eq!(
            thought.text, "a fragment and another",
            "and every fragment after it is appended where it lands, whole"
        );
    }

    #[test]
    fn a_thought_of_any_length_is_kept_whole() {
        // A megabyte of fragments, as a provider with a lot on its mind sends
        // them: the thought is one copy of what the record holds, and there is
        // no cap on it and nothing past it that is let go.
        let mut thought = thinking();
        for _ in 0..1024 {
            thought.push(&"a".repeat(1024));
        }
        assert_eq!(
            thought.text.len(),
            1024 * 1024,
            "not one byte of a long thought is dropped"
        );
        thought.push("z");
        assert!(
            thought.text.ends_with('z'),
            "and the newest fragment is the tail of it, where the pane reads"
        );
    }

    #[test]
    fn a_closed_thought_says_how_long_it_ran() {
        let mut thought = thinking();
        assert_eq!(
            thought.took(),
            None,
            "a thought still arriving has no duration to say"
        );

        thought.close("a thought".to_string(), ticks(35));

        assert_eq!(
            thought.took(),
            Some(Duration::from_millis(4200)),
            "the run's own clock, from the first fragment it was given to the \
             final event that closed it"
        );
    }
}
