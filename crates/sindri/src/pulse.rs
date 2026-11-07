//! The marker at the head of the newest answer: the braille glyph a live run turns, the
//! duration it says in words, and the mark a stopped run settles on (§12, ticket #39).
//!
//! Nothing here reads a clock. [`pulse_glyph`] and [`format_duration`] are functions of
//! milliseconds and of nothing else, so every number the contract froze can be held at its
//! boundary by a unit test, and a frame fixture can paint a marker at 4.2 s without waiting
//! four seconds — the clock is the front end's, and `tui` is where it lives.
//!
//! A settled marker carries the duration it stopped at as a number, and no path here asks a
//! clock again. That is the difference the ticket draws: a marker whose duration was read
//! where it was painted would say `12.4s` when the run stopped and `72.4s` a minute later.
//!
//! The glyphs are the braille cells a spinner is spelled with, one step every 80 ms, and
//! the whole mark is bounded — one cell and at most fourteen characters beside it — so a
//! frame narrow enough to cut prose still holds the mark whole. What the mark is drawn
//! inside — the answer's own gutter, at the head of its first row — is `rows`' business,
//! and [`Head`] is the value the two share.

use ratatui::text::Span;
use sindri_core::run::Stop;

use crate::ink::Ink;

/// How long the marker holds one glyph: one step of §12's pulse, which is also the wait a
/// live run gives the loop in `tui`.
pub const STEP_MS: u64 = 80;

/// The glyphs the marker steps through, in the order it steps through them.
const GLYPHS: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// The glyph a live run wears `elapsed_ms` into its run: one step per 80 ms, wrapping every
/// ten, so the marker is a clock a reader can see moving without it ever saying a number.
pub fn pulse_glyph(elapsed_ms: u64) -> char {
    let step = (elapsed_ms / STEP_MS) % GLYPHS.len() as u64;
    GLYPHS[usize::try_from(step).unwrap_or(0)]
}

/// `ms` as the duration §12 says it in, the unit chosen by how long it is: tenths of a
/// second below ten seconds, seconds below a minute, minutes and seconds below an hour, and
/// hours and minutes above it. Truncated rather than rounded, because a run 999 ms in has
/// not been running a second yet.
pub fn format_duration(ms: u64) -> String {
    match ms {
        ms if ms < 10_000 => format!("{}.{}s", ms / 1_000, ms % 1_000 / 100),
        ms if ms < 60_000 => format!("{}s", ms / 1_000),
        ms if ms < 3_600_000 => format!("{}m{:02}s", ms / 60_000, ms % 60_000 / 1_000),
        ms => format!("{}h{:02}m", ms / 3_600_000, ms % 3_600_000 / 60_000),
    }
}

/// What the marker at the head of the newest answer says this frame (§12): a run's elapsed
/// time while it is live, or the form it settled on when it stopped.
///
/// There is no third case. A frame with no marker on it holds no value of this type at all,
/// which is what says the difference between a launch no run has touched and a run that has
/// stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pulse {
    /// A run is live: the glyph its clock has reached, and how long it has been running.
    Live {
        /// Milliseconds since the run started, read by the one clock there is.
        elapsed_ms: u64,
    },
    /// A run stopped: the verdict it stopped with, and the duration it stopped at — carried
    /// here, and never read off a clock again.
    Settled {
        /// Why it ended, from the core's closed set of stop reasons.
        stop: Stop,
        /// The duration at the stop, which is what the marker keeps saying.
        elapsed_ms: u64,
    },
}

impl Pulse {
    /// The marker a live run wears at `elapsed_ms`.
    pub fn live(elapsed_ms: u64) -> Pulse {
        Pulse::Live { elapsed_ms }
    }

    /// The mark `stop` settles on, at the duration the run stopped at.
    pub fn settled(stop: &Stop, elapsed_ms: u64) -> Pulse {
        Pulse::Settled {
            stop: stop.clone(),
            elapsed_ms,
        }
    }

    /// What the marker says beside its glyph: the time so far, the duration it stopped at,
    /// and the one word a cancelled run has to say about how it ended — `cancelled 5.0s` and
    /// never a bare cross, which is a failure and not the user's own word (§4, §12).
    pub fn words(&self) -> String {
        match self {
            Pulse::Live { elapsed_ms } => format_duration(*elapsed_ms),
            Pulse::Settled {
                stop: Stop::Cancelled,
                elapsed_ms,
            } => format!("cancelled {}", format_duration(*elapsed_ms)),
            Pulse::Settled { elapsed_ms, .. } => format_duration(*elapsed_ms),
        }
    }

    /// The glyph the marker wears and the ink it is drawn in: the braille step while the run
    /// is live, and the check or the cross it settled on — the same pair a tool row ends
    /// with, and in the same two inks, because one is a thing that finished well and the
    /// other is a thing that did not (§12).
    fn mark(&self) -> (char, Ink) {
        match self {
            Pulse::Live { elapsed_ms } => (pulse_glyph(*elapsed_ms), Ink::Accent),
            Pulse::Settled {
                stop: Stop::Final, ..
            } => ('✓', Ink::Ok),
            Pulse::Settled { .. } => ('✗', Ink::Err),
        }
    }

    /// The ink the words beside the glyph are drawn in: a live run's time is a fact beside
    /// the marker it belongs to, and a settled mark is the quieter thing `muted` exists for.
    fn words_ink(&self) -> Ink {
        match self {
            Pulse::Live { .. } => Ink::Dim,
            Pulse::Settled { .. } => Ink::Muted,
        }
    }

    /// The cells the marker is drawn in: its glyph in one cell, a space, and the words
    /// beside it — one cell and at most fourteen characters, which is the whole of what the
    /// mark ever asks a frame for.
    pub fn head(&self) -> Head {
        let (glyph, ink) = self.mark();
        Head {
            spans: vec![
                (glyph.to_string(), ink),
                (format!(" {}", self.words()), self.words_ink()),
            ],
        }
    }
}

/// The marker as the cells it is drawn in, and nothing about where they go: what a row of
/// the transcript is laid out around, and what the row cache compares to know whether the
/// live entry it holds still wears the mark it was laid out with (ADR-0008).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Head {
    /// The runs of glyphs the mark is made of, left to right, each in its own ink.
    spans: Vec<(String, Ink)>,
}

impl Head {
    /// The runs of glyphs this marker is made of, each with the ink it is written in.
    pub fn spans(&self) -> &[(String, Ink)] {
        &self.spans
    }

    /// The columns the mark takes when it is drawn.
    pub fn width(&self) -> u16 {
        let columns: usize = self
            .spans
            .iter()
            .map(|(text, _)| Span::raw(text.as_str()).width())
            .sum();
        u16::try_from(columns).unwrap_or(u16::MAX)
    }

    /// The whole mark as words — `⠹ 4.2s`, `✗ cancelled 5.0s` — which is what a test reads a
    /// mark back as without a screen to paint it on.
    pub fn text(&self) -> String {
        self.spans.iter().map(|(text, _)| text.as_str()).collect()
    }
}
