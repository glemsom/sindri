use sindri_core::run::Stop;

use crate::ink::Ink;
use crate::pen;

/// Also the wait a live run gives the loop in `tui`.
pub const STEP_MS: u64 = 80;

const GLYPHS: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn pulse_glyph(elapsed_ms: u64) -> char {
    let step = (elapsed_ms / STEP_MS) % GLYPHS.len() as u64;
    GLYPHS[usize::try_from(step).unwrap_or(0)]
}

/// Truncated rather than rounded, because a run 999 ms in has not been running a second yet.
pub fn format_duration(ms: u64) -> String {
    match ms {
        ms if ms < 10_000 => format!("{}.{}s", ms / 1_000, ms % 1_000 / 100),
        ms if ms < 60_000 => format!("{}s", ms / 1_000),
        ms if ms < 3_600_000 => format!("{}m{:02}s", ms / 60_000, ms % 60_000 / 1_000),
        ms => format!("{}h{:02}m", ms / 3_600_000, ms % 3_600_000 / 60_000),
    }
}

/// A frame with no marker on it holds no value of this type: that is what says a launch no
/// run has touched apart from a run that has stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pulse {
    Live {
        elapsed_ms: u64,
    },
    /// The duration is carried here and never read off a clock again.
    Settled {
        stop: Stop,
        elapsed_ms: u64,
    },
}

impl Pulse {
    pub fn live(elapsed_ms: u64) -> Pulse {
        Pulse::Live { elapsed_ms }
    }

    pub fn settled(stop: &Stop, elapsed_ms: u64) -> Pulse {
        Pulse::Settled {
            stop: stop.clone(),
            elapsed_ms,
        }
    }

    /// A cancelled run says so: `cancelled 5.0s`, never the bare cross a failure wears.
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

    /// The check and the cross are the pair a tool row ends with, in the same two inks.
    fn mark(&self) -> (char, Ink) {
        match self {
            Pulse::Live { elapsed_ms } => (pulse_glyph(*elapsed_ms), Ink::Accent),
            Pulse::Settled {
                stop: Stop::Final, ..
            } => ('✓', Ink::Ok),
            Pulse::Settled { .. } => ('✗', Ink::Err),
        }
    }

    fn words_ink(&self) -> Ink {
        match self {
            Pulse::Live { .. } => Ink::Dim,
            Pulse::Settled { .. } => Ink::Muted,
        }
    }

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Head {
    spans: Vec<(String, Ink)>,
}

impl Head {
    pub fn spans(&self) -> &[(String, Ink)] {
        &self.spans
    }

    pub fn width(&self) -> u16 {
        self.spans
            .iter()
            .map(|(text, _)| pen::cells(text))
            .fold(0, u16::saturating_add)
    }

    pub fn text(&self) -> String {
        self.spans.iter().map(|(text, _)| text.as_str()).collect()
    }
}
