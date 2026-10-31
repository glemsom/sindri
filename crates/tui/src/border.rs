//! The composer's border as one stroke: the picture a frame draws in place of
//! box-drawing glyphs where the terminal speaks Kitty graphics, held as a pure
//! function of a rect in cells, a cell's pixel size, the ink the panel is drawn
//! in and the columns its label takes — an `image::RgbaImage` out, and nothing
//! else. No `App`, no buffer and no terminal: what the frame looks like is a
//! test's business, exactly as `seam::strip` is. Nothing here draws either;
//! making the picture for a rect and re-making it on a resize is the drawing
//! side's business, as the strip is `render/seam.rs`'s.
//!
//! The stroke is one object, not four rules: the top rule, the right vertical
//! and the bottom rule are the lit side and all end on `SEAM`, the left vertical
//! and its two corners take the panel's own ink, and every corner is cut by 45°
//! at [`CHAMFER`]. Weight is the axis pixels give back — [`QUIET`] at the quiet
//! end, ramping to [`LAMP`] over the last third so the light arrives — and the
//! only thing that antialiases is alpha: a hue is one the theme walks, whatever
//! its opacity, so a chamfered corner stays on the ramp.

use std::ops::Range;

use image::{Rgba, RgbaImage};
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::theme::{self, SEAM};

/// The stroke at the quiet end: one pixel, and no neighbour on its inside face.
pub const QUIET: f32 = 1.0;

/// The stroke at the lamp: three and four tenths of a pixel, which is what
/// light arriving earns and no more. It is the taper's far end, not a rule's
/// weight.
pub const LAMP: f32 = 3.4;

/// The share of a rule that stays at [`QUIET`]. A rule that thickens from its
/// first pixel reads as a defect; flat for two thirds and then a ramp reads as
/// light arriving.
pub const FLAT: f32 = 2.0 / 3.0;

/// How far a corner is cut, in pixels, on each axis: one number for all four,
/// and a 45° diagonal rather than an arc because cells 18 pixels tall have no
/// radius to spend.
pub const CHAMFER: u32 = 12;

/// The air the cut leaves beside the label, in pixels — on both sides of it
/// where there is rule to cut — so a word sits in a cut rather than against a
/// rule's end.
pub const PAD: u32 = 4;

/// The ink a panel is drawn in, and the two there are: the composer's whole
/// border is `PROMPT` while it holds the keyboard and `FAINT` while a run has
/// it. A panel that cannot send says so in words, and never turns its border
/// red, so these are the only stops a walk here may leave from — which is what
/// `theme::RAMPS` names.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    Prompt,
    Faint,
}

impl Ink {
    /// The theme's colour for this ink: named here, invented nowhere. The
    /// renderer that draws the glyphs walks the same two stops.
    pub(crate) fn colour(self) -> Color {
        match self {
            Ink::Prompt => theme::PROMPT,
            Ink::Faint => theme::FAINT,
        }
    }
}

/// The frame as a picture: `area`'s cells at `cell` pixels each, drawn in `ink`
/// with the top rule cut where `label` sits. `label` is a span of columns from
/// the rect's left edge — the cells the frame will draw the title in, which the
/// caller keeps clear of the chamfer, the cut following the label. A label one
/// cell clear of a 12-pixel chamfer at 8-pixel cells leaves no wall on its near
/// side, and a cut that reaches the corner is that allowance spent rather than a
/// rule left unpainted.
pub fn stroke(area: Rect, cell: (u16, u16), ink: Ink, label: Range<u16>) -> RgbaImage {
    let width = u32::from(area.width) * u32::from(cell.0);
    let height = u32::from(area.height) * u32::from(cell.1);
    if width == 0 || height == 0 {
        return RgbaImage::new(width, height);
    }
    let panel = Panel::new(width, height, ink, u32::from(cell.0), label);
    // Every pixel carries the panel's ink, the transparent ones too: a terminal
    // that resamples the picture mixes the colour of pixels the eye never sees,
    // and the default black behind the stroke would drag it grey — the reason
    // `seam::strip` paints its air as well.
    let [r, g, b] = theme::channels(ink.colour());
    let mut picture = RgbaImage::from_pixel(width, height, Rgba([r, g, b, 0]));
    for y in 0..height {
        for x in 0..width {
            if let Some(pixel) = panel.pixel(x, y) {
                picture.put_pixel(x, y, pixel);
            }
        }
    }
    picture
}

/// The frame as geometry: the strokes it is made of, and the two ramps they are
/// read against — the width they thicken across and the span they walk.
struct Panel {
    width: u32,
    /// The frame's outer faces: `0` is the far side of the first pixel and these
    /// are the far side of the last, so a pixel on either edge is half a pixel
    /// from its own line and the weight is spent inward from there.
    right: f32,
    bottom: f32,
    ink: Ink,
    chamfer: f32,
    strokes: Vec<Stroke>,
}

impl Panel {
    fn new(width: u32, height: u32, ink: Ink, cell: u32, label: Range<u16>) -> Panel {
        let right = width as f32;
        let bottom = height as f32;
        let cut = CHAMFER.min(width / 2).min(height / 2) as f32;
        let mut strokes = Vec::with_capacity(9);
        // The quiet side: the left vertical and the two corners it turns, in
        // the ink the panel itself is drawn in.
        strokes.push(Stroke::new(
            (0.0, cut),
            (0.0, bottom - cut),
            (1.0, 0.0),
            Tone::Quiet,
        ));
        strokes.push(Stroke::new((0.0, cut), (cut, 0.0), (1.0, 1.0), Tone::Quiet));
        strokes.push(Stroke::new(
            (0.0, bottom - cut),
            (cut, bottom),
            (1.0, -1.0),
            Tone::Quiet,
        ));
        // The lit side: the right vertical and both its corners, every pixel of
        // them on the seam's own ink.
        strokes.push(Stroke::new(
            (right, cut),
            (right, bottom - cut),
            (-1.0, 0.0),
            Tone::Lamp,
        ));
        strokes.push(Stroke::new(
            (right - cut, 0.0),
            (right, cut),
            (-1.0, 1.0),
            Tone::Lamp,
        ));
        strokes.push(Stroke::new(
            (right, bottom - cut),
            (right - cut, bottom),
            (-1.0, -1.0),
            Tone::Lamp,
        ));
        // The rules that walk from the one ink to the other, the top one cut
        // where the label sits so a word is never drawn over a rule.
        let (start, end) = (cut, right - cut);
        let gap = gap(label, cell, cut, end);
        for (from, to) in cut_at(start, end, gap) {
            strokes.push(Stroke::new((from, 0.0), (to, 0.0), (0.0, 1.0), Tone::Walk));
        }
        for (from, to) in cut_at(start, end, None) {
            strokes.push(Stroke::new(
                (from, bottom),
                (to, bottom),
                (0.0, -1.0),
                Tone::Walk,
            ));
        }
        Panel {
            width,
            right,
            bottom,
            ink,
            chamfer: cut,
            strokes,
        }
    }

    /// One pixel of the stroke, or `None` where nothing is drawn: the heaviest
    /// stroke covering it says both what is painted and how opaque it is.
    fn pixel(&self, x: u32, y: u32) -> Option<Rgba<u8>> {
        let at = (x as f32 + 0.5, y as f32 + 0.5);
        // The panel's own face: a corner cuts the stroke as well as the frame,
        // so nothing a cap spends past a cut is painted back onto the corner it
        // took away.
        let face = self.face(at);
        if face <= 0.0 {
            return None;
        }
        let weight = self.weight(x);
        let mut best: Option<(f32, Tone)> = None;
        for stroke in &self.strokes {
            let coverage = stroke.coverage(at, weight).min(face);
            if coverage > 0.0 && best.is_none_or(|(seen, _)| coverage > seen) {
                best = Some((coverage, stroke.tone));
            }
        }
        let (coverage, tone) = best?;
        let [r, g, b] = self.hue(tone, at.0);
        Some(Rgba([r, g, b, (coverage * 255.0).round() as u8]))
    }

    /// How much of a pixel is inside the frame's cut corners: all of it
    /// everywhere but along a corner's face, where half a pixel, which is where
    /// a 45° line falls when a pixel's centre is on it.
    fn face(&self, at: (f32, f32)) -> f32 {
        let outside = [
            self.chamfer - at.0 - at.1,
            at.0 - at.1 - (self.right - self.chamfer),
            at.1 - at.0 - (self.bottom - self.chamfer),
            at.0 + at.1 - (self.right + self.bottom - self.chamfer),
        ]
        .into_iter()
        .fold(f32::NEG_INFINITY, f32::max);
        (0.5 - outside).clamp(0.0, 1.0)
    }

    /// How thick the stroke is at column `x`: flat for the first two thirds of
    /// the frame's width, then a ramp to the lamp. A corner's weight is the
    /// weight of the column it sits in, so a rule thickens toward the seam
    /// rather than at a corner.
    fn weight(&self, x: u32) -> f32 {
        // One column has no width to ramp across, and a picture with no room for
        // a taper is the quiet end rather than the lamp.
        let along = if self.width > 1 {
            x as f32 / (self.width - 1) as f32
        } else {
            0.0
        };
        let ramp = ((along - FLAT) / (1.0 - FLAT)).clamp(0.0, 1.0);
        QUIET + (LAMP - QUIET) * ramp
    }

    /// What ink a stroke is drawn in at `x`: the quiet end's flat, the lamp's
    /// flat, or the theme's walk between them across the rules' own span — so a
    /// rule ends in exactly the colour its corner is drawn in, the corner needs
    /// no second hue, and the two rules walk together whether or not the label
    /// cuts one of them.
    fn hue(&self, tone: Tone, x: f32) -> [u8; 3] {
        match tone {
            Tone::Quiet => theme::channels(self.ink.colour()),
            Tone::Lamp => theme::channels(SEAM),
            Tone::Walk => {
                let span = self.right - 2.0 * self.chamfer;
                let along = if span > 0.0 {
                    ((x - self.chamfer) / span).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                theme::channels(theme::ramp(self.ink.colour(), SEAM, along))
            }
        }
    }
}

/// Which end of the walk a stroke belongs to.
#[derive(Clone, Copy)]
enum Tone {
    /// The ink the panel is drawn in: the left vertical and its corners.
    Quiet,
    /// The seam's own: the right vertical and both of its corners.
    Lamp,
    /// The walk between the two, one column at a time.
    Walk,
}

/// One straight run of the stroke, and the ink it is drawn in: a line on the
/// frame's outer face, spent inward from there. `inward` is the unit normal
/// pointing into the panel, which is what keeps the stroke on the panel's side
/// of a cut corner instead of straddling the diagonal and painting the corner
/// it was meant to take away.
struct Stroke {
    from: (f32, f32),
    to: (f32, f32),
    inward: (f32, f32),
    tone: Tone,
}

impl Stroke {
    fn new(from: (f32, f32), to: (f32, f32), inward: (f32, f32), tone: Tone) -> Stroke {
        let length = (inward.0 * inward.0 + inward.1 * inward.1).sqrt();
        Stroke {
            from,
            to,
            inward: (inward.0 / length, inward.1 / length),
            tone,
        }
    }

    /// How much of the pixel at `at` the stroke covers, at `weight`: the part of
    /// the pixel between the face and the weight, which is how one pixel and a
    /// 40 % neighbour read as a hairline rather than as two. Past either end of
    /// a run the stroke is measured from that end, so two runs that meet at a
    /// corner have no gap between them.
    fn coverage(&self, at: (f32, f32), weight: f32) -> f32 {
        let (dx, dy) = (self.to.0 - self.from.0, self.to.1 - self.from.1);
        let span = dx * dx + dy * dy;
        let along = if span > f32::EPSILON {
            ((at.0 - self.from.0) * dx + (at.1 - self.from.1) * dy) / span
        } else {
            0.0
        };
        if !(0.0..=1.0).contains(&along) {
            let end = if along < 0.0 { self.from } else { self.to };
            let reach = ((at.0 - end.0).powi(2) + (at.1 - end.1).powi(2)).sqrt();
            return (weight - (reach - 0.5)).clamp(0.0, 1.0);
        }
        let face = (at.0 - self.from.0) * self.inward.0 + (at.1 - self.from.1) * self.inward.1;
        (weight.min(face + 0.5) - (face - 0.5).max(0.0)).clamp(0.0, 1.0)
    }
}

/// The cut the label makes in the top rule, in pixels and clamped to the rule:
/// the label's cells with a little air on each side wherever there is rule to
/// give it.
fn gap(label: Range<u16>, cell: u32, start: f32, end: f32) -> Option<(f32, f32)> {
    if label.is_empty() {
        return None;
    }
    let lo = (u32::from(label.start) * cell).saturating_sub(PAD) as f32;
    let hi = (u32::from(label.end) * cell + PAD) as f32;
    (lo <= end && hi >= start).then(|| (lo.max(start), hi.min(end)))
}

/// The pieces of a rule left after cutting `gap` out of it: one run where the
/// cut is off the rule or none was asked for, two where it lands inside.
fn cut_at(start: f32, end: f32, gap: Option<(f32, f32)>) -> Vec<(f32, f32)> {
    if start >= end {
        return Vec::new();
    }
    let Some((lo, hi)) = gap else {
        return vec![(start, end)];
    };
    let mut pieces = Vec::with_capacity(2);
    if lo > start {
        pieces.push((start, lo));
    }
    if hi < end {
        pieces.push((hi, end));
    }
    pieces
}
