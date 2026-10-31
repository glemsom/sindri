//! The theme's ordering laws, stated as *nearness*: how far an ink sits from
//! the surface it is drawn on.
//!
//! An ordering is never a statement about brightness. "Dimmer", "a step
//! above", "follows faint" are claims about contrast against the canvas — or
//! against the panel, for a panel's ink — and because a ratio is symmetric the
//! same law holds whichever way the surface and its inks are turned: on a dark
//! canvas a nearer ink is a brighter one, on a light canvas a darker one.
//! Stated in raw luminance the laws hold on Anvil and would refuse a correct
//! light theme, so they are stated here as distance from the surface, and
//! `a_light_palette_holds_the_same_orderings` holds them to a light palette
//! that is written in this file and never shipped.

use super::ratio;
use ratatui::style::Color;
use sindri_tui::theme;

/// The inks an ordering is about, with the two surfaces they are measured
/// against: the canvas every word is written on, and the panel a fence or a
/// diff sits on.
pub struct Inks {
    /// The palette being held to the law, named for the failure message.
    name: &'static str,
    canvas: Color,
    panel: Color,
    faint: Color,
    leader: Color,
    seam: Color,
    diff_file: Color,
}

impl Inks {
    /// The theme as it ships: Anvil, on Anvil's three surfaces.
    pub fn shipped() -> Self {
        Inks {
            name: "the theme",
            canvas: theme::CANVAS,
            panel: theme::PANEL,
            faint: theme::FAINT,
            leader: theme::LEADER,
            seam: theme::SEAM,
            diff_file: theme::DIFF_FILE,
        }
    }

    /// The seam is a step above faint, which is to say further from the canvas
    /// than faint is: dots at an eighth of a cell read lighter than they are,
    /// so they are drawn at the far end of the gap.
    pub fn assert_seam_beyond_faint(&self) {
        let faint = ratio(self.faint, self.canvas);
        let seam = ratio(self.seam, self.canvas);
        assert!(
            seam > faint,
            "on {} the seam is {seam:.2}:1 from the canvas and faint is {faint:.2}:1: \
             the seam is a step beyond faint, not a step short of it",
            self.name
        );
    }

    /// A leader is dimmer than the label it bridges to, which is to say nearer
    /// the canvas than the label is.
    pub fn assert_leader_nearer_the_canvas(&self) {
        let faint = ratio(self.faint, self.canvas);
        let leader = ratio(self.leader, self.canvas);
        assert!(
            leader < faint,
            "on {} a leader is {leader:.2}:1 from the canvas and a label {faint:.2}:1: \
             a leader sits nearer the canvas than the label it bridges to",
            self.name
        );
    }

    /// A diff's file line follows faint, which is to say it sits nearer the
    /// surface it is drawn on. That surface is the panel, not the canvas: the
    /// file line is a panel's ink, so both it and faint are measured on the
    /// panel a fence and a diff are drawn on.
    pub fn assert_file_line_nearer_the_panel(&self) {
        let faint = ratio(self.faint, self.panel);
        let file = ratio(self.diff_file, self.panel);
        assert!(
            file < faint,
            "on {} a diff's file line is {file:.2}:1 from the panel and faint is {faint:.2}:1: \
             the file line follows faint as the step nearer the panel",
            self.name
        );
    }
}

/// Every ordering in both directions: a law nothing exercises against a light
/// surface is a law half-checked. This palette ships nowhere — it exists to
/// prove the laws are about nearness and not about a dark canvas. Its inks
/// meet the same floors Anvil's do (the leader clears 3:1 on its canvas, faint
/// and the file line 4.5:1 on the panel), so it is a theme that could be
/// written, refused only by the luminance phrasing.
#[test]
fn a_light_palette_holds_the_same_orderings() {
    let light = Inks {
        name: "a light palette",
        canvas: Color::Rgb(0xf4, 0xf5, 0xf7),
        panel: Color::Rgb(0xe6, 0xe9, 0xee),
        faint: Color::Rgb(0x4d, 0x56, 0x63),
        leader: Color::Rgb(0x7d, 0x85, 0x91),
        seam: Color::Rgb(0x39, 0x40, 0x4b),
        diff_file: Color::Rgb(0x52, 0x5b, 0x68),
    };
    // The floors the shipped theme holds, held here too: the point is that the
    // ordering survives being turned over, not that it is the only thing left.
    for (role, ink, surface, floor) in [
        ("a leader", light.leader, light.canvas, 3.0),
        ("faint", light.faint, light.panel, 4.5),
        ("a diff's file line", light.diff_file, light.panel, 4.5),
    ] {
        let cleared = ratio(ink, surface);
        assert!(
            cleared >= floor,
            "on {} {role} is {cleared:.2}:1, under the {floor}:1 it needs",
            light.name
        );
    }
    light.assert_seam_beyond_faint();
    light.assert_leader_nearer_the_canvas();
    light.assert_file_line_nearer_the_panel();
}
