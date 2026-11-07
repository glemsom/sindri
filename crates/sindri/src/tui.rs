//! The terminal UI's contract with the terminal it draws on: the alternate screen it
//! takes and gives back, raw mode with flow control off, the keys it reads and the
//! sequences it writes back, and a refusal at exit 2 when there is no terminal to do
//! any of that on (§3).
//!
//! The terminal itself is the one thing this crate owns and the core may not name
//! (ADR-0005): the alternate screen, raw mode, the keys read off the tty and the
//! sequences written back to it all live in this module, and what a frame holds is
//! the core's business rather than the terminal's.
//!
//! The loop is a loop that waits. One frame is painted when the screen is taken — the
//! boot frame, empty transcript and all (§12) — and after it a key arrives as bytes
//! from `read`, so a launch with nobody at the keyboard writes nothing further: there
//! is no timer, no tick and no repaint, which is what §12's "an idle frame is never
//! drawn" means as code. The one exception is a run in flight: the marker at the head of
//! its newest answer is a clock, so while a run is live the wait is one step of the pulse
//! and a frame that ran out of time is a frame whose marker has moved. The run stopping is
//! what ends it — the settled marker is drawn once and the wait goes back to forever (#39).
//! A key that arrives is decoded here, because a key is more than
//! a byte — `Shift+Enter` is `CSI 13;2u` under the protocol §17 assumes — and a paste is
//! more than a key: it is a burst of bytes, or a bracketed paste (`ESC[200~ … ESC[201~`)
//! when the terminal offers one, and it reaches the composer as one edit with its line
//! breaks kept.
//!
//! A launch also says what it is doing, over OSC 7501 and on the terminal it owns: `idle`
//! while nothing is in flight, the turn count while a run asks, and the stop reason when it
//! lands (§14). The pace a report is told at is the core's, the writing is this module's,
//! and the frame's own step is the clock a withheld report waits for.
//!
//! A prompt that leaves the composer starts a run, and the run is driven on a thread of its
//! own: the loop must stay asleep in `poll` for a key while a request is in flight, so the
//! fragments the wire lands are woken into this loop through the same pipe a signal uses
//! and painted as they arrive (§4, §12). Nothing of what a run decides is here — it is one
//! `launch`, assembled and driven the way batch mode's is — and what is here is the
//! terminal's half: when to paint, and when to give the screen back.
//!
//! A prompt sent while a run is live cannot be answered now, so it waits in the frame's queue
//! above the composer and runs once that run stops, in the order it was typed (§12): `Enter`
//! is never a key that throws a prompt away, which is the one thing a reader would take as
//! the product losing their words.
//!
//! Two of the `/` commands are the terminal's to finish rather than the frame's (§12): the
//! model name a screen chose is written into the config through the one owner-only save
//! (ADR-0009), and the names the `/model` screen offers are asked of the wire — a request,
//! so on a thread of its own, landing in the frame the way a run's fragments land. What a
//! command means is not decided here: the frame reads the line and hands back an outcome,
//! and this is the half that owns a socket and a file.
//!
//! `Ctrl-C` and a `SIGINT` from outside are one path. Raw mode means the terminal sends `^C`
//! as a byte and no signal, so the two arrive at different doors — the decoder here and the
//! handler in `signal` — and both set the run's own cancellation flag, which stops the run
//! when the request in flight does and kills the command it is in flight with (§4). The
//! draft stays where it was on either, because a prompt half-written when a run dies is the
//! one the reader was writing (§12).

use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::panic::{self, PanicHookInfo};
use std::process::ExitCode;
use std::sync::{Arc, Mutex, MutexGuard, Once, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ratatui::backend::CrosstermBackend;
use rustix::event::Timespec;
use rustix::event::{PollFd, PollFlags, poll};
use rustix::termios::{
    InputModes, OptionalActions, Termios, Winsize, tcgetattr, tcgetwinsize, tcsetattr,
};
use sindri::composer::{Key, Outcome, Prompt, State};
use sindri::graphics::CellSize;
use sindri::picture;
use sindri::pictures::Pictures;
use sindri::pulse::{self, Pulse};
use sindri::settings::persist;
use sindri::view::View;
use sindri_core::config::{Config, Environment, Process, Touched};
use sindri_core::provider::Deltas;
use sindri_core::provider::served::Listing;
use sindri_core::records::Told;
use sindri_core::redact::SecretSet;
use sindri_core::run::{Cancel, Exchange, Stop};
use sindri_core::tools::FrontEnd;

use crate::cli::Cli;
use crate::launch::{Launch, Refusal, Turns};
use crate::osc::Teller;
use crate::signal::{Signals, Wake};

/// Spec §15's usage error: a launch that cannot be a run, a screen a launch cannot
/// draw on.
const USAGE: u8 = 2;

/// Spec §15's setup failure: a terminal that will not take the screen or will not
/// give a key back.
const SETUP: u8 = 3;

/// Spec §15's cancellation: a launch the user's own word ended, which is the code a
/// `SIGINT` out of nowhere has always meant.
const CANCELLED: u8 = 130;

/// Take the alternate screen, hide the cursor so the first frame is not drawn around a
/// caret sitting wherever the last shell left it, and ask for bracketed paste — so a
/// pasted prompt arrives as one sequence and not as a burst of keystrokes (§12).
const TAKE_SCREEN: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?2004h";

/// Give the alternate screen back, stop asking for bracketed paste first, and put the
/// cursor back last, so a shell gets a terminal that looks the way it did.
const GIVE_BACK_SCREEN: &[u8] = b"\x1b[?2004l\x1b[?25h\x1b[?1049l";

/// §11's key for the model a run asks: the one row `/model` writes, named here so the write
/// reads as the schema's key rather than as a word that happens to match it.
const MODEL: &str = "model";

/// `CSI 16 t`: the one question a launch puts to its terminal, asking how many pixels one
/// cell of it is. A terminal that knows answers `CSI 6 ; height ; width t`; one that does
/// not says nothing at all, which is why the wait below is bounded (§12, ADR-0003).
const ASK_CELL: &[u8] = b"\x1b[16t";

/// How long the terminal is given to answer the cell question. A terminal that answers
/// takes microseconds and one that cannot never will, so this is only ever the cost of a
/// terminal that says nothing — and it is paid once, before the first frame.
const REPLY_WAIT: Duration = Duration::from_millis(120);

/// Hands `view` the pictures its terminal places, decided from the two things the terminal
/// says it is: one that speaks Kitty is handed the pictures its frames draw from, and one that
/// does not is handed none, which is what leaves it the words a picture is spoken of as (§12,
/// ADR-0003).
///
/// `term` and `program` are `TERM` and `TERM_PROGRAM`, read off the environment by the launch
/// and never off the tty: the launch has spent its one round trip on the cell size, and a
/// second question before the first frame would buy a fact that decides only whether a picture
/// is drawn. They are arguments rather than reads because a test has no environment to set: it
/// hands the function the two facts and reads the frame it gets back.
fn hand_pictures(view: &mut View, cell: CellSize, term: Option<&str>, program: Option<&str>) {
    view.set_pictures(match picture::speaks_kitty(term, program) {
        true => Pictures::kitty(cell),
        false => Pictures::none(),
    });
}

/// The terminal UI: takes the screen, holds it until `Ctrl-Q` or the user's own word, gives
/// it back.
pub fn run(cli: &Cli, config: &Config) -> ExitCode {
    if let Some(reason) = missing_terminal() {
        return crate::refuse(&reason, USAGE);
    }
    // The signal door is opened before the screen is taken: a `SIGINT` that arrives between
    // raw mode and the loop has to be a cancellation and not a process dying with the
    // terminal left raw and the alternate screen up (§4, §12).
    let signals = match Signals::install() {
        Ok(signals) => signals,
        Err(error) => {
            return crate::refuse(
                &format!("the signal pipe could not be made: {error}"),
                SETUP,
            );
        }
    };
    let terminal = match Terminal::stdio() {
        Ok(terminal) => terminal,
        Err(error) => {
            return crate::refuse(&format!("the terminal could not be taken: {error}"), SETUP);
        }
    };
    let screen = match Screen::take(&terminal) {
        Ok(screen) => screen,
        Err(error) => {
            return crate::refuse(&format!("the terminal would not go raw: {error}"), SETUP);
        }
    };
    // The pixels in one cell, asked of the terminal once and kept by the frame for the length
    // of the launch: every picture this screen measures is measured against it (§12,
    // ADR-0003). A launch with no terminal never reaches this line, which is what keeps the
    // round trip out of batch mode. A key typed while the launch waited is handed on to the
    // loop rather than dropped: a reader typing at a blank screen did not ask for their words
    // to go missing.
    let (cell_size, typed) = probed_cell(&terminal);
    let mut view = View::boot();
    view.set_cell_size(cell_size);
    // And how its terminal places a picture, read off the environment once, before the first
    // frame.
    let named = |key: &str| std::env::var(key).ok();
    hand_pictures(
        &mut view,
        cell_size,
        named("TERM").as_deref(),
        named("TERM_PROGRAM").as_deref(),
    );
    // The frame is told the directory the launch stands in, because `@` walks the workspace
    // and a frame knows only what it was handed. It is the same directory `Launch::open`
    // gives a run — read the same way, from the process — so a path a reader picks is a path
    // the run's own commands can read (§12).
    let workspace = match std::env::current_dir() {
        Ok(workspace) => workspace,
        Err(error) => {
            return crate::refuse(
                &format!("cannot read the working directory: {error}"),
                SETUP,
            );
        }
    };
    view.set_workspace(workspace.clone());
    // And the home the user-scope skills are read from, for the same reason: a name
    // `/skill:name` accepts and a name a run could call have to come from one catalog, which
    // is why the directory is read the way `launch` reads it and not read again here (§7).
    view.set_home(crate::launch::home());
    let mut canvas = match canvas(&terminal) {
        Ok(canvas) => canvas,
        Err(error) => {
            return crate::refuse(&format!("the frame could not be painted: {error}"), SETUP);
        }
    };
    if let Err(error) = paint(&mut canvas, &mut view) {
        return crate::refuse(&format!("the frame could not be painted: {error}"), SETUP);
    }
    // §14: what the run is doing, said on the terminal this module owns. One teller for the
    // whole launch and not one per run, so the pace a status is told at is the launch's; the
    // screen is a second handle on the same descriptor a frame is drawn on, and `idle` is
    // what a launch with no run in flight is.
    let mut telling = match terminal.screen.try_clone() {
        Ok(screen) => Teller::new(screen, config.tells_status(), &workspace),
        Err(error) => {
            return crate::refuse(&format!("the terminal could not be told: {error}"), SETUP);
        }
    };
    if let Err(error) = telling.say(Told::Idle) {
        return crate::refuse(&format!("the status could not be written: {error}"), SETUP);
    }
    // The config this launch hands each run, which a visit to the settings modal replaces: the
    // file is written once and resolved again, so the prompt after the visit runs on it.
    let mut config = config.clone();
    let keys = keys(
        &terminal,
        &mut canvas,
        &mut view,
        cli,
        &mut config,
        &Process,
        &signals,
        &typed,
        &mut telling,
    );
    // The screen is given back before a word is said about how the launch ended: a refusal
    // is a line for the shell a reader is handed back to, and not a smear over a frame.
    drop(screen);
    match keys {
        Ok(Ending::Quit) => ExitCode::SUCCESS,
        Ok(Ending::Cancelled) => ExitCode::from(CANCELLED),
        Ok(Ending::Refused(refusal)) => crate::refuse(refusal.message(), refusal.code()),
        Err(error) => crate::refuse(&format!("the terminal could not be read: {error}"), SETUP),
    }
}

/// How a launch ended: the ways the loop stops, and the one thing a prompt can be refused.
enum Ending {
    /// `Ctrl-Q`, or a terminal that went away and left nothing to quit from: the user's own
    /// key, and the exit code a key of theirs has.
    Quit,
    /// The user's word from outside the keyboard: the run it stopped has ended, the screen is
    /// given back, and §15's cancellation code is the launch's.
    Cancelled,
    /// A prompt that could not start a run at all: the reason is said once the screen is back,
    /// because a launch with no run to draw has no frame to say it on.
    Refused(Refusal),
}

/// What a launch with no prompt is short of when it is short of a terminal: the
/// stream that is not one, and the thing a person can do about it.
fn missing_terminal() -> Option<String> {
    if !io::stdout().is_terminal() {
        return Some(String::from(
            "the terminal UI needs a terminal to draw on, and stdout is not one: \
             give a prompt with -b to run as a filter",
        ));
    }
    if !io::stdin().is_terminal() {
        return Some(String::from(
            "the terminal UI needs a terminal to read keys from, and stdin is not one: \
             give a prompt with -b to run as a filter",
        ));
    }
    None
}

/// The terminal this front end draws on, held as the two fds it really is — the one
/// keys arrive on and the one frames leave by — so a test can hand in a
/// pseudo-terminal and read what a person would have seen.
struct Terminal {
    /// Where keys come from: the terminal's input side, whose termios is raw mode.
    keys: File,
    /// Where frames go: the terminal's output side, the alternate screen.
    screen: File,
}

impl Terminal {
    /// This process's own standard input and output, which `run` has already
    /// refused to build a UI on unless both are terminals.
    fn stdio() -> io::Result<Terminal> {
        Terminal::pair(io::stdin().as_fd(), io::stdout().as_fd())
    }

    /// A terminal from the two ends a caller names, taken as copies so this value
    /// owns no descriptor the rest of the process still reads and writes.
    fn pair(keys: BorrowedFd<'_>, screen: BorrowedFd<'_>) -> io::Result<Terminal> {
        Ok(Terminal {
            keys: File::from(keys.try_clone_to_owned()?),
            screen: File::from(screen.try_clone_to_owned()?),
        })
    }
}

/// The screen a launch holds: taking one is what puts the terminal in raw mode, and
/// dropping it is what gives the terminal back. A guard rather than two calls, so
/// there is no path — a `?`, an early return, a panic — that can forget half.
struct Screen;

impl Screen {
    /// Takes the alternate screen and raw mode together, remembering the state that
    /// replacing them is a change to.
    fn take(terminal: &Terminal) -> io::Result<Screen> {
        guard_panics();
        let saved = tcgetattr(&terminal.keys)?;
        let mut raw = saved.clone();
        raw.make_raw();
        // `make_raw` clears flow control already, and this line stays because the
        // day raw mode is spelled differently is the day `Ctrl-Q` stops arriving.
        raw.input_modes.remove(InputModes::IXON);
        tcsetattr(&terminal.keys, OptionalActions::Now, &raw)?;

        // Both halves are handed to the one place a panic hook can reach, so the
        // screen is given back even when the unwinding that follows runs no guard.
        let restore = Restore {
            keys: terminal.keys.try_clone()?,
            screen: terminal.screen.try_clone()?,
            saved,
        };
        *lock(&GIVE_BACK) = Some(restore);
        if let Err(error) = tell(&terminal.screen, TAKE_SCREEN) {
            give_back();
            return Err(error);
        }
        Ok(Screen)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        give_back();
    }
}

/// The screen frames are painted through: ratatui's canvas over this terminal's output
/// side. It is taken once, because a canvas that remembered nothing would rewrite every
/// cell of the frame on every key, and a frame is drawn once per change rather than
/// whole.
type Canvas<'a> = ratatui::Terminal<CrosstermBackend<&'a File>>;

/// A canvas over this terminal's output side, remembering what it last drew.
fn canvas(terminal: &Terminal) -> io::Result<Canvas<'_>> {
    ratatui::Terminal::new(CrosstermBackend::new(&terminal.screen))
}

/// The same canvas at a size already known, so nothing is asked of the operating system. A test
/// holds both ends of a pseudo-terminal whose size it set itself, so the answer is the test's to
/// give: crossterm answers a size question by opening `/dev/tty` and, with none to open, by
/// running `tput` — and under a suite's parallel threads that `fork` can be refused for want of
/// process slots, which arrives here as a terminal that could not be measured rather than as the
/// resource it was (§16).
#[cfg(test)]
fn canvas_at(terminal: &Terminal, area: ratatui::layout::Rect) -> io::Result<Canvas<'_>> {
    ratatui::Terminal::with_options(
        CrosstermBackend::new(&terminal.screen),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(area),
        },
    )
}

/// Paints the frame the view holds now: at launch the boot frame, and afterwards one
/// frame per key that changed what it holds.
///
/// It goes through `View::paint` — the seam the fixture harness renders through
/// (ADR-0012) — onto this terminal's own output side, so what a reader sees and what a
/// golden frame holds are one picture and not two. The caret is the terminal's own
/// cursor and never a cell of the frame: an ink names a foreground and nothing else, so
/// a caret drawn as a cell would be a tinted letter and nothing at all on a space.
fn paint(canvas: &mut Canvas<'_>, view: &mut View) -> io::Result<()> {
    canvas.draw(|frame| {
        view.paint(frame.area(), frame.buffer_mut());
        if let Some(at) = view.cursor(frame.area()) {
            frame.set_cursor_position(at);
        }
    })?;
    Ok(())
}

/// Reads the terminal until the launch ends, one event at a time, and repaints when an
/// event changed what the frame holds.
///
/// The loop waits in `poll` and paints nothing while it waits: a launch with nobody at the
/// keyboard asks the kernel for bytes and sleeps, so an idle frame is not skipped, it is
/// never drawn (§12). Three things can end that wait — a key on the terminal, a signal, and
/// a run with something to say — and all three are fds, so one wait covers them and no timer
/// is needed to make a fragment, a cancellation or a keystroke arrive.
///
/// `typed` is what the cell probe read off the terminal before the loop started: a reader's
/// keys, answered here rather than left in a buffer nothing reads. `telling` is what the run
/// in flight says about itself, written by the one thing here that owns a tty (§14).
#[allow(
    clippy::too_many_arguments,
    reason = "the one seam every mode's key is handed through, plus the teller the run is announced to"
)]
fn keys(
    terminal: &Terminal,
    canvas: &mut Canvas<'_>,
    view: &mut View,
    cli: &Cli,
    config: &mut Config,
    environment: &impl Environment,
    signals: &Signals,
    typed: &[u8],
    telling: &mut Teller<File>,
) -> io::Result<Ending> {
    let mut decoder = Decoder::new();
    let mut read = [0u8; 4096];
    let mut running: Option<Running> = None;
    let mut asking: Option<Asking> = None;
    let mut interrupted = false;
    let wake = signals.wake();

    // A key the cell probe read is a key: it arrived before the first frame was painted, and
    // the loop answers it here rather than waiting for a second one that may never come.
    decoder.push(typed);
    while let Some(event) = decoder.next() {
        if let Some(ending) = answer(
            terminal,
            canvas,
            view,
            event,
            &mut running,
            &mut asking,
            cli,
            config,
            environment,
            &wake,
            telling,
        )? {
            return Ok(ending);
        }
    }

    loop {
        let waiting = wake.reader();
        let mut fds = [
            PollFd::new(&terminal.keys, PollFlags::IN),
            PollFd::new(&waiting, PollFlags::IN),
        ];
        // A run in flight is the only clock this loop has: its marker steps once per 80 ms,
        // so the wait is that long while one is live and forever while none is. Nothing is
        // drawn for a wait nobody owes a frame for (§12).
        let step = running
            .is_some()
            .then(|| Timespec::try_from(Duration::from_millis(pulse::STEP_MS)).ok())
            .flatten();
        match poll(&mut fds, step.as_ref()) {
            // A signal is not a failure: the pipe it pings says what arrived.
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(std::io::Error::from(error)),
        }
        let keyed = fds[0].revents().contains(PollFlags::IN);
        let woken = fds[1].revents().contains(PollFlags::IN);

        // §14's second has a clock here and nowhere else: a report the pace withheld is said
        // the moment its window is up, so a withheld report is late rather than lost.
        telling.due()?;

        // A live run's marker is read off the run's own clock once per pass, so the frame a
        // step draws, the frame a key draws and the frame a fragment draws all carry the time
        // the run is at rather than the time it was at when the pulse was last asked for.
        if let Some(flight) = &running {
            view.set_pulse(Some(Pulse::live(flight.elapsed_ms())));
        }

        // The user's own word, from outside the keyboard: the run it stops is the run
        // `Ctrl-C` stops, and this launch ends once that run has (§4). It is read every pass
        // and not only on the pass the pipe woke, because the word is a flag that is never
        // cleared and the byte is only what wakes the loop: a `SIGINT` that landed while the
        // loop was awake for something else has already raised a byte a drain may have taken.
        if signals.arrived() && !interrupted {
            interrupted = true;
            if let Some(flight) = &running {
                flight.cancel();
            }
        }

        if woken {
            wake.drain();
            served(canvas, view, &mut asking)?;
            if take(canvas, view, &mut running, telling)? && !interrupted {
                // The run stopped, so the frame's composer is idle again (§12) and the next
                // prompt the reader typed while it was live is the run to start. A launch the
                // user's own word ended starts nothing: the queue waits for a reader who
                // still wants it.
                if let Some(ending) =
                    resume(canvas, view, &mut running, cli, config, &wake, telling)?
                {
                    return Ok(ending);
                }
            }
        }

        // A run the user's word stopped has ended: a launch that stayed would be a screen left
        // over a run that is not coming back, so the terminal is given back as soon as there
        // is nothing left to stop (§4, §12).
        if interrupted && running.is_none() {
            return Ok(Ending::Cancelled);
        }

        if keyed {
            let count = Read::read(&mut &terminal.keys, &mut read)?;
            // The terminal went away: there is nothing left to quit from, and the screen is
            // still given back by the guard.
            if count == 0 {
                return Ok(Ending::Quit);
            }
            decoder.push(&read[..count]);
            while let Some(event) = decoder.next() {
                if let Some(ending) = answer(
                    terminal,
                    canvas,
                    view,
                    event,
                    &mut running,
                    &mut asking,
                    cli,
                    config,
                    environment,
                    &wake,
                    telling,
                )? {
                    return Ok(ending);
                }
            }
        }

        // The wait ran out with nothing to say: a run is live and its marker has stepped, so
        // the frame that drew the last glyph is a frame that is now wrong. This is the only
        // frame this loop ever draws for no event but the clock, and it is owed to a run
        // nobody is watching idle (§12).
        if !keyed && !woken && running.is_some() {
            paint(canvas, view)?;
        }
    }
}

/// The run in flight: the flag the user's word lands on, the reports the loop has not taken
/// yet, and the thread that will end — held together so one `Ctrl-C` and one `SIGINT` reach
/// the same three.
struct Running {
    /// The cancellation the loop reads and a command in flight hears (§4).
    cancel: Cancel,
    /// What the run has said that the frame has not been told.
    pending: Arc<Mutex<Vec<Report>>>,
    /// The thread driving it to a stop reason.
    thread: JoinHandle<()>,
    /// When the run started, which is where the marker's elapsed time is measured from.
    started: Instant,
}

impl Running {
    /// Drives `prompt` in its own thread: the fragment a wire lands is pushed where the loop
    /// takes it and the loop is pinged through the pipe a signal uses, so a run's words
    /// arrive as a keystroke does and nothing polls for them.
    fn start(launch: Launch, prompt: Prompt, wake: Arc<Wake>) -> Running {
        let cancel = Cancel::new();
        let pending = Arc::new(Mutex::new(Vec::new()));
        // Two handles on one list of reports: what the wire lands, and the turns that end,
        // both of which the loop takes and only this thread knows about.
        let mut reports = Reports {
            pending: Arc::clone(&pending),
            wake: Arc::clone(&wake),
        };
        let mut turns = Reports {
            pending: Arc::clone(&pending),
            wake,
        };
        let interrupted = cancel.clone();
        let started = Instant::now();
        let thread = thread::spawn(move || {
            // A launch that could not report a run at all is a run that ended with no stop
            // reason, which the loop cannot do; the marker settles on it as a failure and
            // says so rather than turning for a run nobody is driving.
            let (turns, stop) =
                match launch.drive(prompt.text(), &mut reports, &mut turns, interrupted) {
                    Ok(outcome) => (outcome.turns(), outcome.stopped().clone()),
                    Err(refusal) => (
                        0,
                        Stop::Error(SecretSet::default().redact(refusal.message())),
                    ),
                };
            // The duration is read here, where the run stopped, and carried to the frame: a
            // frame that read the clock when it painted would put a different number beside
            // the same verdict a minute later (§12).
            let elapsed_ms = millis(started.elapsed());
            reports.push(Report::Stopped {
                stop,
                elapsed_ms,
                turns,
            });
        });
        Running {
            cancel,
            pending,
            thread,
            started,
        }
    }

    /// The user's word: the run stops when the request in flight does, and the command it is
    /// in flight with is killed now (§4).
    fn cancel(&self) {
        self.cancel.now();
    }

    /// How long this run has been going, which is the number its live marker is painted from.
    fn elapsed_ms(&self) -> u64 {
        millis(self.started.elapsed())
    }
}

/// A span of time as whole milliseconds, which is the unit §12's durations are said in.
fn millis(span: Duration) -> u64 {
    u64::try_from(span.as_millis()).unwrap_or(u64::MAX)
}

/// One thing the run thread has to say to the frame: a fragment of the turn in flight, which
/// the frame draws where it lands, and the end of the run, which only the loop needs.
enum Report {
    /// A fragment of the answer.
    Text(String),
    /// A fragment of the reasoning, which has its own gutter and ink (§4, §12).
    Reasoning(String),
    /// What the run is doing, in §14's five words and the facts they are built from: that
    /// it is asking the model in this turn, which the frame has nothing to draw but a
    /// terminal wants to know.
    Told(Told),
    /// The run stopped, and with what: the marker settles on the verdict and the duration
    /// the run reported, which are the two facts §12 settles a pulse on (#39).
    Stopped {
        /// Why the run ended, from the core's closed set of stop reasons.
        stop: Stop,
        /// How long it had been running when it stopped, read at the stop itself.
        elapsed_ms: u64,
        /// How many turns it had started, which is the count its last status names.
        turns: u32,
    },
}

/// The run's own door into the frame, as the run thread holds it: every fragment lands in the
/// list the loop takes from, and the loop is pinged once so what landed is painted.
struct Reports {
    /// What has landed and not been taken.
    pending: Arc<Mutex<Vec<Report>>>,
    /// The pipe the loop waits on.
    wake: Arc<Wake>,
}

impl Reports {
    /// Leaves `report` for the loop and wakes it.
    fn push(&mut self, report: Report) {
        lock(&self.pending).push(report);
        self.wake.raise();
    }
}

impl Deltas for Reports {
    fn text(&mut self, fragment: &str) {
        self.push(Report::Text(fragment.to_owned()));
    }

    fn reasoning(&mut self, fragment: &str) {
        self.push(Report::Reasoning(fragment.to_owned()));
    }
}

impl Turns for Reports {
    /// A turn ended and asked for calls, so the run's next ask is the news a status bar
    /// wants: the count it carries moves on, and the frame says so by drawing the answer the
    /// turn left (§14).
    ///
    /// A turn that asked for no call ended the run, and the stop reason is the word a
    /// terminal is owed: "asking the model in turn 2" would be a status about a turn this run
    /// will never start. The calls themselves are run inside the turn, so the one state that
    /// cannot be said from here is `blocked` (see `Told::Waiting`).
    fn turn(&mut self, ended: &Exchange) {
        if !ended.calls.is_empty() {
            self.push(Report::Told(Told::Asking {
                turn: ended.turn + 1,
            }));
        }
    }
}

/// Takes everything the run in flight has left and puts it on the frame, and says whether the
/// run has ended.
///
/// A run that ended is a composer that is idle again — so the next `Ctrl-C` is the key that
/// clears the draft and not one that cancels nothing (§12) — and a thread that is joined
/// rather than left for the process to exit under. What the last turn said stays on the frame
/// as the live entry until the next run starts: the record is where a settled turn is kept,
/// and putting its entries on the frame is the transcript's (ADR-0007, ADR-0008).
fn take(
    canvas: &mut Canvas<'_>,
    view: &mut View,
    running: &mut Option<Running>,
    telling: &mut Teller<File>,
) -> io::Result<bool> {
    let Some(flight) = running.as_ref() else {
        return Ok(false);
    };
    let left = std::mem::take(&mut *lock(&flight.pending));
    let mut painted = false;
    let mut stopped = None;
    for report in left {
        match report {
            Report::Text(fragment) => {
                view.text(&fragment);
                painted = true;
            }
            Report::Reasoning(fragment) => {
                view.reasoning(&fragment);
                painted = true;
            }
            // A status changes no cell of the frame, and is said by the one thing here that
            // owns a tty: the report the run handed over is written as it lands (§14).
            Report::Told(told) => telling.say(told)?,
            Report::Stopped {
                stop,
                elapsed_ms,
                turns,
            } => stopped = Some((stop, elapsed_ms, turns)),
        }
    }
    if let Some((ref stop, elapsed_ms, turns)) = stopped {
        // The run's last word on the terminal: the stop reason is a change of state and is
        // said at once, whatever the pace would say about a turn (§14).
        telling.say(Told::Stopped {
            turn: turns,
            stop: stop.clone(),
        })?;
        // The marker settles on the verdict and on the duration the run stopped at, and this
        // is the last frame it is drawn on: the wait goes back to forever once the run is
        // gone, so settling draws once and nothing after (§12).
        view.set_pulse(Some(Pulse::settled(stop, elapsed_ms)));
        view.set_state(State::Idle);
        painted = true;
        if let Some(flight) = running.take() {
            let _ = flight.thread.join();
        }
    }
    if painted {
        paint(canvas, view)?;
    }
    Ok(stopped.is_some())
}

/// Answers one event, and says whether the launch ends there: the one place an event becomes
/// a repaint, a run, or an exit.
///
/// Which key means what is not decided here. The mode stack decides that — one row of one
/// table per mode — and this function is left with what the terminal does about the outcome
/// it is handed: paint the frame, start the run a prompt asks for, cancel the run in
/// flight, write the words an `OSC` 52 carries to the terminal, or give the screen back. A
/// new key is a row in a table, and a new screen is a mode, with no arm added to this
/// function for either.
#[allow(
    clippy::too_many_arguments,
    reason = "the one seam #43, #48, #50 and #56 each hand their value through"
)]
fn answer(
    terminal: &Terminal,
    canvas: &mut Canvas<'_>,
    view: &mut View,
    event: Event,
    running: &mut Option<Running>,
    asking: &mut Option<Asking>,
    cli: &Cli,
    config: &mut Config,
    environment: &impl Environment,
    wake: &Arc<Wake>,
    telling: &mut Teller<File>,
) -> io::Result<Option<Ending>> {
    let key = match event {
        // A paste is one edit and never a key, so nothing inside one can send: the
        // composer is handed the whole text with its line breaks in it.
        Event::Paste(text) => {
            view.paste(&text);
            paint(canvas, view)?;
            return Ok(None);
        }
        Event::Key(key) => key,
    };
    match view.key(key) {
        Outcome::Unchanged => Ok(None),
        Outcome::Edited => {
            paint(canvas, view)?;
            Ok(None)
        }
        // `Ctrl-C`: the run in flight is the thing cancelled and the draft stays (§12). The
        // flag is the one an outside `SIGINT` sets, so the keyboard and the signal are one
        // path to the same place and the run cannot tell them apart (§4).
        Outcome::Cancel => {
            if let Some(flight) = running {
                flight.cancel();
            }
            paint(canvas, view)?;
            Ok(None)
        }
        // A prompt that left the composer starts a run, unless one is already in flight:
        // then it waits above the composer for the run in flight to stop, and runs in the
        // order it was typed (§12).
        Outcome::Send(prompt) => {
            let sent = send(prompt, canvas, view, running, cli, config, wake, telling)?;
            Ok(sent)
        }
        // `Enter` at the settings modal: the keys a reader touched are written once, by the
        // core's owner-only atomic save, and the launch's config is resolved again so the next
        // prompt runs on what the visit left (§11, ADR-0009). A value the schema refuses comes
        // back naming its key and is said on the screen, with nothing written at all.
        Outcome::Save(touched) => {
            match persist(environment, &touched, cli.flags()) {
                Ok(fresh) => {
                    *config = fresh;
                    // The one switch, as the visit left it: a `status` key turned off here
                    // stops the telling at the next report rather than at the next launch.
                    telling.switch(config.tells_status());
                    view.saved();
                }
                Err(error) => view.refuse(&error.to_string()),
            }
            paint(canvas, view)?;
            Ok(None)
        }
        // `Ctrl-Y`: the words leave the terminal by OSC 52, and they are written here
        // because this module is the one that owns the tty — a core that named a sequence
        // would be a core that knows what a terminal is (§12, §14, ADR-0005). Nothing is
        // repainted: a copy changes no cell of the frame.
        Outcome::Copy(words) => {
            tell(&terminal.screen, &crate::osc::copy(&words))?;
            Ok(None)
        }
        // A model name `/model` chose or a `/model <name>` line named: it is written as the
        // one row a settings visit touches, so the name goes through the very save the
        // settings modal writes with — atomic, 0600, patching only the `model` key and
        // leaving the rest of the file, comments and all, the reader's own (ADR-0009, §11).
        // The config is then resolved again, so the next prompt runs on the name this one
        // wrote rather than on the one the launch started with (§12). A file that cannot be
        // written is the launch refusing, because a frame has no band for a refusal and a
        // name silently not written is the one silence this front end must not keep (§15).
        Outcome::Model(name) => {
            let mut touched = Touched::new();
            touched.set(MODEL, name);
            match persist(environment, &touched, cli.flags()) {
                Ok(fresh) => {
                    *config = fresh;
                    paint(canvas, view)?;
                    Ok(None)
                }
                Err(error) => Ok(Some(Ending::Refused(Refusal::new(
                    error.to_string(),
                    error.exit_code(),
                )))),
            }
        }
        // `/settings`: the screen the settings modal fills in, opened over the draft with the
        // config as it resolved — every row a key and the value in force. The frame holds no
        // config, so the seeding is here, where the one the launch resolved is (§11, §12).
        Outcome::Settings => {
            view.open_settings(config);
            paint(canvas, view)?;
            Ok(None)
        }
        // The `/model` screen is up and has nothing to rank: the wire is asked on a thread
        // of its own, so the loop is back in `poll` for a key while the request is out
        // (§10, §12).
        Outcome::Served => {
            *asking = ask(config, wake);
            paint(canvas, view)?;
            Ok(None)
        }
        // `Ctrl-Q` is the one way out.
        Outcome::Quit => Ok(Some(Ending::Quit)),
    }
}

/// A wire being asked what it serves, on a thread of its own: `/model` opens its screen at
/// once and the names land in the frame when they land, because a request takes as long as
/// it takes and the loop owes a key its answer now (§10, §12).
struct Asking {
    /// Where the listing lands, or `None` while the request is still out.
    landing: Arc<Mutex<Option<Listing>>>,
    /// The thread that asked, joined once the answer is on the frame.
    thread: JoinHandle<()>,
}

/// Asks the wire the config names what it serves, and pings the loop through the pipe a
/// signal uses when the answer is in: one more fd the loop waits on, and no timer.
fn ask(config: &Config, wake: &Arc<Wake>) -> Option<Asking> {
    let config = config.clone();
    let landing = Arc::new(Mutex::new(None));
    let answer = Arc::clone(&landing);
    let wake = Arc::clone(wake);
    let thread = thread::spawn(move || {
        // A launch with no wire is one `main` refused before the screen was taken, so the
        // only answer this thread can leave is a listing.
        if let Some(listing) = crate::launch::served(&config) {
            *lock(&answer) = Some(listing);
            wake.raise();
        }
    });
    // A thread that has landed its answer already is still joined below, and nothing here
    // blocks: what the frame paints now is the screen saying it is asking.
    Some(Asking { landing, thread })
}

/// Puts the names a wire answered with on the frame, and drops the thread that asked them:
/// the `/model` screen is painted once with the listing it was waiting for, the way a run's
/// fragments are painted as they land (§10, §12).
fn served(canvas: &mut Canvas<'_>, view: &mut View, asking: &mut Option<Asking>) -> io::Result<()> {
    let landed = match asking.as_ref() {
        Some(flight) => lock(&flight.landing).take(),
        None => None,
    };
    let Some(listing) = landed else {
        return Ok(());
    };
    view.set_served(listing);
    paint(canvas, view)?;
    if let Some(flight) = asking.take() {
        let _ = flight.thread.join();
    }
    Ok(())
}

/// Starts the run a sent draft asks for, as a launch of its own and a thread of its own, and
/// says whether the launch ends instead.
///
/// A launch that cannot run at all ends the launch rather than being drawn: a frame has no
/// band for a refusal yet, and a screen that swallowed the reason nothing ran would be the one
/// silence this front end must not keep (§15).
#[allow(
    clippy::too_many_arguments,
    reason = "the one seam a prompt starts a run through, and the teller it is announced to"
)]
fn send(
    prompt: Prompt,
    canvas: &mut Canvas<'_>,
    view: &mut View,
    running: &mut Option<Running>,
    cli: &Cli,
    config: &Config,
    wake: &Arc<Wake>,
    telling: &mut Teller<File>,
) -> io::Result<Option<Ending>> {
    if running.is_some() {
        // A run is live, so the prompt cannot be answered now: it joins the queue the frame
        // says as `N queued` rather than being dropped, and the run that stops next is the
        // oldest prompt waiting (§12).
        view.enqueue(prompt);
        paint(canvas, view)?;
        return Ok(None);
    }
    let launch = match Launch::open(cli, config, FrontEnd::Terminal) {
        Ok(launch) => launch,
        Err(refusal) => return Ok(Some(Ending::Refused(refusal))),
    };
    // A new run's fragments must not be drawn under the answer the last one left: the turn in
    // flight goes, and the composer is told a run is live so `Ctrl-C` cancels it (§12).
    view.settle();
    view.set_state(State::Running);
    *running = Some(Running::start(launch, prompt, Arc::clone(wake)));
    // The marker a run wears before it has said anything: the first step of the braille
    // cycle and no time on the clock yet, so the frame a prompt is sent from is a frame that
    // says the run has begun rather than a frame that looks exactly as it did (§12, #39).
    view.set_pulse(Some(Pulse::live(0)));
    paint(canvas, view)?;
    // The terminal is told the run has begun: its first turn is on the wire, and a status
    // bar that heard nothing while the frame went busy would be a status bar lying (§14).
    telling.say(Told::Asking { turn: 1 })?;
    Ok(None)
}

/// Starts the next prompt the queue is holding, once the run before it has stopped: one
/// prompt per stop, oldest first, which is what §12's "in order" means as a run.
///
/// A queue with nothing waiting starts nothing, and the launch goes back to waiting for a
/// key. A launch that cannot run at all ends the launch here as it does from a key — the
/// prompt is already out of the queue, and a reason nothing ran is still said.
#[allow(
    clippy::too_many_arguments,
    reason = "the same seam `send` is, plus the prompt a run that stopped left queued"
)]
fn resume(
    canvas: &mut Canvas<'_>,
    view: &mut View,
    running: &mut Option<Running>,
    cli: &Cli,
    config: &mut Config,
    wake: &Arc<Wake>,
    telling: &mut Teller<File>,
) -> io::Result<Option<Ending>> {
    let Some(prompt) = view.dequeue() else {
        return Ok(None);
    };
    send(prompt, canvas, view, running, cli, config, wake, telling)
}

/// What the terminal sent, once the bytes that carry it are whole.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    /// One key, however many bytes it took to say.
    Key(Key),
    /// A pasted prompt: one edit, its line breaks kept, and no way to send it.
    Paste(String),
}

/// What reading one sequence out of the bytes came to: a whole event, a sequence that
/// only changed what the next one means, or a sequence whose last bytes are still on
/// their way.
enum Step {
    Event(Event),
    Taken,
    Waiting,
}

/// `ESC`, the byte every sequence but a control one starts with.
const ESCAPE: u8 = 0x1b;

/// `ESC [ 2 0 1 ~`: the sequence that ends a bracketed paste, so everything before it is
/// one paste.
const PASTE_END: &[u8] = b"\x1b[201~";

/// The bytes a key arrives as, kept until the sequence they start is complete: a key is
/// more than a byte (§17), and a terminal is free to send one in as many reads as it
/// likes.
struct Decoder {
    /// Bytes read but not yet answered by an event.
    pending: Vec<u8>,
    /// Whether the newest read was one byte, which is the only thing that tells the
    /// `Enter` key from a paste's line break, and a bare `ESC` from the first byte of a
    /// sequence: a burst of bytes is a paste or a sequence, and no keyboard puts two
    /// keys in one read.
    alone: bool,
    /// Whether the bytes arriving now are a bracketed paste, up to `PASTE_END`.
    pasting: bool,
}

impl Decoder {
    /// A decoder holding nothing: the state a launch's first read starts from.
    fn new() -> Decoder {
        Decoder {
            pending: Vec::new(),
            alone: false,
            pasting: false,
        }
    }

    /// Hands over the bytes of one read, as `read` gave them up.
    fn push(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        self.alone = bytes.len() == 1;
    }

    /// The next event the bytes hold, or nothing when the sequence at their front is
    /// still incomplete — whatever it holds stays pending for the next read.
    fn next(&mut self) -> Option<Event> {
        loop {
            let step = if self.pasting {
                self.pasted()
            } else {
                self.one()
            };
            match step {
                Step::Event(event) => return Some(event),
                Step::Taken => {}
                Step::Waiting => return None,
            }
        }
    }

    /// The next key the bytes hold, one byte or one sequence at a time.
    fn one(&mut self) -> Step {
        let Some(&byte) = self.pending.first() else {
            return Step::Waiting;
        };
        // An `ESC` that arrived on its own is the Esc key: a sequence that opens with it
        // brings the bytes that say which sequence it is, and the alternative — waiting
        // for a byte a keyboard may never send — is a screen nobody can close.
        if byte == ESCAPE && self.pending.len() == 1 && self.alone {
            self.take(1);
            return Step::Event(Event::Key(Key::Escape));
        }
        if byte == ESCAPE {
            return self.escape();
        }
        if byte >= 0x80 {
            if byte < 0xc0 {
                // A byte that starts no glyph: nothing types one, and a decoder that
                // waited on it would never answer the key behind it.
                self.take(1);
                return Step::Taken;
            }
            return match self.glyph() {
                Some(glyph) => Step::Event(Event::Key(Key::Char(glyph))),
                None => Step::Waiting,
            };
        }
        self.take(1);
        let key = match byte {
            0x03 => Key::Cancel,
            // `Ctrl-U`: the value a settings row holds goes, and the key with it (§12).
            0x15 => Key::Clear,
            // `Ctrl-X`: the newest queued prompt is dropped (§12).
            0x18 => Key::Drop,
            // `Tab` is the byte raw mode leaves of it, and `Shift+Tab` is the sequence
            // `ESC [ Z` below: one key of a screen's, either way.
            b'\t' => Key::Tab,
            // `Ctrl-Q`, which is why flow control has to be off: a terminal left with
            // `IXON` eats this byte instead of sending it.
            0x11 => Key::Quit,
            // `Ctrl-Y`: the copy of §12, and no glyph of any draft.
            0x19 => Key::Copy,
            0x7f | 0x08 => Key::Backspace,
            // `Ctrl-J` is the plain newline every terminal sends, and it breaks a line
            // rather than sending one: `Enter` is the carriage return below.
            b'\n' => Key::Newline,
            b'\r' if self.alone => Key::Enter,
            // A carriage return that came with more of its burst, before or after it, is
            // a paste's line break: a clipboard's newline reaches a terminal as the byte
            // its Enter key would send, and no line break in a paste may send.
            b'\r' => Key::Newline,
            0x20..=0x7e => Key::Char(char::from(byte)),
            // A control byte no key of this frame's is: not ours to answer.
            _ => return Step::Taken,
        };
        Step::Event(Event::Key(key))
    }

    /// One escape sequence off the front of the bytes: the keys §17's protocol and the
    /// arrows spell, and the sequence that opens a bracketed paste.
    fn escape(&mut self) -> Step {
        match self.pending.get(1) {
            None => Step::Waiting,
            Some(b'[') => self.control(),
            Some(b'O') => {
                // The older single-shift arrows: `ESC O A` is the up arrow under a
                // terminal that is not speaking the protocol §17 assumes.
                let Some(&last) = self.pending.get(2) else {
                    return Step::Waiting;
                };
                self.take(3);
                match arrow(last) {
                    Some(key) => Step::Event(Event::Key(key)),
                    None => Step::Taken,
                }
            }
            Some(_) => {
                // A lone `ESC` is a key this frame has no use for, and holding it would
                // stall every key behind it.
                self.take(1);
                Step::Taken
            }
        }
    }

    /// A control sequence, `ESC [ …`, answered at its final byte — which is how a key of
    /// more than one byte is read whole whatever the terminal split it into.
    fn control(&mut self) -> Step {
        let Some(last_at) = self.pending[2..]
            .iter()
            .position(|byte| (0x40..=0x7e).contains(byte))
        else {
            return Step::Waiting;
        };
        let last_at = last_at + 2;
        let last = self.pending[last_at];
        let params = String::from_utf8_lossy(&self.pending[2..last_at]).into_owned();
        self.take(last_at + 1);
        match (last, params.as_str()) {
            // A modified arrow says so between the bracket and its final byte, which is
            // the one spelling §17's terminals use for `Shift+↑` and `Shift+↓` (§12, #50).
            (b'A', _) if shifted(&params) => Step::Event(Event::Key(Key::ShiftUp)),
            (b'B', _) if shifted(&params) => Step::Event(Event::Key(Key::ShiftDown)),
            (b'A', _) => Step::Event(Event::Key(Key::Up)),
            (b'B', _) => Step::Event(Event::Key(Key::Down)),
            (b'C', _) => Step::Event(Event::Key(Key::Right)),
            (b'D', _) => Step::Event(Event::Key(Key::Left)),
            // `Shift+Tab` is `CSI Z` on every terminal, and `CSI 1;2Z` under one that
            // says which modifier it held.
            (b'Z', _) => Step::Event(Event::Key(Key::BackTab)),
            (b'H', _) | (b'~', "1") | (b'~', "7") => Step::Event(Event::Key(Key::Home)),
            (b'F', _) | (b'~', "4") | (b'~', "8") => Step::Event(Event::Key(Key::End)),
            // The paste that starts here is answered by `pasted`: one event for
            // everything up to the sequence that ends it.
            (b'~', "200") => {
                self.pasting = true;
                Step::Taken
            }
            (b'u', _) => match reported(&params) {
                Some(key) => Step::Event(Event::Key(key)),
                None => Step::Taken,
            },
            _ => Step::Taken,
        }
    }

    /// The bracketed paste that is arriving: everything up to the sequence that ends it,
    /// as one edit. Until that sequence lands there is no event at all, which is what
    /// makes a line break inside a paste unable to send it.
    fn pasted(&mut self) -> Step {
        let Some(end) = find(&self.pending, PASTE_END) else {
            return Step::Waiting;
        };
        let text = String::from_utf8_lossy(&self.pending[..end]).into_owned();
        self.take(end + PASTE_END.len());
        self.pasting = false;
        Step::Event(Event::Paste(text))
    }

    /// One glyph off the front of the bytes, or nothing while its last bytes are still
    /// on their way: a paste in a language this keyboard is not in arrives a read at a
    /// time, and half a glyph is not a glyph.
    fn glyph(&mut self) -> Option<char> {
        let width = match self.pending.first() {
            Some(0xc0..=0xdf) => 2,
            Some(0xe0..=0xef) => 3,
            Some(0xf0..=0xf7) => 4,
            _ => return None,
        };
        if self.pending.len() < width {
            return None;
        }
        let glyph = std::str::from_utf8(&self.pending[..width])
            .ok()
            .and_then(|text| text.chars().next());
        self.take(width);
        glyph
    }

    /// Drops `count` bytes off the front of what is pending.
    fn take(&mut self, count: usize) {
        self.pending.drain(..count);
    }
}

/// Whether a control sequence's parameters say the shift key was held: the modifiers are
/// the last field, one more than a mask of shift, alt, ctrl and super.
fn shifted(params: &str) -> bool {
    /// The bit that says shift was held, in the protocol's modifier mask.
    const SHIFT: u32 = 1;

    let modifiers = params.rsplit(';').next().unwrap_or_default();
    let modifiers = modifiers.split(':').next().unwrap_or_default();
    modifiers
        .parse::<u32>()
        .is_ok_and(|held| held.saturating_sub(1) & SHIFT != 0)
}

/// The key an arrow's final byte names, from either spelling of the arrows.
fn arrow(last: u8) -> Option<Key> {
    match last {
        b'A' => Some(Key::Up),
        b'B' => Some(Key::Down),
        b'C' => Some(Key::Right),
        b'D' => Some(Key::Left),
        _ => None,
    }
}

/// The key a kitty-protocol report names: `CSI <code> ; <modifiers> u`, where the
/// modifiers are one more than a mask of shift, alt, ctrl and super. Only the reports
/// this frame's map needs are answered — the `Enter` that sends and the two that break a
/// line — because a key nobody answers is not a key.
fn reported(params: &str) -> Option<Key> {
    /// The bit that says shift was held, in the protocol's modifier mask.
    const SHIFT: u32 = 1;
    /// The bit that says ctrl was held.
    const CTRL: u32 = 4;

    let (code, modifiers) = params.split_once(';').unwrap_or((params, "1"));
    let code: u32 = code.parse().ok()?;
    let modifiers: u32 = modifiers.split(':').next()?.parse().ok()?;
    let modifiers = modifiers.saturating_sub(1);
    match (code, modifiers) {
        (13, 0) => Some(Key::Enter),
        (9, 0) => Some(Key::Tab),
        (9, held) if held & SHIFT != 0 => Some(Key::BackTab),
        (13, held) if held & SHIFT != 0 => Some(Key::Newline),
        (106, held) if held & CTRL != 0 => Some(Key::Newline),
        _ => None,
    }
}

/// Where `needle` first appears in `bytes`, which is how a bracketed paste finds the
/// sequence that ends it.
fn find(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes
        .windows(needle.len())
        .position(|window| window == needle)
}

/// What the terminal is to be given back as: the state raw mode replaced and the two
/// fds it is put back through, held where a panic hook can reach them, because an
/// unwinding path runs no guard this module owns.
static GIVE_BACK: Mutex<Option<Restore>> = Mutex::new(None);

/// The state to put back, and the terminal it goes to.
struct Restore {
    /// The terminal's input side, whose termios raw mode changed.
    keys: File,
    /// The terminal's output side, where the alternate screen was taken.
    screen: File,
    /// The terminal's state before raw mode.
    saved: Termios,
}

impl Restore {
    /// Puts the terminal back: the screen is left first, so nothing that follows —
    /// a panic message included — is drawn on a screen that is about to be somebody
    /// else's, and then the state raw mode replaced is restored.
    fn put_back(self) {
        let _ = tell(&self.screen, GIVE_BACK_SCREEN);
        let _ = tcsetattr(&self.keys, OptionalActions::Now, &self.saved);
    }
}

/// Gives the terminal back, exactly once, whichever of the guard and the panic hook
/// asks first: the state is taken out of the static, so the second ask finds nothing
/// and a shell is never handed a half-restored terminal twice.
fn give_back() {
    let taken = lock(&GIVE_BACK).take();
    if let Some(restore) = taken {
        restore.put_back();
    }
}

/// A panic hook, in the shape `std::panic` hands out and takes back: what the hook
/// this module installs keeps of the one it replaced, so a panic still gets reported.
type Hook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync>;

/// Installs, once, the hook that gives the terminal back when a panic unwinds past
/// the guard: a terminal left raw and blank is a shell nobody can type in, and an
/// unwinding path runs no guard at all.
fn guard_panics() {
    /// Whether this process has a hook of this module's yet.
    static INSTALLED: Once = Once::new();
    /// The hook this one replaced, which is still what reports a panic.
    static REPLACED: OnceLock<Hook> = OnceLock::new();

    INSTALLED.call_once(|| {
        // The replaced hook is called rather than dropped: a terminal given back is
        // no reason for a panic to go unreported, and a panic nobody can read is the
        // bug this hook is least able to catch.
        let _ = REPLACED.set(panic::take_hook());
        panic::set_hook(Box::new(|raised| {
            give_back();
            if let Some(replaced) = REPLACED.get() {
                replaced(raised);
            }
        }));
    });
}

/// The pixel size of one cell, as this launch's terminal reports it, and the bytes the
/// terminal sent that were not the answer and are therefore a reader's keys (§12, ADR-0003).
///
/// Three answers, in the order of what they cost. The kernel's own `winsize` carries the
/// window's pixels on a terminal that fills it, and costs no round trip at all. `CSI 16 t` is
/// one write and one bounded wait, for the terminals that do not fill it. And a terminal that
/// will not say is drawn at the nominal cell rather than holding the launch open.
fn probed_cell(terminal: &Terminal) -> (CellSize, Vec<u8>) {
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

/// The cell size a terminal's own `winsize` implies, when it carries pixels: the window's
/// pixels over the cells it holds. A terminal that reports no pixels, or fewer pixels than it
/// has cells, has not said, and `None` says so.
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

/// The cell size a terminal's answer carries, read off the bytes it sent, beside the bytes
/// that were not that answer — a key typed while the launch waited is a key, not a reply.
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

/// A `CSI 6 ; height ; width t` answer at the front of `bytes`, and how many bytes it took.
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

/// Writes a sequence out and flushes it: a frame that sat in a buffer is a frame
/// nobody saw, and a screen given back only at exit is a terminal left blank when
/// the process dies.
fn tell(out: &File, sequence: &[u8]) -> io::Result<()> {
    let mut out = out;
    out.write_all(sequence)?;
    out.flush()
}

/// A lock held for the length of one statement: a panic hook that blocked on a
/// poisoned lock would nest the panic it was called to clean up after.
fn lock<T>(held: &Mutex<T>) -> MutexGuard<'_, T> {
    held.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    //! The one contract an integration test cannot reach from outside the process: a
    //! panic unwinds past no guard this module owns, so the hook is proved here, on a
    //! pseudo-terminal of this process's own, where a panic can be raised and caught
    //! on purpose and the terminal's state read back afterwards.

    use std::panic;
    use std::path::PathBuf;

    use clap::Parser;
    use rustix::fs::{Mode, OFlags, open};
    use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
    use rustix::termios::{LocalModes, tcsetwinsize};
    use sindri::rows::Scroll;
    use sindri_core::records::Entry;
    use sindri_core::redact::SecretSet;

    use super::*;

    /// A pseudo-terminal the test holds both ends of: the slave stands in for both
    /// standard streams a launch would be handed, and the master is where the
    /// sequences a launch writes come back.
    struct Pty {
        /// The end nobody in the program reads, where what it drew lands.
        master: File,
        /// The end the program takes as its terminal.
        slave: File,
    }

    impl Pty {
        /// A fresh pseudo-terminal, nobody's controlling terminal.
        fn open() -> Pty {
            let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).expect("a pseudoterminal");
            grantpt(&master).expect("the slave is granted");
            unlockpt(&master).expect("the slave is unlocked");
            let name = ptsname(&master, Vec::new()).expect("the slave has a name");
            let path = PathBuf::from(name.to_str().expect("a printable name"));
            let slave = open(path.as_path(), OFlags::RDWR | OFlags::NOCTTY, Mode::empty())
                .expect("the slave opens");
            Pty {
                master: File::from(master),
                slave: File::from(slave),
            }
        }

        /// Everything written to the terminal since the last read of it.
        fn drawn(&mut self) -> String {
            let mut drawn = [0u8; 4096];
            let mut master = &self.master;
            let count = master.read(&mut drawn).expect("the terminal's screen");
            String::from_utf8_lossy(&drawn[..count]).into_owned()
        }

        /// The size this terminal reports, cells and pixels both.
        fn resize(&self, cols: u16, rows: u16, xpixel: u16, ypixel: u16) {
            tcsetwinsize(
                &self.slave,
                Winsize {
                    ws_row: rows,
                    ws_col: cols,
                    ws_xpixel: xpixel,
                    ws_ypixel: ypixel,
                },
            )
            .expect("the terminal takes its size");
        }

        /// Whether the launch has written nothing at all: a master with no byte waiting is a
        /// terminal nobody spoke to, and asking that question must not block the way a read
        /// would.
        fn silent(&mut self) -> bool {
            let mut fds = [PollFd::new(&self.master, PollFlags::IN)];
            let wait = Timespec::try_from(Duration::from_millis(20)).expect("a span");
            poll(&mut fds, Some(&wait)).is_ok() && !fds[0].revents().contains(PollFlags::IN)
        }

        /// The terminal's state, as the program left it.
        fn attrs(&self) -> Termios {
            tcgetattr(&self.slave).expect("the terminal's state")
        }
    }

    /// The size a test's frame is painted at: a pseudo-terminal holds no size of its own until
    /// one is set, and a canvas that asks for one instead of being told would put a question to
    /// the environment that the test already has the answer to.
    fn painted_at() -> ratatui::layout::Rect {
        ratatui::layout::Rect::new(0, 0, 80, 24)
    }

    /// A canvas a test paints through is the shape the test gave it, not the shape the terminal
    /// reports: the size is an argument, so no `/dev/tty` is opened and no `tput` is spawned to
    /// answer a question a pseudo-terminal was never going to answer anyway (§16).
    #[test]
    fn a_canvas_told_its_size_is_the_shape_it_was_told() {
        let pty = Pty::open();
        pty.resize(100, 30, 1000, 600);
        let terminal =
            Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");

        let mut canvas = canvas_at(&terminal, painted_at()).expect("a canvas over it");
        let mut drawn = None;
        canvas
            .draw(|frame| drawn = Some(frame.area()))
            .expect("a frame");

        assert_eq!(
            drawn,
            Some(painted_at()),
            "the frame is the shape the test gave it, and the terminal's own 100 by 30 is not"
        );
        // The question this canvas never asks is `Terminal::size`, which is answered from
        // outside this process — by the terminal a suite happens to run under, or by a `tput`
        // that had to be spawned to find one — so a refusal to answer it is reported here as a
        // frame that could not be measured rather than as the process slot it wanted.
    }

    /// ADR-0003's cheap half: a terminal that fills its `winsize`'s pixel fields is asked
    /// nothing, because the kernel's own answer is the whole of what a picture needs.
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

    /// ADR-0003's amendment at the one place the environment is turned into a frame: the two
    /// facts the terminal gives reach the view, so a terminal that speaks Kitty is handed the
    /// pictures its frames draw from and one that speaks no protocol is handed none — the
    /// frame it paints is the words of a picture, whole (§12).
    #[test]
    fn a_terminal_that_speaks_kitty_is_handed_pictures_and_one_that_speaks_none_is_handed_none() {
        let cell = CellSize::new(10, 20);

        let mut speaks = View::boot();
        hand_pictures(&mut speaks, cell, Some("xterm-kitty"), None);
        assert!(
            speaks.pictures().draws(),
            "a terminal that names Kitty is handed the pictures its frames draw from"
        );

        let mut silent = View::boot();
        silent.set_pictures(Pictures::kitty(cell));
        hand_pictures(&mut silent, cell, Some("screen-256color"), Some("tmux"));
        assert!(
            !silent.pictures().draws(),
            "and a terminal that names no protocol is handed none, which is the words of a \
             picture and no placement"
        );
    }

    /// The other answer: a terminal that reports no pixels is asked, and one that says
    /// nothing is drawn at the nominal cell rather than holding the launch open.
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

    /// And the answer, read for what it says: `CSI 6 ; height ; width t`, with whatever a
    /// reader typed while the launch waited kept beside it rather than swallowed.
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

    #[test]
    fn a_panic_gives_the_terminal_back_before_the_guard_can() {
        let mut pty = Pty::open();
        let before = pty.attrs();
        let terminal =
            Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");
        let screen = Screen::take(&terminal).expect("the screen is taken");

        assert!(
            !pty.attrs().local_modes.contains(LocalModes::ICANON),
            "raw mode is in force, so there is something for a panic to undo"
        );

        let caught = panic::catch_unwind(|| panic!("a panic the hook has to clean up after"));

        assert!(caught.is_err(), "the panic really was raised");
        let after = pty.attrs();
        assert_eq!(
            after.local_modes.bits(),
            before.local_modes.bits(),
            "the terminal's local modes are the ones raw mode replaced"
        );
        assert_eq!(
            after.input_modes.bits(),
            before.input_modes.bits(),
            "and its input modes too, flow control included"
        );
        let drawn = pty.drawn();
        assert!(
            drawn.contains(std::str::from_utf8(GIVE_BACK_SCREEN).expect("a printable sequence")),
            "the alternate screen was given back while the guard was still alive: {drawn:?}"
        );

        // The guard is dropped after the panic has already been cleaned up after, and
        // a terminal handed back twice is a shell with its state overwritten.
        drop(screen);
        assert_eq!(
            pty.attrs().input_modes.bits(),
            before.input_modes.bits(),
            "the second give-back changes nothing"
        );
    }

    #[test]
    fn a_copy_leaves_the_terminal_by_osc_52_and_asks_no_clipboard_utility() {
        let mut pty = Pty::open();
        let terminal =
            Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to write to");
        let mut canvas = canvas_at(&terminal, painted_at()).expect("a canvas over it");
        let mut view = View::of(
            vec![Entry::Assistant {
                turn: 1,
                text: SecretSet::default().redact("read the transcript"),
            }],
            Scroll::Tail,
        );

        // No mark, so `Ctrl-Y` copies the last answer, which is the case a launch with no
        // pointer can reach: the words leave the terminal as the sequence that carries
        // them, and nothing is spawned to put them in a clipboard — the bytes go down the
        // wire the frame does, which is what makes a copy work over SSH.
        // The launch's own values, untouched by a copy: nothing here starts a run, and a
        // wake nobody writes to is the loop's when it is there.
        let cli = Cli::parse_from(["sindri"]);
        let mut config = sindri_core::config::Config::default();
        let environment = std::collections::BTreeMap::new();
        let wake = Arc::new(Wake::open().expect("a wake"));
        // A teller on the terminal the frame is drawn on, because that is where a status
        // goes (§14). Neither key here is a report, so it writes nothing.
        let mut telling = Teller::new(
            pty.slave
                .try_clone()
                .expect("a second handle on the terminal"),
            true,
            std::path::Path::new("/work/sindri"),
        );
        let mut running = None;
        let mut asking = None;
        let ended = answer(
            &terminal,
            &mut canvas,
            &mut view,
            Event::Key(Key::Copy),
            &mut running,
            &mut asking,
            &cli,
            &mut config,
            &environment,
            &wake,
            &mut telling,
        )
        .expect("the copy is written");

        assert!(ended.is_none(), "a copy is not the key that quits");
        let drawn = pty.drawn();
        let sequence = crate::osc::copy("read the transcript");
        assert!(
            drawn.contains(std::str::from_utf8(&sequence).expect("a printable sequence")),
            "the words left as OSC 52 and as nothing else: {drawn:?}"
        );
    }

    /// The config home a settings test writes into: one `config.toml` a person left with a
    /// comment and a model in it, and the environment that points at it.
    fn config_home(
        text: &str,
    ) -> (
        tempfile::TempDir,
        std::collections::BTreeMap<String, String>,
    ) {
        let home = tempfile::tempdir().expect("a temp dir");
        let directory = home.path().join("sindri");
        std::fs::create_dir_all(&directory).expect("the sindri directory");
        std::fs::write(directory.join("config.toml"), text).expect("the config file");
        let environment = [(
            String::from("XDG_CONFIG_HOME"),
            home.path().display().to_string(),
        )]
        .into_iter()
        .collect();
        (home, environment)
    }

    /// The row a settings screen's keys act on, as a reader walks to it.
    fn walk_to(view: &mut View, key: &str) {
        for _ in 0..sindri_core::config::Config::KEYS.len() {
            let row = view
                .settings()
                .rows()
                .nth(view.settings().current())
                .map(|(name, _)| name);
            if row == Some(key) {
                return;
            }
            view.key(Key::Down);
        }
        panic!("{key} is a key of the schema, so the screen has a row for it");
    }

    #[test]
    fn enter_at_the_settings_modal_writes_the_file_and_the_next_prompt_runs_on_it() {
        let (home, environment) = config_home("# mine\nmodel = \"old-model\"\n");
        let path = home.path().join("sindri").join("config.toml");
        let cli = Cli::parse_from(["sindri"]);
        let mut config = Config::resolve(&environment, cli.flags()).expect("a config");
        let pty = Pty::open();
        let terminal =
            Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to write to");
        let mut canvas = canvas_at(&terminal, painted_at()).expect("a canvas over it");
        let mut view = View::boot();
        view.open_settings(&config);
        let wake = Arc::new(Wake::open().expect("a wake"));
        // A teller on the terminal the frame is drawn on, because that is where a status
        // goes (§14). Neither key here is a report, so it writes nothing.
        let mut telling = Teller::new(
            pty.slave
                .try_clone()
                .expect("a second handle on the terminal"),
            true,
            std::path::Path::new("/work/sindri"),
        );
        let mut running = None;
        let mut asking = None;

        walk_to(&mut view, "model");
        view.key(Key::Clear);
        for glyph in "new-model".chars() {
            view.key(Key::Char(glyph));
        }
        let ended = answer(
            &terminal,
            &mut canvas,
            &mut view,
            Event::Key(Key::Enter),
            &mut running,
            &mut asking,
            &cli,
            &mut config,
            &environment,
            &wake,
            &mut telling,
        )
        .expect("the write is made");

        assert!(ended.is_none(), "a write is not the key that quits");
        assert_eq!(
            std::fs::read_to_string(&path).expect("the file the write landed in"),
            "# mine\nmodel = \"new-model\"\n",
            "Enter writes at once, and only the row the reader touched: the comment is the \
             byte it was"
        );
        assert_eq!(
            view.mode(),
            sindri::keys::Mode::Composer,
            "and the visit is over, so the draft has the keys again"
        );
        assert_eq!(
            config.model.as_deref(),
            Some("new-model"),
            "the launch's config is resolved again from what the visit wrote, so the next \
             prompt — the run `send` starts from this very value — runs on it"
        );
    }

    mod decoding {
        //! The bytes a terminal really sends, read as the keys the composer's map
        //! answers: a sequence that arrives in pieces, a paste that arrives as a burst,
        //! and the bracketed paste §17's protocol wraps one in — none of them guessed at.

        use super::super::*;

        /// Every event the decoder answers for the bytes, pushed the way `read` hands
        /// them over: one chunk at a time.
        fn events(chunks: &[&[u8]]) -> Vec<Event> {
            let mut decoder = Decoder::new();
            let mut events = Vec::new();
            for chunk in chunks {
                decoder.push(chunk);
                while let Some(event) = decoder.next() {
                    events.push(event);
                }
            }
            events
        }

        /// The one event the bytes come to, or a failure naming what came instead.
        fn event(bytes: &[u8]) -> Event {
            let mut all = events(&[bytes]);
            assert_eq!(
                all.len(),
                1,
                "one key is one event, and these bytes made {all:?}"
            );
            all.pop().expect("the event")
        }

        /// A key, as the event that carries it.
        fn key(key: Key) -> Event {
            Event::Key(key)
        }

        #[test]
        fn a_bare_escape_is_the_key_that_closes_a_screen() {
            assert_eq!(
                event(b"\x1b"),
                key(Key::Escape),
                "one ESC read on its own is the Esc key, and not the start of a sequence \
                 that never came"
            );
        }

        #[test]
        fn a_glyph_typed_is_one_key() {
            assert_eq!(event(b"a"), key(Key::Char('a')));
        }

        #[test]
        fn a_glyph_split_across_two_reads_arrives_whole() {
            // The tick's three bytes, as a terminal may hand them over across two reads.
            assert_eq!(events(&[&[0xe2, 0x9c], &[0x93]]), vec![key(Key::Char('✓'))]);
        }

        #[test]
        fn enter_is_the_carriage_return_a_keyboard_sends() {
            assert_eq!(event(b"\r"), key(Key::Enter));
        }

        #[test]
        fn ctrl_j_is_the_newline_every_terminal_sends() {
            assert_eq!(event(b"\n"), key(Key::Newline));
        }

        #[test]
        fn shift_enter_under_the_kitty_protocol_breaks_the_line() {
            assert_eq!(event(b"\x1b[13;2u"), key(Key::Newline));
        }

        #[test]
        fn a_kitty_enter_report_without_a_modifier_still_sends() {
            assert_eq!(event(b"\x1b[13u"), key(Key::Enter));
        }

        #[test]
        fn ctrl_x_is_the_byte_that_drops_the_newest_queued_prompt() {
            assert_eq!(event(b"\x18"), key(Key::Drop));
        }

        #[test]
        fn ctrl_q_and_ctrl_c_are_whole_keys() {
            assert_eq!(event(b"\x11"), key(Key::Quit));
            assert_eq!(event(b"\x03"), key(Key::Cancel));
        }

        #[test]
        fn backspace_arrives_as_either_byte_raw_mode_leaves_it() {
            assert_eq!(event(b"\x7f"), key(Key::Backspace));
            assert_eq!(event(b"\x08"), key(Key::Backspace));
        }

        #[test]
        fn the_arrows_and_the_end_keys_arrive_in_either_spelling() {
            assert_eq!(event(b"\x1b[A"), key(Key::Up));
            assert_eq!(event(b"\x1bOB"), key(Key::Down), "the older spelling");
            assert_eq!(event(b"\x1b[C"), key(Key::Right));
            assert_eq!(event(b"\x1b[D"), key(Key::Left));
            assert_eq!(event(b"\x1b[H"), key(Key::Home));
            assert_eq!(event(b"\x1b[1~"), key(Key::Home));
            assert_eq!(event(b"\x1b[F"), key(Key::End));
            assert_eq!(event(b"\x1b[4~"), key(Key::End));
        }

        #[test]
        fn a_sequence_read_in_pieces_is_still_one_key() {
            assert_eq!(
                events(&[b"\x1b[", b"13;", b"2u"]),
                vec![key(Key::Newline)],
                "a key split across three reads is one key, not four"
            );
        }

        #[test]
        fn a_bracketed_paste_is_one_event_however_many_lines_it_holds() {
            assert_eq!(
                event(b"\x1b[200~one\r\ntwo\rthree\x1b[201~"),
                Event::Paste(String::from("one\r\ntwo\rthree")),
                "the clipboard's text, whole, and not one event per line"
            );
        }

        #[test]
        fn a_paste_that_has_not_ended_yet_is_not_a_key() {
            assert!(
                events(&[b"\x1b[200~one\r"]).is_empty(),
                "no line break inside a paste can escape as the key that sends it"
            );
        }

        #[test]
        fn a_paste_that_arrives_as_a_burst_sends_nothing() {
            let events = events(&[b"one\rtwo\r"]);

            assert!(
                !events.contains(&key(Key::Enter)),
                "a terminal without bracketed paste sends a paste as pressed keys, and \
                 no line break in one may send: {events:?}"
            );
            assert_eq!(events.len(), 8, "one event per glyph and per line break");
        }

        #[test]
        fn ctrl_y_arrives_as_the_key_that_copies() {
            assert_eq!(
                event(b"\x19"),
                key(Key::Copy),
                "`Ctrl-Y` is one control byte, and it is the key the mark is copied with \
                 rather than a glyph of the draft"
            );
        }

        #[test]
        fn a_shifted_arrow_arrives_as_the_key_that_grows_the_mark() {
            assert_eq!(
                event(b"\x1b[1;2A"),
                key(Key::ShiftUp),
                "a modified arrow carries its modifiers between the bracket and the final \
                 byte, which is how a terminal says shift was held"
            );
            assert_eq!(event(b"\x1b[1;2B"), key(Key::ShiftDown));
        }

        #[test]
        fn a_key_after_a_paste_is_read_again() {
            assert_eq!(
                events(&[b"\x1b[200~a\x1b[201~", b"\r"]),
                vec![Event::Paste(String::from("a")), key(Key::Enter)],
                "the paste ends where its sequence ends, and the next key is a key"
            );
        }
    }
}
