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

pub struct Window<W: Write> {
    stream: W,
    on: bool,
    workspace: PathBuf,
    said: Throttle,
    window: Option<WindowState>,
}

struct WindowState {
    state: Option<Phase>,
    marked: Option<bool>,
    focused: bool,
}

impl<W: Write> Window<W> {
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
