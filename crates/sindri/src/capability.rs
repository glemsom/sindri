use sindri_core::config::Environment;

use crate::graphics::CellSize;
use crate::palette::Palette;
use crate::picture;
use crate::pictures::Pictures;
use crate::plane;
use crate::pointer;
use crate::size;

/// `CSI 16 t`: asks how many pixels one cell is. A terminal that knows answers
/// `CSI 6 ; height ; width t` and one that cannot says nothing, hence the bounded wait.
const ASK_CELL: &[u8] = b"\x1b[16t";

/// Home, a cursor report, a space over a two-by-two block, and the report that answers it: a terminal
/// that drew the space at size moved its cursor two cells and one that reads no sizing moved it none.
/// The screen is erased after the answer, because the block is no cell of any frame.
const ASK_SIZING: &[u8] = b"\x1b[H\x1b[6n\x1b]66;s=2; \x1b\\\x1b[6n\x1b[2J";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Question {
    Cell,
    Sizing,
    Animation,
}

impl Question {
    pub fn asked(&self) -> Vec<u8> {
        match self {
            Question::Cell => ASK_CELL.to_vec(),
            Question::Sizing => ASK_SIZING.to_vec(),
            Question::Animation => plane::frames_asked(),
        }
    }
}

pub trait Asked {
    fn winsize(&self) -> Option<CellSize>;
    fn ask(&self, question: Question) -> Option<Vec<u8>>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Answers {
    cell: Option<CellSize>,
    sized: bool,
    animation: bool,
}

impl Answers {
    pub fn silent() -> Answers {
        Answers::default()
    }

    pub fn at(cell: CellSize) -> Answers {
        Answers {
            cell: Some(cell),
            ..Answers::default()
        }
    }

    pub fn sized(self) -> Answers {
        Answers {
            sized: true,
            ..self
        }
    }

    pub fn animating(self) -> Answers {
        Answers {
            animation: true,
            ..self
        }
    }
}

/// The answers a terminal gives the feature queries it was asked: the parameters of each query
/// echoed back, under the identity the query named, with the payload the terminal says of them.
fn answered(asked: &[u8], said: &str) -> Vec<u8> {
    let asked = String::from_utf8_lossy(asked);
    let mut answered = String::new();
    for query in asked.split("\x1b\\").filter(|query| !query.is_empty()) {
        let params = query
            .strip_prefix("\x1b_Ga=q,")
            .expect("a feature query names no feature");
        answered.push_str(&format!("\x1b_G{params};{said}\x1b\\"));
    }
    answered.into_bytes()
}

impl Asked for Answers {
    fn winsize(&self) -> Option<CellSize> {
        self.cell
    }

    fn ask(&self, question: Question) -> Option<Vec<u8>> {
        match question {
            Question::Cell => None,
            Question::Sizing => Some(match self.sized {
                true => b"\x1b[1;1R\x1b[1;3R".to_vec(),
                false => b"\x1b[1;1R\x1b[1;1R".to_vec(),
            }),
            Question::Animation => Some(answered(
                &question.asked(),
                match self.animation {
                    true => "OK",
                    false => "ENOTSUPPORTED",
                },
            )),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Capabilities {
    cell: CellSize,
    palette: Palette,
    sizing: bool,
    pointer: bool,
    pictures: Pictures,
    animation: bool,
    face: bool,
}

impl Default for Capabilities {
    fn default() -> Capabilities {
        Capabilities {
            cell: CellSize::NOMINAL,
            palette: Palette::default(),
            sizing: false,
            pointer: false,
            pictures: Pictures::none(),
            animation: false,
            face: true,
        }
    }
}

impl Capabilities {
    pub fn probed(
        environment: &impl Environment,
        asked: &impl Asked,
        face: bool,
    ) -> (Capabilities, Vec<u8>) {
        let (term, program, vte) = (
            environment.var("TERM"),
            environment.var("TERM_PROGRAM"),
            environment.var("VTE_VERSION"),
        );
        let (cell, mut typed) = match asked.winsize() {
            Some(cell) => (cell, Vec::new()),
            None => match asked.ask(Question::Cell) {
                Some(bytes) => {
                    let (cell, keys) = cell_reply(&bytes);
                    (cell.unwrap_or(CellSize::NOMINAL), keys)
                }
                None => (CellSize::NOMINAL, Vec::new()),
            },
        };
        let palette = Palette::probed(
            environment.var("NO_COLOR").as_deref(),
            term.as_deref(),
            environment.var("COLORTERM").as_deref(),
        );
        let pictures = match picture::speaks_kitty(term.as_deref(), program.as_deref()) {
            true => Pictures::kitty(cell),
            false => Pictures::none(),
        };
        let sizing = size::speaks(term.as_deref(), program.as_deref())
            && match asked.ask(Question::Sizing) {
                Some(bytes) => {
                    let (sized, keys) = sizing_reply(&bytes);
                    typed.extend(keys);
                    sized
                }
                None => false,
            };
        let animation = pictures.draws()
            && match asked.ask(Question::Animation) {
                Some(bytes) => {
                    let (animates, keys) = animation_reply(&bytes);
                    typed.extend(keys);
                    animates
                }
                None => false,
            };
        let pointer = pointer::speaks(term.as_deref(), program.as_deref(), vte.as_deref());
        (
            Capabilities {
                cell,
                palette,
                sizing,
                pointer,
                pictures,
                animation,
                face,
            },
            typed,
        )
    }

    pub fn faced(self, drawn: bool) -> Capabilities {
        Capabilities {
            face: drawn,
            ..self
        }
    }

    pub fn reporting(self, reports: bool) -> Capabilities {
        Capabilities {
            pointer: reports,
            ..self
        }
    }

    pub fn celled(self, cell: CellSize) -> Capabilities {
        Capabilities {
            cell,
            pictures: self.pictures.celled(cell),
            ..self
        }
    }

    pub fn cell(&self) -> CellSize {
        self.cell
    }

    pub fn palette(&self) -> Palette {
        self.palette
    }

    pub fn sizes_text(&self) -> bool {
        self.sizing
    }

    pub fn reports_pointer(&self) -> bool {
        self.pointer
    }

    pub fn pictures(&self) -> &Pictures {
        &self.pictures
    }

    pub fn pictures_mut(&mut self) -> &mut Pictures {
        &mut self.pictures
    }

    pub fn animates(&self) -> bool {
        self.animation
    }

    pub fn draws_face(&self) -> bool {
        self.face
    }
}

/// The cell size a terminal's answer carries, beside the bytes that were not that answer — a key
/// typed while the launch waited is a key, not a reply.
fn cell_reply(bytes: &[u8]) -> (Option<CellSize>, Vec<u8>) {
    let mut cell = None;
    let mut typed = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if cell.is_none()
            && let Some((length, answer)) = cell_at(&bytes[at..])
        {
            cell = Some(answer);
            at += length;
            continue;
        }
        typed.push(bytes[at]);
        at += 1;
    }
    (cell, typed)
}

fn cell_at(bytes: &[u8]) -> Option<(usize, CellSize)> {
    const OPENING: &[u8] = b"\x1b[6;";
    let params = bytes.strip_prefix(OPENING)?;
    let end = params.iter().position(|byte| *byte == b't')?;
    let (height, width) = std::str::from_utf8(&params[..end]).ok()?.split_once(';')?;
    Some((
        end + OPENING.len() + 1,
        CellSize::new(width.parse().ok()?, height.parse().ok()?),
    ))
}

/// The answer the terminal's two feature reports carry, and the keys that arrived beside them: the
/// face animates only where both the frames and their animation were answered `OK`.
fn animation_reply(bytes: &[u8]) -> (bool, Vec<u8>) {
    let (mut frames, mut turns) = (false, false);
    let mut typed = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match feature_at(&bytes[at..]) {
            Some((length, keys, kept)) => {
                let supports = |key: &str| keys.split(',').any(|said| said == key);
                frames |= kept && supports("e=f") && supports("c=1") && supports("X=1");
                turns |= kept && supports("e=a");
                at += length;
            }
            None => {
                typed.push(bytes[at]);
                at += 1;
            }
        }
    }
    (frames && turns, typed)
}

fn feature_at(bytes: &[u8]) -> Option<(usize, &str, bool)> {
    const OPENING: &[u8] = b"\x1b_G";
    let params = bytes.strip_prefix(OPENING)?;
    let end = params.windows(2).position(|pair| pair == b"\x1b\\")?;
    let (keys, payload) = std::str::from_utf8(&params[..end]).ok()?.split_once(';')?;
    Some((end + OPENING.len() + 2, keys, payload == "OK"))
}

/// The answer the terminal's two cursor reports carry, and the keys that arrived beside them: the
/// cursor is two cells further along only where the block was drawn at size.
fn sizing_reply(bytes: &[u8]) -> (bool, Vec<u8>) {
    let (reports, typed) = reports(bytes);
    let sized = match reports.as_slice() {
        [before, after, ..] => after.1.saturating_sub(before.1) >= 2,
        _ => false,
    };
    (sized, typed)
}

fn reports(bytes: &[u8]) -> (Vec<(u16, u16)>, Vec<u8>) {
    let mut reports = Vec::new();
    let mut typed = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if let Some((length, report)) = report_at(&bytes[at..]) {
            reports.push(report);
            at += length;
            continue;
        }
        typed.push(bytes[at]);
        at += 1;
    }
    (reports, typed)
}

fn report_at(bytes: &[u8]) -> Option<(usize, (u16, u16))> {
    const OPENING: &[u8] = b"\x1b[";
    let params = bytes.strip_prefix(OPENING)?;
    let end = params.iter().position(|byte| *byte == b'R')?;
    let (row, column) = std::str::from_utf8(&params[..end]).ok()?.split_once(';')?;
    Some((
        end + OPENING.len() + 1,
        (row.parse().ok()?, column.parse().ok()?),
    ))
}

#[cfg(test)]
pub(crate) mod caps {
    use std::collections::BTreeMap;

    use super::*;

    pub(crate) fn pictured() -> Capabilities {
        probed(
            &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
            Answers::silent(),
        )
    }

    pub(crate) fn animating() -> Capabilities {
        probed(
            &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
            Answers::silent().animating(),
        )
    }

    pub(crate) fn sized() -> Capabilities {
        probed(
            &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
            Answers::silent().sized(),
        )
    }

    pub(crate) fn pointing() -> Capabilities {
        probed(
            &[("VTE_VERSION", "5000"), ("COLORTERM", "truecolor")],
            Answers::silent(),
        )
    }

    pub(crate) fn inked(palette: Palette) -> Capabilities {
        probed(
            match palette {
                Palette::SLOTS => &[("NO_COLOR", "1")],
                Palette::ANSI => &[("TERM", "xterm-256color")],
                _ => &[("COLORTERM", "truecolor")],
            },
            Answers::silent(),
        )
    }

    fn probed(terms: &[(&str, &str)], answers: Answers) -> Capabilities {
        let terms: BTreeMap<String, String> = terms
            .iter()
            .map(|(key, value)| (String::from(*key), String::from(*value)))
            .collect();
        Capabilities::probed(&terms, &answers, true).0
    }
}

#[cfg(test)]
mod tests;
