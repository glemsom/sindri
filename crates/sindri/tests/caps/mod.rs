#![allow(dead_code)]

use std::collections::BTreeMap;

use sindri::capability::{Answers, Capabilities};
use sindri::graphics::CellSize;
use sindri::palette::Palette;

pub fn kitty() -> Capabilities {
    probed(
        &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
        Answers::at(CellSize::NOMINAL).sized(),
    )
}

pub fn pictured() -> Capabilities {
    probed(
        &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
        Answers::at(CellSize::NOMINAL),
    )
}

pub fn pointing() -> Capabilities {
    probed(
        &[("VTE_VERSION", "5000"), ("COLORTERM", "truecolor")],
        Answers::silent(),
    )
}

pub fn at(cell: CellSize) -> Capabilities {
    probed(
        &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
        Answers::at(cell),
    )
}

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

pub fn greys() -> Capabilities {
    probed(
        &[("TERM", "xterm-kitty")],
        Answers::at(CellSize::NOMINAL).sized(),
    )
}

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
