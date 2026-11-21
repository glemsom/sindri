//! The terminal outside the frame: the title it wears, the mark a streaming run carries, and the
//! notification a run that ended while nobody was looking raises. None of it is a cell, so a
//! terminal that takes none of these sequences is handed the status document alone.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use sindri_core::records::{Phase, Status, Told};
use sindri_core::status::Throttle;

use sindri::link;

use crate::osc;

/// Whether a terminal takes the sequences a window is written with: the ones that name themselves,
/// and none that do not, because a title a terminal mangles is worse than the shell's own. A VTE
/// names only its version, and no version is known to have learned the mark.
pub fn speaks(term: Option<&str>, program: Option<&str>) -> bool {
    link::named(term, program, link::HYPERLINKS)
}

/// What a launch says to its terminal: the status document a status bar reads, and — where the
/// stream is a terminal that carries one — the window around the frame.
pub struct Window<W: Write> {
    stream: W,
    on: bool,
    workspace: PathBuf,
    said: Throttle,
    /// The window's own state, or nothing where this stream carries no window: a filter's stderr
    /// took no screen, so it has no title to wear and no mark to clear.
    window: Option<WindowState>,
}

/// What the window says besides the status: the state its title names, the mark it wears, and
/// whether the terminal says anyone is looking.
struct WindowState {
    state: Option<Phase>,
    marked: Option<bool>,
    focused: bool,
}

impl<W: Write> Window<W> {
    /// A telling of `workspace`'s run, written on a stream that carries a window only where its
    /// terminal takes the sequences one is drawn with.
    pub fn new(stream: W, on: bool, workspace: &Path, carries: bool) -> Window<W> {
        Window {
            stream,
            on,
            workspace: workspace.to_path_buf(),
            said: Throttle::new(),
            window: carries.then(WindowState::default),
        }
    }

    /// A `status` key turned off mid-launch stops the telling at the next report — and gives up the
    /// mark it had already set, because a telling that has gone quiet can never say the stop.
    pub fn switch(&mut self, on: bool) -> io::Result<()> {
        self.on = on;
        let Some(window) = self.window.as_mut() else {
            return Ok(());
        };
        if on || window.marked != Some(true) {
            return Ok(());
        }
        window.marked = Some(false);
        self.stream.write_all(&osc::progress(false))?;
        self.stream.flush()
    }

    /// Whether the terminal says this window is the one being read: the frame recedes with it, and
    /// a run that ends while it is away raises the one notification nobody saw.
    pub fn focused(&mut self, focused: bool) {
        if let Some(window) = self.window.as_mut() {
            window.focused = focused;
        }
    }

    pub fn say(&mut self, told: Told) -> io::Result<()> {
        if !self.on {
            return Ok(());
        }
        let report = Status::new(told, &self.workspace);
        match self.said.offer(report, Instant::now()) {
            Some(report) => self.write(&report),
            None => Ok(()),
        }
    }

    /// Says the report the pace withheld: a front end with a clock calls this whenever it
    /// wakes, so a withheld report is said late rather than lost.
    pub fn due(&mut self) -> io::Result<()> {
        if !self.on {
            return Ok(());
        }
        match self.said.due(Instant::now()) {
            Some(report) => self.write(&report),
            None => Ok(()),
        }
    }

    /// Flushed, because a terminal waiting for the rest of a sequence reads the frame as a
    /// smear.
    fn write(&mut self, report: &Status) -> io::Result<()> {
        self.stream.write_all(&osc::status(report))?;
        let window = self.window_sequences(report);
        self.stream.write_all(&window)?;
        self.stream.flush()
    }

    /// The window's own sequences for one report, and none of them twice: a title is written when
    /// the state it names has changed, the mark when a run begins or stops streaming, and the one
    /// notification a run that ended unwatched raises is written inside the state it ended in.
    fn window_sequences(&mut self, report: &Status) -> Vec<u8> {
        let Some(window) = self.window.as_mut() else {
            return Vec::new();
        };
        let phase = report.phase();
        let mut said = Vec::new();
        if window.state != Some(phase) {
            said.extend(osc::title(report.workspace(), phase.word()));
            if phase.ended() && !window.focused {
                said.extend(osc::notification(&format!(
                    "{}: {}",
                    report.workspace(),
                    report.message()
                )));
            }
            window.state = Some(phase);
        }
        let marked = phase.working();
        if window.marked != Some(marked) {
            said.extend(osc::progress(marked));
            window.marked = Some(marked);
        }
        said
    }
}

impl Default for WindowState {
    /// A window nobody has reported on is the one being read: a terminal that never says otherwise
    /// keeps the frame it has always drawn.
    fn default() -> WindowState {
        WindowState {
            state: None,
            marked: None,
            focused: true,
        }
    }
}

#[cfg(test)]
mod tests;
