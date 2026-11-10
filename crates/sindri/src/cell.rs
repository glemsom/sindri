//! The cell-size question and its bounded wait: how many pixels one cell is, asked of a terminal
//! that may never answer. Three answers, cheapest first — the kernel's own `winsize`, the
//! `CSI 16 t` a terminal may leave unanswered, and the nominal cell rather than a launch held open.

use std::io::Read;
use std::time::Duration;

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::termios::{Winsize, tcgetwinsize};
use sindri::graphics::CellSize;

use crate::term::{Terminal, tell};

/// `CSI 16 t`: asks how many pixels one cell is. A terminal that knows answers
/// `CSI 6 ; height ; width t` and one that cannot says nothing, hence the bounded wait.
const ASK_CELL: &[u8] = b"\x1b[16t";

const REPLY_WAIT: Duration = Duration::from_millis(120);

/// The pixel size of one cell, as this launch's terminal reports it, and the bytes that were not
/// the answer and are therefore a reader's keys. Three answers, cheapest first: `winsize`, then
/// `CSI 16 t`, then the nominal cell rather than a launch held open.
pub(crate) fn probed_cell(terminal: &Terminal) -> (CellSize, Vec<u8>) {
    if let Some(cell) = tcgetwinsize(&terminal.screen).ok().and_then(of_winsize) {
        return (cell, Vec::new());
    }
    if tell(&terminal.screen, ASK_CELL).is_err() {
        return (CellSize::NOMINAL, Vec::new());
    }
    let Some(wait) = Timespec::try_from(REPLY_WAIT).ok() else {
        return (CellSize::NOMINAL, Vec::new());
    };
    let mut fds = [PollFd::new(&terminal.keys, PollFlags::IN)];
    match poll(&mut fds, Some(&wait)) {
        Ok(_) if fds[0].revents().contains(PollFlags::IN) => {}
        // Nothing arrived in the time a terminal that will answer takes: this one will not.
        _ => return (CellSize::NOMINAL, Vec::new()),
    }
    let mut read = [0u8; 64];
    let count = Read::read(&mut &terminal.keys, &mut read).unwrap_or(0);
    let (cell, typed) = cell_reply(&read[..count]);
    (cell.unwrap_or(CellSize::NOMINAL), typed)
}

/// The cell size a terminal's own `winsize` implies, when it carries pixels. A terminal that
/// reports no pixels, or fewer pixels than it has cells, has not said, and `None` says so.
fn of_winsize(size: Winsize) -> Option<CellSize> {
    let (cols, rows) = (size.ws_col, size.ws_row);
    let (xpixel, ypixel) = (size.ws_xpixel, size.ws_ypixel);
    if cols == 0 || rows == 0 || xpixel < cols || ypixel < rows {
        return None;
    }
    Some(CellSize::new(
        u32::from(xpixel / cols),
        u32::from(ypixel / rows),
    ))
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

#[cfg(test)]
mod tests {
    use std::os::fd::AsFd;

    use crate::term::Terminal;
    use crate::term::pty::Pty;

    use super::*;

    /// A terminal that fills its `winsize`'s pixel fields is asked nothing, because the kernel's
    /// own answer is the whole of what a picture needs.
    #[test]
    fn a_terminal_that_reports_its_pixels_is_asked_no_question() {
        let mut pty = Pty::open();
        pty.resize(80, 24, 800, 480);
        let terminal =
            Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");

        let (cell, typed) = probed_cell(&terminal);

        assert_eq!(
            cell,
            CellSize::new(10, 20),
            "800 by 480 pixels over 80 by 24 cells is a 10 by 20 cell"
        );
        assert!(typed.is_empty(), "and nothing was typed into the launch");
        assert!(
            pty.silent(),
            "the terminal was asked nothing at all: no sequence went out"
        );
    }

    #[test]
    fn a_terminal_that_answers_nothing_is_drawn_at_the_nominal_cell() {
        let mut pty = Pty::open();
        pty.resize(80, 24, 0, 0);
        let terminal =
            Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");

        let (cell, typed) = probed_cell(&terminal);

        assert_eq!(
            cell,
            CellSize::NOMINAL,
            "the nominal cell is what a terminal that will not say gets"
        );
        assert!(typed.is_empty(), "and no key was typed, so none was kept");
        assert!(
            pty.drawn().contains("\u{1b}[16t"),
            "the question really was put to it"
        );
    }

    /// An answer cut in half is not an answer, and every byte of it is a key a reader typed.
    #[test]
    fn the_answer_is_read_off_the_bytes_the_terminal_sent_and_the_keys_beside_it_kept() {
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
