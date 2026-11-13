//! The text-sizing question and the bounded wait for its answer, asked before the first frame is
//! painted: a heading is measured by the cells the sizing leaves, and a terminal that says nothing
//! keeps the spelling its glyphs are.

use crate::term::{Terminal, reply};

/// Home, a cursor report, a space over a two-by-two block, and the report that answers it: a terminal
/// that drew the space at size moved its cursor two cells and one that reads no sizing moved it none.
/// The screen is erased after the answer, because the block is no cell of any frame.
const ASK: &[u8] = b"\x1b[H\x1b[6n\x1b]66;s=2; \x1b\\\x1b[6n\x1b[2J";

/// Whether this terminal draws text at more than one cell, beside the bytes that were not its
/// answer and are therefore a reader's keys.
pub(crate) fn probed_sizing(terminal: &Terminal) -> (bool, Vec<u8>) {
    match reply(terminal, ASK) {
        None => (false, Vec::new()),
        Some(bytes) => sizing_reply(&bytes),
    }
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
    use std::os::fd::AsFd;

    use crate::term::Terminal;
    use crate::term::pty::Pty;

    use super::*;

    #[test]
    fn a_terminal_that_drew_the_block_at_size_moved_the_cursor_two_cells() {
        assert_eq!(
            sizing_reply(b"\x1b[1;1R\x1b[1;3R"),
            (true, Vec::new()),
            "two reports two cells apart are a terminal that drew the space at twice the cell"
        );
    }

    #[test]
    fn a_terminal_that_reads_no_sizing_moved_the_cursor_none_and_says_nothing_is_no_answer() {
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
    fn the_keys_typed_beside_the_answer_are_kept_in_the_order_they_were_typed() {
        assert_eq!(
            sizing_reply(b"a\x1b[1;1R\x1b[1;3Rb"),
            (true, b"ab".to_vec()),
            "the two keys that were not the answer are kept, and the answer is still read"
        );
    }

    #[test]
    fn a_terminal_that_answers_nothing_is_asked_and_reads_no_large_text() {
        let mut pty = Pty::open();
        pty.resize(80, 24, 0, 0);
        let terminal =
            Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to ask");

        let (sized, typed) = probed_sizing(&terminal);

        assert!(!sized, "a terminal that says nothing sizes no text");
        assert!(typed.is_empty(), "and no key was typed, so none was kept");
        assert!(
            pty.drawn().contains("\x1b]66;s=2; \x1b\\"),
            "the question really was put to it"
        );
    }
}
