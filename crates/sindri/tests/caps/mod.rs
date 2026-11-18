//! The terminals a test draws its frames on, stated rather than probed: a test crate names only the
//! terminals its own frames are drawn on, so no test needs a tty behind it.
#![allow(dead_code)]

use std::collections::BTreeMap;

use sindri::capability::{Answers, Capabilities};
use sindri::graphics::CellSize;
use sindri::palette::Palette;

/// A terminal that names Kitty and drew the block at size: it places pictures, draws large text and
/// reports a pointer.
pub fn kitty() -> Capabilities {
    probed(
        &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
        Answers::at(CellSize::NOMINAL).sized(),
    )
}

/// A terminal that places pictures at the nominal cell and draws no large text: a Kitty-named one
/// that answered it read the block as one cell.
pub fn pictured() -> Capabilities {
    probed(
        &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
        Answers::at(CellSize::NOMINAL),
    )
}

/// A terminal that reports a pointer, and nothing else.
pub fn pointing() -> Capabilities {
    probed(
        &[("VTE_VERSION", "5000"), ("COLORTERM", "truecolor")],
        Answers::silent(),
    )
}

/// A terminal that names Kitty and reported `cell`: pictures at that cell, and neither large text
/// nor a pointer.
pub fn at(cell: CellSize) -> Capabilities {
    probed(
        &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
        Answers::at(cell),
    )
}

/// A terminal drawn in `palette`: the terminal's own slots for one told no colour, and the colours
/// of a terminal with no twenty-four-bit colour for one that names none.
pub fn inked(palette: Palette) -> Capabilities {
    probed(
        match palette {
            Palette::SLOTS => &[("NO_COLOR", "1")],
            Palette::ANSI => &[("TERM", "xterm-256color")],
            _ => &[("COLORTERM", "truecolor")],
        },
        Answers::silent(),
    )
}

/// A terminal that names Kitty and was told `NO_COLOR`: it places pictures, and draws them on the
/// reader's own slots rather than on the design's colours.
pub fn uncoloured() -> Capabilities {
    probed(
        &[
            ("TERM", "xterm-kitty"),
            ("NO_COLOR", "1"),
            ("COLORTERM", "truecolor"),
        ],
        Answers::at(CellSize::NOMINAL).sized(),
    )
}

/// A terminal that names Kitty, with the config's face switch as given.
pub fn faced(face: bool) -> Capabilities {
    let terms = BTreeMap::from([
        (String::from("TERM"), String::from("xterm-kitty")),
        (String::from("COLORTERM"), String::from("truecolor")),
    ]);
    Capabilities::probed(&terms, &Answers::at(CellSize::NOMINAL).sized(), face).0
}

fn probed(terms: &[(&str, &str)], answers: Answers) -> Capabilities {
    let terms: BTreeMap<String, String> = terms
        .iter()
        .map(|(key, value)| (String::from(*key), String::from(*value)))
        .collect();
    Capabilities::probed(&terms, &answers, true).0
}
