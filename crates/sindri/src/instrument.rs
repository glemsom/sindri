use ratatui::layout::Rect;
use sindri_core::records::Usage;

use crate::ledger::{self, Kind, Row};

/// The hundredth of a block's width a shape covers: one of the hundred an instrument's image is
/// drawn at, so a share that moved is a placement of an image the terminal already holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Share(u8);

impl Share {
    pub fn of(percent: u8) -> Share {
        Share(percent.min(100))
    }

    pub fn pixels(self, width: u32) -> u32 {
        (u32::from(self.0) * width + 50) / 100
    }

    pub fn hundredth(self) -> u8 {
        self.0
    }
}

/// The context window's fill: the share of its block the run has filled, and whether it is near the
/// ceiling the frame's own meter turns its ink at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gauge {
    pub share: Share,
    pub turned: bool,
}

impl Gauge {
    pub(crate) fn of(facts: &ledger::Facts) -> Option<Gauge> {
        let fill = facts.ctx?.fill()?;
        Some(Gauge {
            share: Share::of(fill.percent()),
            turned: fill.turned(),
        })
    }
}

/// The foot's two figures, each drawn as its share of the larger, so the run's spend is read as a
/// shape and the two bars are read on one scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Spend {
    pub input: Option<Share>,
    pub output: Option<Share>,
}

impl Spend {
    pub(crate) fn of(usage: &Usage) -> Option<Spend> {
        let whole = usage.total.or_else(|| match (usage.input, usage.output) {
            (Some(input), Some(output)) => Some(input.saturating_add(output)),
            (Some(one), None) | (None, Some(one)) => Some(one),
            (None, None) => None,
        })?;
        Some(Spend {
            input: usage.input.map(|figure| share(figure, whole)),
            output: usage.output.map(|figure| share(figure, whole)),
        })
    }
}

fn share(figure: u64, whole: u64) -> Share {
    Share::of(ledger::share(figure, whole).unwrap_or(0))
}

const FOOT: [Kind; 3] = [Kind::Input, Kind::Output, Kind::Total];

/// The rows of a rail that state nothing: the air between the last row it writes and the foot it
/// spends, in the coordinates `ledger::rows` returns, and none where the rail states no foot.
pub(crate) fn bezel(rows: &[Row], width: u16) -> Option<Rect> {
    let bottom = rows
        .iter()
        .filter(|row| row.kind().is_some_and(|kind| FOOT.contains(&kind)))
        .map(Row::y)
        .min()?;
    let top = rows
        .iter()
        .filter(|row| row.y() < bottom && !row.parts().is_empty())
        .map(Row::y)
        .max()?
        .saturating_add(1);
    Some(Rect {
        x: 0,
        y: top,
        width,
        height: bottom - top,
    })
}

#[cfg(test)]
mod tests;
