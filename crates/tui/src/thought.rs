//! A thought the run is having: the turn it belongs to, the text so far and the
//! moment it began — and, once it is over, the moment the pane began to fold
//! back into the scar the thread keeps of it.
//! It is held beside the thread and never in it, because a thought is a state
//! the run is in and not something that was said — the transcript is a document
//! of what was said, and a thought is what happened first. `render::pane` is the
//! one surface it is drawn on, and the one row it leaves in the thread belongs
//! to `entry::Entry`'s scar.
//!
//! It is also the pane's smoothing buffer: a provider sends its words in bursts,
//! so a fragment is queued where it lands and released to the pane at a reading
//! rate — `drain` is pure in the clock, as the pane's motion is — and the text
//! glides where rows in the flow could only jerk. This is the one thing a pane
//! buys that the thread cannot.

use std::time::Duration;

use crate::pulse;

/// The characters the pane takes from the queue a tick: a shade over a hundred a
/// second at the pulse's 120 ms, which is faster than a thought is read and slow
/// enough that a burst arrives as motion. It is the pane's one rate, and the
/// smoothness of a glide is this number and nothing else.
pub const PANE_RATE: usize = 12;

/// What a thought may hold unshown, in bytes: three panes' worth of words at a
/// hundred columns, and no more. Past it the oldest fragments are dropped rather
/// than spilled — a thought is not an answer and has no spill file, and a pane
/// lagging the model by a stretch the reader has already left is a pane showing
/// them a past they will never want. The record holds the whole text for
/// `sindri sessions show` and `/export`, so nothing anyone asked for is lost.
pub const PANE_QUEUE: usize = 2048;

/// One thought, as it arrives. `started` is the run's own clock when the first
/// fragment landed, which is what the pane's head row counts from and what its
/// unfurl is measured against; `text` is every fragment of it, appended in the
/// order the provider sent them.
///
/// `shown` is the part of that text the pane has released: the pane draws a
/// prefix and the rest is the queue, so the words arrive at the rate a reader
/// can follow them and never at the rate a provider chose to send them.
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
    /// The byte offset into `text` the pane has released to, always on a
    /// character boundary: `text[..shown]` is what the pane draws and
    /// `text[shown..]` is the queue.
    shown: usize,
    /// The clock the last release was owed from, so the rate is measured by the
    /// clock and not by how often the frame happened to call `drain`.
    released: Duration,
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
            shown: 0,
            released: started,
        }
    }

    /// A fragment in, at the tail of the text the pane is drawing from — and,
    /// past the cap, the oldest of what it has not shown let go.
    pub fn push(&mut self, fragment: &str) {
        self.text.push_str(fragment);
        self.trim();
    }

    /// The final reasoning event: the whole of the turn's thinking, which is
    /// what the scar keeps, and the moment the fold begins. What the queue still
    /// held is dropped — the pane may lag the model by a beat, and it may not
    /// hold the frame open to finish catching up — so the fold runs on the tick
    /// it would have folded anyway.
    pub fn close(&mut self, whole: String, now: Duration) {
        self.text = whole;
        self.shown = self.boundary(self.shown.min(self.text.len()));
        self.trim();
        self.closed.get_or_insert(now);
    }

    /// What the pane draws: the part of the thought the reading rate has
    /// released, which is a prefix of the whole and never a word out of order.
    pub fn shown(&self) -> &str {
        &self.text[..self.shown]
    }

    /// What is queued: the characters of the thought the pane has not released,
    /// in bytes so a frame can ask every tick without walking a long thought.
    pub fn queued(&self) -> usize {
        self.text.len() - self.shown
    }

    /// Lets the clock on: what has queued reaches the pane at `PANE_RATE`
    /// characters a tick, so a burst arrives as a glide. Pure in `now` — the
    /// release is measured from the thought's own clock, so two calls at one
    /// moment release nothing twice and a scripted clock is the whole of what
    /// drives it. A thought already closed has nothing to release: the fold is
    /// the clock's, and a pane that caught up on the way out would be the frame
    /// held open for words it has already dropped.
    pub fn drain(&mut self, now: Duration) {
        if self.closed.is_some() {
            return;
        }
        if self.queued() == 0 {
            // Nothing was owed while the queue stood empty, so a fragment that
            // lands after a quiet minute waits its own share of ticks rather
            // than arriving on the burst of one nobody spent.
            self.released = now;
            return;
        }
        let ticks = pulse::ticks(now.saturating_sub(self.released));
        if ticks == 0 {
            return;
        }
        self.released += pulse::TICK * ticks as u32;
        self.release(ticks * PANE_RATE);
    }

    /// The first `characters` of the queue released to the pane, or all of it
    /// where there is less.
    fn release(&mut self, characters: usize) {
        self.shown = match self.text[self.shown..].char_indices().nth(characters) {
            Some((at, _)) => self.shown + at,
            None => self.text.len(),
        };
    }

    /// The cap held: past it the oldest unshown fragments are dropped by moving
    /// the pane's reading on, which costs nothing and loses only words the pane
    /// was already too far behind to reach in time.
    fn trim(&mut self) {
        let Some(excess) = self.queued().checked_sub(PANE_QUEUE) else {
            return;
        };
        self.shown = self.boundary(self.shown + excess);
    }

    /// `at` stepped on to the nearest character boundary of `text`: a cap in
    /// bytes and a replacement of the whole may both land inside a glyph, and a
    /// pane may not draw half of one.
    fn boundary(&self, at: usize) -> usize {
        let mut at = at;
        while !self.text.is_char_boundary(at) {
            at += 1;
        }
        at
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
    use super::{PANE_QUEUE, PANE_RATE, Thought};
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

    /// Twenty fragments, lettered so a test can say which of them the pane has
    /// reached: what a provider that bursts sends in one tick.
    fn burst(thought: &mut Thought) -> String {
        let whole: String = (1..=20).map(|n| format!("fragment {n} ")).collect();
        for n in 1..=20 {
            thought.push(&format!("fragment {n} "));
        }
        whole
    }

    #[test]
    fn a_burst_reaches_the_pane_at_a_reading_rate() {
        let mut thought = thinking();
        let whole = burst(&mut thought);

        thought.drain(Duration::ZERO);
        assert_eq!(
            thought.shown(),
            "",
            "twenty fragments landing in one tick are not the pane's on that tick"
        );

        for nth in 1..=10 {
            thought.drain(ticks(nth));
            let shown = thought.shown();
            assert!(
                whole.starts_with(shown),
                "the pane shows a prefix of the thought at tick {nth}: {shown:?}"
            );
            assert_eq!(
                shown.chars().count(),
                nth as usize * PANE_RATE,
                "and releases `PANE_RATE` characters a tick, not the burst's"
            );
        }

        // The tick after the last release is the whole of it, and no tick after
        // that moves it again.
        for nth in 11..=40 {
            thought.drain(ticks(nth));
            if thought.shown().len() == whole.len() {
                break;
            }
        }
        assert_eq!(
            thought.shown(),
            whole,
            "the glide ends on the whole thought and never short of it"
        );
    }

    #[test]
    fn the_queue_drops_its_oldest_past_its_cap() {
        // A megabyte of fragments, as a provider with a lot on its mind sends
        // them: the queue is the frame's own memory and it is capped, while the
        // text it reads from is one copy of what the record holds too.
        let mut thought = thinking();
        for _ in 0..1024 {
            thought.push(&"a".repeat(1024));
        }
        assert!(
            thought.queued() <= PANE_QUEUE,
            "a megabyte of fragments leaves the queue at its cap: {} bytes",
            thought.queued()
        );
        assert!(
            thought.shown().len() > 1000 * 1024,
            "and the oldest fragments are dropped rather than spilled: the pane \
             begins its reading well past the thought's head"
        );
        assert_eq!(
            thought.text.len(),
            1024 * 1024,
            "the text is kept whole, because the scar opens what the pane dropped"
        );

        thought.push(&"z".repeat(1024));
        assert!(
            thought.queued() <= PANE_QUEUE && thought.text.ends_with('z'),
            "the newest fragment is what a full queue keeps"
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

    #[test]
    fn a_closed_thought_releases_nothing_more() {
        // The pane may lag the model by a beat; it may not hold the frame open
        // to catch up. The final event says the whole text, and the queue is
        // dropped where it stood.
        let mut thought = thinking();
        let whole = burst(&mut thought);
        thought.drain(ticks(1));
        let shown = thought.shown().to_string();
        assert!(!shown.is_empty(), "the glide has begun");

        thought.close(whole.clone(), ticks(1));
        for nth in 2..=20 {
            thought.drain(ticks(nth));
        }

        assert_eq!(
            thought.shown(),
            shown,
            "a closed thought folds on the tick it would have folded anyway, with \
             the remainder dropped rather than shown on the way out"
        );
        assert_eq!(
            thought.text, whole,
            "and the scar keeps every word of it, which is what `Enter` opens"
        );
    }
}
