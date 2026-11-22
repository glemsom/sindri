use sindri_core::run::Stop;

use crate::ink::Ink;
use crate::pen;
use crate::rows::stop_mark;

pub const STEP_MS: u64 = 80;

const GLYPHS: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn step(elapsed_ms: u64) -> u64 {
    elapsed_ms / STEP_MS
}

pub fn pulse_glyph(elapsed_ms: u64) -> char {
    let step = step(elapsed_ms) % GLYPHS.len() as u64;
    GLYPHS[usize::try_from(step).unwrap_or(0)]
}

pub fn format_duration(ms: u64) -> String {
    duration(ms, "")
}

pub fn format_duration_spaced(ms: u64) -> String {
    duration(ms, " ")
}

fn duration(ms: u64, gap: &str) -> String {
    match ms {
        ms if ms < 10_000 => format!("{}.{}{gap}s", ms / 1_000, ms % 1_000 / 100),
        ms if ms < 60_000 => format!("{}{gap}s", ms / 1_000),
        ms if ms < 3_600_000 => format!("{}m{gap}{:02}s", ms / 60_000, ms % 60_000 / 1_000),
        ms => format!("{}h{gap}{:02}m", ms / 3_600_000, ms % 3_600_000 / 60_000),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pulse {
    Live { elapsed_ms: u64 },
    Settled { stop: Stop, elapsed_ms: u64 },
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

    pub fn words(&self) -> String {
        match self {
            Pulse::Live { elapsed_ms } => format_duration(*elapsed_ms),
            Pulse::Settled {
                stop: Stop::Final,
                elapsed_ms,
            } => format_duration(*elapsed_ms),
            Pulse::Settled { stop, elapsed_ms } => {
                format!("{} {}", stop.word(), format_duration(*elapsed_ms))
            }
        }
    }

    pub(crate) fn stepping(&self) -> Option<char> {
        match self {
            Pulse::Live { .. } => Some(self.mark().0),
            Pulse::Settled { .. } => None,
        }
    }

    fn mark(&self) -> (char, Ink) {
        match self {
            Pulse::Live { elapsed_ms } => (pulse_glyph(*elapsed_ms), Ink::Accent),
            Pulse::Settled { stop, .. } => stop_mark(stop),
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
