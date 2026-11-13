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
            if matches!(phase, Phase::Done | Phase::Error) && !window.focused {
                said.extend(osc::notification(&format!(
                    "{}: {}",
                    report.workspace(),
                    report.message()
                )));
            }
            window.state = Some(phase);
        }
        let marked = phase == Phase::Working;
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
mod tests {
    use std::path::Path;

    use sindri_core::records::Told;
    use sindri_core::redact::SecretSet;
    use sindri_core::run::Stop;

    use crate::term::pty::Pty;

    use super::*;

    /// The payload of every OSC sequence the stream was handed, in the order it was handed them:
    /// what a terminal at the far end reads, with the bytes around it taken off.
    fn sequences(written: &[u8]) -> Vec<String> {
        let text = String::from_utf8_lossy(written).into_owned();
        let mut said = Vec::new();
        let mut rest = text.as_str();
        while let Some(at) = rest.find("\x1b]") {
            rest = &rest[at + 2..];
            let Some(end) = rest.find("\x1b\\") else {
                break;
            };
            said.push(rest[..end].to_owned());
            rest = &rest[end + 2..];
        }
        said
    }

    fn failed() -> Stop {
        Stop::Error(SecretSet::default().redact("the wire said no"))
    }

    #[test]
    fn a_window_names_the_workspace_and_the_state_and_writes_the_title_only_when_the_state_changes()
    {
        let mut written = Vec::new();
        {
            let mut window = Window::new(&mut written, true, Path::new("/work/sindri"), true);
            window.say(Told::Idle).expect("the state a launch opens in");
            window
                .say(Told::Asking { turn: 1 })
                .expect("the run has begun");
            window
                .say(Told::Asking { turn: 2 })
                .expect("a turn inside the same state");
            window
                .say(Told::Stopped {
                    turn: 2,
                    stop: Stop::Final,
                })
                .expect("the run answered");
        }

        let titles: Vec<String> = sequences(&written)
            .into_iter()
            .filter(|payload| payload.starts_with("2;"))
            .collect();
        assert_eq!(
            titles,
            [
                "2;sindri \u{2014} idle".to_owned(),
                "2;sindri \u{2014} working".to_owned(),
                "2;sindri \u{2014} done".to_owned(),
            ],
            "the title is the workspace and the state, and a turn inside one state names the same \
             title: a title re-said every turn is a title bar repainted for nothing"
        );
    }

    #[test]
    fn a_streaming_run_carries_a_mark_and_every_stop_clears_it() {
        let mut written = Vec::new();
        {
            let mut window = Window::new(&mut written, true, Path::new("/work/sindri"), true);
            window.say(Told::Idle).expect("before the run begins");
            window
                .say(Told::Asking { turn: 1 })
                .expect("the run streams");
            window
                .say(Told::Stopped {
                    turn: 1,
                    stop: failed(),
                })
                .expect("the run stopped");
        }

        let marks: Vec<String> = sequences(&written)
            .into_iter()
            .filter(|payload| payload.starts_with("9;4;"))
            .collect();
        assert_eq!(
            marks,
            ["9;4;0".to_owned(), "9;4;3".to_owned(), "9;4;0".to_owned()],
            "the mark is worn while the run streams and taken away by the stop: a mark left on a \
             finished run says a run is working that has ended"
        );
        assert_eq!(marks.last().map(String::as_str), Some("9;4;0"));
    }

    #[test]
    fn a_run_that_ends_unwatched_raises_one_notification_and_one_watched_raises_none() {
        let mut written = Vec::new();
        {
            let mut window = Window::new(&mut written, true, Path::new("/work/sindri"), true);
            window.focused(false);
            window.say(Told::Idle).expect("before the run begins");
            window
                .say(Told::Asking { turn: 1 })
                .expect("the run streams");
            window
                .say(Told::Stopped {
                    turn: 1,
                    stop: Stop::Final,
                })
                .expect("the run answered while nobody was looking");
            window.focused(true);
            window
                .say(Told::Asking { turn: 2 })
                .expect("the reader is back");
            window
                .say(Told::Stopped {
                    turn: 2,
                    stop: failed(),
                })
                .expect("the run failed where the reader watched it fail");
        }

        let notifications: Vec<String> = sequences(&written)
            .into_iter()
            .filter(|payload| payload.starts_with("9;") && !payload.starts_with("9;4;"))
            .collect();
        assert_eq!(
            notifications,
            ["9;sindri: answered".to_owned()],
            "one notification for the run nobody saw end, and none for the one they watched"
        );
    }

    #[test]
    fn a_telling_with_no_window_says_the_status_and_carries_no_title_or_mark() {
        let mut written = Vec::new();
        {
            let mut window = Window::new(&mut written, true, Path::new("/work/sindri"), false);
            window.focused(false);
            window.say(Told::Idle).expect("before the run begins");
            window
                .say(Told::Asking { turn: 1 })
                .expect("the run streams");
            window
                .say(Told::Stopped {
                    turn: 1,
                    stop: Stop::Final,
                })
                .expect("the run stopped");
        }

        let said = sequences(&written);
        assert!(!said.is_empty(), "the status is still told: {said:?}");
        assert!(
            said.iter().all(|payload| payload.starts_with("7501;")),
            "a stream that took no screen carries the status document and no window: {said:?}"
        );
    }

    #[test]
    fn a_telling_that_is_off_says_nothing_at_all() {
        let mut written = Vec::new();
        let mut window = Window::new(&mut written, false, Path::new("/work/sindri"), true);

        window.say(Told::Idle).expect("a window that is off");
        window.focused(false);
        window
            .say(Told::Stopped {
                turn: 1,
                stop: Stop::Final,
            })
            .expect("a stop nobody asked to hear");
        assert!(written.is_empty(), "nothing was written: {written:?}");
    }

    #[test]
    fn a_telling_says_one_state_once() {
        let mut written = Vec::new();
        {
            let mut window = Window::new(&mut written, true, Path::new("/work/sindri"), false);
            window
                .say(Told::Asking { turn: 1 })
                .expect("the first report");
            window
                .say(Told::Asking { turn: 2 })
                .expect("a turn inside the second");
            window
                .say(Told::Asking { turn: 1 })
                .expect("the state already told");
        }
        let text = String::from_utf8(written).expect("a sequence is UTF-8");

        assert_eq!(
            text.matches("\x1b]7501;").count(),
            1,
            "one state, said once: {text:?}"
        );
        assert!(text.contains("\"turn\":1"), "{text:?}");
    }

    #[test]
    fn a_telling_turned_off_gives_up_the_mark_it_had_set() {
        let mut written = Vec::new();
        {
            let mut window = Window::new(&mut written, true, Path::new("/work/sindri"), true);
            window
                .say(Told::Asking { turn: 1 })
                .expect("the run streams");
            window
                .switch(false)
                .expect("the reader turned the telling off");
        }

        let marks: Vec<String> = sequences(&written)
            .into_iter()
            .filter(|payload| payload.starts_with("9;4;"))
            .collect();
        assert_eq!(
            marks,
            ["9;4;3".to_owned(), "9;4;0".to_owned()],
            "a telling that has gone quiet can never say the stop, so the mark goes with it"
        );
    }

    #[test]
    fn a_pseudo_terminal_reads_the_title_the_mark_and_one_notification_off_the_wire() {
        let mut pty = Pty::open();
        {
            let mut window = Window::new(
                pty.slave.try_clone().expect("a handle on the terminal"),
                true,
                Path::new("/work/sindri"),
                true,
            );
            window.focused(false);
            window
                .say(Told::Asking { turn: 1 })
                .expect("the run streams");
            window
                .say(Told::Stopped {
                    turn: 1,
                    stop: Stop::Final,
                })
                .expect("the run answered");
        }

        let drawn = pty.drawn();
        assert!(
            drawn.contains("\x1b]2;sindri \u{2014} working\x1b\\"),
            "the terminal was handed the title the state began in: {drawn:?}"
        );
        assert!(
            drawn.contains("\x1b]9;4;3\x1b\\"),
            "and the mark the run wore while it streamed: {drawn:?}"
        );
        assert!(
            drawn.contains("\x1b]9;4;0\x1b\\"),
            "and the state that took the mark away: {drawn:?}"
        );
        assert_eq!(
            drawn.matches("\x1b]9;sindri: answered\x1b\\").count(),
            1,
            "one notification, off the wire a terminal reads: {drawn:?}"
        );
    }

    #[test]
    fn a_terminal_that_names_none_of_these_sequences_is_handed_no_window() {
        assert!(!speaks(None, None), "a terminal that says nothing");
        assert!(!speaks(Some("xterm-256color"), None), "an xterm");
        assert!(
            speaks(Some("xterm-kitty"), None),
            "a terminal named after one that takes them"
        );
        assert!(
            speaks(Some("xterm-256color"), Some("WezTerm")),
            "and one named by the program that runs it"
        );
    }
}
