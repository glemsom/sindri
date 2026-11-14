//! What the terminal a frame draws on can do, probed once before the first frame: its cell, its
//! colours, its sizing, its pointer, its pictures and the face the config asks for over the rail.

use sindri_core::config::Environment;

use crate::graphics::CellSize;
use crate::palette::Palette;
use crate::picture;
use crate::pictures::Pictures;
use crate::pointer;
use crate::size;

/// `CSI 16 t`: asks how many pixels one cell is. A terminal that knows answers
/// `CSI 6 ; height ; width t` and one that cannot says nothing, hence the bounded wait.
const ASK_CELL: &[u8] = b"\x1b[16t";

/// Home, a cursor report, a space over a two-by-two block, and the report that answers it: a terminal
/// that drew the space at size moved its cursor two cells and one that reads no sizing moved it none.
/// The screen is erased after the answer, because the block is no cell of any frame.
const ASK_SIZING: &[u8] = b"\x1b[H\x1b[6n\x1b]66;s=2; \x1b\\\x1b[6n\x1b[2J";

/// The two questions a launch puts to the terminal it draws on, and no third.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Question {
    Cell,
    Sizing,
}

impl Question {
    /// The bytes this question is put as.
    pub fn asked(&self) -> &'static [u8] {
        match self {
            Question::Cell => ASK_CELL,
            Question::Sizing => ASK_SIZING,
        }
    }
}

/// The terminal a probe asks: the pixel size its kernel already reports, and what it says to a
/// question. A launch asks the terminal of this process; a test answers for one that is not there.
pub trait Asked {
    fn winsize(&self) -> Option<CellSize>;
    fn ask(&self, question: Question) -> Option<Vec<u8>>;
}

/// The answers a terminal gave when a launch asked it what it can do: the cell it reported and
/// whether it drew the block at size. `Asked`, so a test states them instead of a terminal.
#[derive(Clone, Copy, Debug, Default)]
pub struct Answers {
    cell: Option<CellSize>,
    sized: bool,
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
}

impl Asked for Answers {
    fn winsize(&self) -> Option<CellSize> {
        self.cell
    }

    fn ask(&self, question: Question) -> Option<Vec<u8>> {
        match question {
            // The cell is said through the kernel's own report, so these answers never carry one.
            Question::Cell => None,
            Question::Sizing => Some(match self.sized {
                true => b"\x1b[1;1R\x1b[1;3R".to_vec(),
                false => b"\x1b[1;1R\x1b[1;1R".to_vec(),
            }),
        }
    }
}

/// What the terminal a frame draws on can do, as one value.
#[derive(Clone, Debug)]
pub struct Capabilities {
    cell: CellSize,
    palette: Palette,
    sizing: bool,
    pointer: bool,
    pictures: Pictures,
    face: bool,
}

impl Default for Capabilities {
    /// A terminal nobody probed, drawn in the colours the design is drawn in.
    fn default() -> Capabilities {
        Capabilities {
            cell: CellSize::NOMINAL,
            palette: Palette::default(),
            sizing: false,
            pointer: false,
            pictures: Pictures::none(),
            face: true,
        }
    }
}

impl Capabilities {
    /// What `environment` says the terminal is and what it answered when asked, beside the keys a
    /// reader typed while it thought and the face the config asks for.
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
        let pointer = pointer::speaks(term.as_deref(), program.as_deref(), vte.as_deref());
        (
            Capabilities {
                cell,
                palette,
                sizing,
                pointer,
                pictures,
                face,
            },
            typed,
        )
    }

    /// The same value with Sindri's face drawn over the rail, or not: the one fact in it the config
    /// holds, and the one a settings visit can move while a frame is open.
    pub fn faced(self, drawn: bool) -> Capabilities {
        Capabilities {
            face: drawn,
            ..self
        }
    }

    /// The same value with the terminal reporting a pointer, or not: one terminal drawn both ways is
    /// what proves the pointer is no cell of the frame.
    pub fn reporting(self, reports: bool) -> Capabilities {
        Capabilities {
            pointer: reports,
            ..self
        }
    }

    pub fn cell(&self) -> CellSize {
        self.cell
    }

    pub fn palette(&self) -> Palette {
        self.palette
    }

    /// Whether the terminal draws text at more than one cell.
    pub fn sizes_text(&self) -> bool {
        self.sizing
    }

    pub fn reports_pointer(&self) -> bool {
        self.pointer
    }

    pub fn pictures(&self) -> &Pictures {
        &self.pictures
    }

    /// The frame places pictures here; the placements are the frame's cache.
    pub fn pictures_mut(&mut self) -> &mut Pictures {
        &mut self.pictures
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

/// The cursor reports in `bytes`, in the order the terminal wrote them, and every byte that was not
/// one of them.
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
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use super::*;

    fn terms(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (String::from(*key), String::from(*value)))
            .collect()
    }

    fn probed(environment: &BTreeMap<String, String>, asked: &impl Asked) -> Capabilities {
        Capabilities::probed(environment, asked, true).0
    }

    /// A terminal a test states: the pixel size it reports, the bytes it answers each question
    /// with, and the questions that were put to it.
    #[derive(Default)]
    struct Scripted {
        size: Option<CellSize>,
        cell: Option<&'static [u8]>,
        sizing: Option<&'static [u8]>,
        asked: RefCell<Vec<Question>>,
    }

    impl Scripted {
        fn asked(&self) -> Vec<Question> {
            self.asked.borrow().clone()
        }
    }

    impl Asked for Scripted {
        fn winsize(&self) -> Option<CellSize> {
            self.size
        }

        fn ask(&self, question: Question) -> Option<Vec<u8>> {
            self.asked.borrow_mut().push(question);
            match question {
                Question::Cell => self.cell,
                Question::Sizing => self.sizing,
            }
            .map(<[u8]>::to_vec)
        }
    }

    #[test]
    fn the_cell_is_the_one_the_terminal_reported() {
        let (capabilities, typed) =
            Capabilities::probed(&terms(&[]), &Answers::at(CellSize::new(9, 18)), true);

        assert_eq!(
            capabilities.cell(),
            CellSize::new(9, 18),
            "the cell a terminal reports is the one a picture is measured against"
        );
        assert!(typed.is_empty(), "and nothing was typed into the launch");
    }

    #[test]
    fn a_terminal_that_says_nothing_is_drawn_at_the_nominal_cell() {
        let capabilities = probed(&terms(&[]), &Answers::silent());

        assert_eq!(
            capabilities.cell(),
            CellSize::NOMINAL,
            "the nominal cell is what a terminal that will not say gets"
        );
    }

    #[test]
    fn a_terminal_that_names_kitty_is_handed_pictures_at_the_cell_it_reported() {
        let capabilities = probed(
            &terms(&[("TERM", "xterm-kitty")]),
            &Answers::at(CellSize::new(9, 18)),
        );

        assert_eq!(
            capabilities.pictures().cell(),
            Some(CellSize::new(9, 18)),
            "a terminal that names Kitty is handed the pictures its frames draw from, measured \
             at the cell it reported"
        );
    }

    #[test]
    fn a_terminal_that_names_no_protocol_is_handed_no_pictures() {
        let capabilities = probed(
            &terms(&[("TERM", "xterm-256color")]),
            &Answers::at(CellSize::NOMINAL),
        );

        assert!(
            !capabilities.pictures().draws(),
            "and one that names no protocol is handed none, which is the words of a picture and \
             no placement"
        );
    }

    #[test]
    fn the_pointer_is_the_one_the_names_vouch_for() {
        assert!(
            probed(&terms(&[("VTE_VERSION", "5000")]), &Answers::silent()).reports_pointer(),
            "a terminal that names a version reporting input reports a pointer"
        );
        assert!(
            !probed(&terms(&[("TERM", "xterm-256color")]), &Answers::silent()).reports_pointer(),
            "and one nobody can vouch for is handed a frame that takes none"
        );
    }

    #[test]
    fn the_palette_is_the_one_the_names_give() {
        let palette = |pairs: &[(&str, &str)]| probed(&terms(pairs), &Answers::silent()).palette();

        assert_eq!(
            palette(&[("COLORTERM", "truecolor")]),
            Palette::TRUECOLOR,
            "the design as it was drawn, for a terminal that says it holds the colours"
        );
        assert_eq!(
            palette(&[("TERM", "xterm-256color")]),
            Palette::ANSI,
            "and the design in the colours a terminal with no twenty-four-bit colour has"
        );
        assert_eq!(
            palette(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")]),
            Palette::SLOTS,
            "a reader who asked for no colour is given the terminal's own slots"
        );
    }

    #[test]
    fn the_keys_typed_beside_the_answers_come_back_in_the_order_they_were_typed() {
        let scripted = Scripted {
            cell: Some(b"a\x1b[6;18;9tb"),
            sizing: Some(b"c\x1b[1;1R\x1b[1;3Rd"),
            ..Scripted::default()
        };

        let (capabilities, typed) =
            Capabilities::probed(&terms(&[("TERM", "xterm-kitty")]), &scripted, true);

        assert_eq!(capabilities.cell(), CellSize::new(9, 18));
        assert_eq!(
            typed,
            b"abcd".to_vec(),
            "the keys the cell answer left out come first and the sizing answer's after them"
        );
    }

    #[test]
    fn the_face_is_the_one_the_config_asked_for() {
        let (drawn, _) = Capabilities::probed(&terms(&[]), &Answers::silent(), true);
        assert!(
            drawn.draws_face(),
            "a config that asks for the face gets it"
        );

        let (hidden, _) = Capabilities::probed(&terms(&[]), &Answers::silent(), false);
        assert!(
            !hidden.draws_face(),
            "and one that does not is drawn without it"
        );
    }

    #[test]
    fn the_face_a_settings_visit_moves_leaves_every_other_fact_where_it_was() {
        let capabilities = probed(&terms(&[("TERM", "xterm-kitty")]), &Answers::silent());
        let hidden = capabilities.clone().faced(false);

        assert!(!hidden.draws_face(), "the visit turned the face off");
        assert_eq!(hidden.cell(), capabilities.cell(), "and moved no cell");
        assert_eq!(hidden.palette(), capabilities.palette(), "and no colour");
    }

    #[test]
    fn the_pointer_a_reporting_moves_leaves_every_other_fact_where_it_was() {
        let capabilities = probed(&terms(&[("TERM", "xterm-kitty")]), &Answers::silent());
        let reporting = capabilities.clone().reporting(true);

        assert!(reporting.reports_pointer(), "the pointer was taken up");
        assert_eq!(reporting.cell(), capabilities.cell(), "and no cell moved");
        assert_eq!(
            reporting.palette(),
            capabilities.palette(),
            "and no colour moved"
        );
    }

    #[test]
    fn the_sizing_question_is_never_put_to_a_terminal_that_names_no_large_text() {
        let scripted = Scripted::default();
        let capabilities = probed(&terms(&[("TERM", "xterm-256color")]), &scripted);

        assert!(
            !capabilities.sizes_text(),
            "a terminal that names no large text is handed the spelling its glyphs are"
        );
        assert!(
            !scripted.asked().contains(&Question::Sizing),
            "and it is never asked whether it does: {:?}",
            scripted.asked()
        );
    }

    #[test]
    fn a_terminal_that_names_large_text_is_asked_and_the_answer_read() {
        let scripted = Scripted {
            sizing: Some(b"\x1b[1;1R\x1b[1;3R"),
            ..Scripted::default()
        };
        let capabilities = probed(&terms(&[("TERM", "xterm-kitty")]), &scripted);

        assert!(
            capabilities.sizes_text(),
            "a terminal that names large text moved its cursor two cells over the block"
        );
        assert_eq!(
            scripted.asked(),
            [Question::Cell, Question::Sizing],
            "the cell first, then the sizing, and no third question"
        );
    }

    #[test]
    fn a_block_drawn_at_size_moved_the_cursor_two_cells() {
        assert_eq!(
            sizing_reply(b"\x1b[1;1R\x1b[1;3R"),
            (true, Vec::new()),
            "two reports two cells apart are a terminal that drew the space at twice the cell"
        );
    }

    #[test]
    fn a_terminal_that_reads_no_sizing_moved_the_cursor_none() {
        assert_eq!(
            sizing_reply(b"\x1b[1;1R\x1b[1;1R"),
            (false, Vec::new()),
            "a terminal that read the block as one cell drew no large text"
        );
        assert_eq!(
            sizing_reply(b""),
            (false, Vec::new()),
            "and one that answered nothing is handed the spelling its glyphs are"
        );
        assert_eq!(
            sizing_reply(b"\x1b[1;1R"),
            (false, Vec::new()),
            "an answer cut in half is no answer"
        );
    }

    #[test]
    fn the_keys_typed_beside_the_sizing_answer_are_kept_in_the_order_they_were_typed() {
        assert_eq!(
            sizing_reply(b"a\x1b[1;1R\x1b[1;3Rb"),
            (true, b"ab".to_vec()),
            "the two keys that were not the answer are kept, and the answer is still read"
        );
    }

    #[test]
    fn the_cell_is_read_off_the_bytes_the_terminal_sent_and_the_keys_beside_it_kept() {
        let (cell, typed) = cell_reply(b"a\x1b[6;18;9tb");

        assert_eq!(cell, Some(CellSize::new(9, 18)), "9 wide by 18 tall");
        assert_eq!(
            typed,
            b"ab".to_vec(),
            "the two keys that were not the answer are kept, in the order they were typed"
        );

        let (cell, typed) = cell_reply(b"\x1b[6;18;9");
        assert_eq!(cell, None, "an answer cut in half is no answer");
        assert_eq!(
            typed,
            b"\x1b[6;18;9".to_vec(),
            "and every byte of it is a key"
        );
    }
}

#[cfg(test)]
pub(crate) mod caps {
    use std::collections::BTreeMap;

    use super::*;

    /// A terminal that names Kitty and read the block as one cell: pictures at the nominal cell and
    /// no large text.
    pub(crate) fn pictured() -> Capabilities {
        probed(
            &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
            Answers::silent(),
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

    /// A terminal drawn in `palette`: the names a terminal gives itself that put a frame in it.
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
