use ratatui::layout::Rect;
use sindri_core::records::Usage;

use crate::ink::Ink;
use crate::ledger::{self, Kind, Row};

/// The hundredth of a block's width a shape covers: the rung of the ladder an instrument is drawn
/// as, so a share that moved is a placement of an image the terminal already holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Share(u8);

impl Share {
    pub fn of(percent: u8) -> Share {
        Share(percent.min(100))
    }

    pub fn pixels(self, width: u32) -> u32 {
        (u32::from(self.0) * width + 50) / 100
    }

    pub fn rungs(self) -> u8 {
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
            turned: fill.ink() == Ink::Accent,
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
        let larger = usage.input.into_iter().chain(usage.output).max()?;
        Some(Spend {
            input: usage.input.map(|figure| share_of(figure, larger)),
            output: usage.output.map(|figure| share_of(figure, larger)),
        })
    }
}

fn share_of(figure: u64, larger: u64) -> Share {
    let percent = (u128::from(figure) * 100 + u128::from(larger) / 2) / u128::from(larger.max(1));
    Share::of(u8::try_from(percent).unwrap_or(100))
}

const FOOT: [Kind; 3] = [Kind::Input, Kind::Output, Kind::Total];

/// The rows of a rail that state nothing: the air between the last fact it speaks and the foot it
/// spends, in the coordinates `ledger::rows` returns, and none where the rail fills its band.
pub(crate) fn bezel(rows: &[Row], area: Rect) -> Option<Rect> {
    let mut stated: Option<u16> = None;
    let mut foot: Option<u16> = None;
    for row in rows {
        match row.kind() {
            Some(kind) if FOOT.contains(&kind) => {
                foot = Some(foot.map_or(row.y(), |at: u16| at.min(row.y())));
            }
            Some(_) => stated = Some(stated.map_or(row.y(), |at: u16| at.max(row.y()))),
            None => {}
        }
    }
    let top = stated?.saturating_add(1);
    let bottom = foot.unwrap_or_else(|| area.bottom());
    (top < bottom).then(|| Rect {
        x: area.x,
        y: top,
        width: area.width,
        height: bottom - top,
    })
}

#[cfg(test)]
mod tests;
