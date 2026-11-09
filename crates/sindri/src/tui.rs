//! The one place the terminal is owned: raw mode with flow control off, without which
//! `Ctrl-Q` never arrives, the alternate screen taken and given back on every path out
//! including a panic, and a launch with no terminal refused before any of it.

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
use sindri::rail::Facts;
use sindri::settings::persist;
use sindri::view::View;
use sindri_core::config::{Config, Environment, Process, Touched};
use sindri_core::provider::Deltas;
use sindri_core::provider::served::Listing;
use sindri_core::records::{SessionDocument, Told};
use sindri_core::redact::SecretSet;
use sindri_core::run::{Cancel, Exchange, Stop};
use sindri_core::tools::FrontEnd;

use crate::cli::Cli;
use crate::launch::{Launch, Refusal, Turns};
use crate::osc::Teller;
use crate::signal::{Signals, Wake};

/// A launch that cannot be a run, or a screen it cannot draw on: exit 2.
const USAGE: u8 = 2;

/// A terminal that will not take the screen or will not give a key back: exit 3.
const SETUP: u8 = 3;

/// The user's own word from outside the keyboard: the code a `SIGINT` has always meant.
const CANCELLED: u8 = 130;

/// Takes the alternate screen with the cursor hidden, so the first frame is not drawn
/// around a caret the last shell left, and asks for bracketed paste so a paste is one edit.
const TAKE_SCREEN: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?2004h";

/// Gives the screen back with bracketed paste off first and the cursor last, so a shell
/// gets the terminal it had.
const GIVE_BACK_SCREEN: &[u8] = b"\x1b[?2004l\x1b[?25h\x1b[?1049l";

const MODEL: &str = "model";

/// `CSI 16 t`: asks how many pixels one cell is. A terminal that knows answers
/// `CSI 6 ; height ; width t` and one that cannot says nothing, hence the bounded wait.
const ASK_CELL: &[u8] = b"\x1b[16t";

const REPLY_WAIT: Duration = Duration::from_millis(120);

/// Hands `view` the pictures its terminal places: a Kitty terminal gets the pictures its
/// frames draw from and any other gets none, which leaves it the words. `term` and `program`
/// are arguments rather than reads so a test need not set an environment.
fn hand_pictures(view: &mut View, cell: CellSize, term: Option<&str>, program: Option<&str>) {
    view.set_pictures(match picture::speaks_kitty(term, program) {
        true => Pictures::kitty(cell),
        false => Pictures::none(),
    });
}

pub fn run(cli: &Cli, config: &Config) -> ExitCode {
    if let Some(reason) = missing_terminal() {
        return crate::refuse(&reason, USAGE);
    }
    // Opened before the screen is taken: a `SIGINT` that arrives between raw mode and the loop
    // has to be a cancellation, not a process dying with the terminal left raw and the screen up.
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
    // The cell size is asked before the first frame, and a key typed while the launch waited is
    // handed to the loop rather than dropped.
    let (cell_size, typed) = probed_cell(&terminal);
    let mut view = View::boot();
    view.set_cell_size(cell_size);
    let named = |key: &str| std::env::var(key).ok();
    hand_pictures(
        &mut view,
        cell_size,
        named("TERM").as_deref(),
        named("TERM_PROGRAM").as_deref(),
    );
    // The same directory `Launch::open` gives a run, read the same way, so a path a reader
    // picks is one the run's own commands can read.
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
    // Read the way `launch` reads it, so the name `/skill:name` accepts and a name a run could
    // call come from one catalog.
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
    // One teller for the whole launch, so the pace a status is told at is the launch's: the
    // screen it writes on is a second handle on the descriptor a frame is drawn on.
    let mut telling = match terminal.screen.try_clone() {
        Ok(screen) => Teller::new(screen, config.tells_status(), &workspace),
        Err(error) => {
            return crate::refuse(&format!("the terminal could not be told: {error}"), SETUP);
        }
    };
    if let Err(error) = telling.say(Told::Idle) {
        return crate::refuse(&format!("the status could not be written: {error}"), SETUP);
    }
    // Replaced by a visit to the settings modal: the file is written once and resolved again,
    // so the prompt after the visit runs on what it holds.
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
    // Given back before a word is said about the ending: a refusal is a line for the shell, and
    // not a smear over a frame.
    drop(screen);
    match keys {
        Ok(Ending::Quit) => ExitCode::SUCCESS,
        Ok(Ending::Cancelled) => ExitCode::from(CANCELLED),
        Ok(Ending::Refused(refusal)) => crate::refuse(refusal.message(), refusal.code()),
        Err(error) => crate::refuse(&format!("the terminal could not be read: {error}"), SETUP),
    }
}

enum Ending {
    Quit,
    Cancelled,
    Refused(Refusal),
}

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

struct Terminal {
    keys: File,
    screen: File,
}

impl Terminal {
    fn stdio() -> io::Result<Terminal> {
        Terminal::pair(io::stdin().as_fd(), io::stdout().as_fd())
    }

    fn pair(keys: BorrowedFd<'_>, screen: BorrowedFd<'_>) -> io::Result<Terminal> {
        Ok(Terminal {
            keys: File::from(keys.try_clone_to_owned()?),
            screen: File::from(screen.try_clone_to_owned()?),
        })
    }
}

/// The screen a launch holds: taking one puts the terminal in raw mode and dropping it gives
/// the terminal back. A guard, so no `?`, early return or panic can forget half of it.
struct Screen;

impl Screen {
    fn take(terminal: &Terminal) -> io::Result<Screen> {
        guard_panics();
        let saved = tcgetattr(&terminal.keys)?;
        let mut raw = saved.clone();
        raw.make_raw();
        // `make_raw` clears flow control too, and this stays because the day raw mode is spelled
        // differently is the day `Ctrl-Q` stops arriving.
        raw.input_modes.remove(InputModes::IXON);
        tcsetattr(&terminal.keys, OptionalActions::Now, &raw)?;

        // Handed to the panic hook as well, so the screen is given back even when the unwinding
        // that follows runs no guard.
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

/// The screen frames are painted through, taken once: a canvas that remembered nothing would
/// rewrite every cell of the frame on every key.
type Canvas<'a> = ratatui::Terminal<CrosstermBackend<&'a File>>;

fn canvas(terminal: &Terminal) -> io::Result<Canvas<'_>> {
    ratatui::Terminal::new(CrosstermBackend::new(&terminal.screen))
}

/// The same canvas at a size already known, so nothing is asked of the operating system.
/// crossterm answers a size question by opening `/dev/tty` and, with none to open, by running
/// `tput`, whose `fork` a suite's parallel threads can have refused for want of process slots.
#[cfg(test)]
fn canvas_at(terminal: &Terminal, area: ratatui::layout::Rect) -> io::Result<Canvas<'_>> {
    ratatui::Terminal::with_options(
        CrosstermBackend::new(&terminal.screen),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(area),
        },
    )
}

/// Paints the frame the view holds now. The caret is the terminal's own cursor and never a
/// cell: an ink names a foreground, so a caret drawn as a cell is invisible on a space.
fn paint(canvas: &mut Canvas<'_>, view: &mut View) -> io::Result<()> {
    canvas.draw(|frame| {
        view.paint(frame.area(), frame.buffer_mut());
        if let Some(at) = view.cursor(frame.area()) {
            frame.set_cursor_position(at);
        }
    })?;
    Ok(())
}

/// Reads the terminal until the launch ends, one event at a time, and paints only when an
/// event changed the frame: the wait sleeps in `poll` and draws nothing meanwhile, so an idle
/// frame is never drawn. A key, a signal and a live run's fragment are all fds in that wait.
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

    // Read before the first frame was painted: the loop answers these keys here rather than
    // waiting for a second one that may never come.
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
        // A run in flight is the only clock this loop has: its marker steps once per 80 ms, so the
        // wait is that long while one is live and forever while none is.
        let step = running
            .is_some()
            .then(|| Timespec::try_from(Duration::from_millis(pulse::STEP_MS)).ok())
            .flatten();
        match poll(&mut fds, step.as_ref()) {
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(std::io::Error::from(error)),
        }
        let keyed = fds[0].revents().contains(PollFlags::IN);
        let woken = fds[1].revents().contains(PollFlags::IN);

        // The pace has no other clock: a report withheld past its window is said late, never lost.
        telling.due()?;

        if let Some(flight) = &running {
            view.set_pulse(Some(Pulse::live(flight.elapsed_ms())));
        }

        // Read every pass and not only the pass the pipe woke: the word is a flag that is never
        // cleared and the byte is only what wakes the loop, so a `SIGINT` that landed while the
        // loop was awake for something else raised a byte a drain may have taken.
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
                // A launch the user's own word ended starts nothing: the queue waits for a reader
                // who still wants it.
                if let Some(ending) =
                    resume(canvas, view, &mut running, cli, config, &wake, telling)?
                {
                    return Ok(ending);
                }
            }
        }

        // Given back only with nothing left to stop: leaving while a run is live is a screen over a
        // run that is not coming back.
        if interrupted && running.is_none() {
            return Ok(Ending::Cancelled);
        }

        if keyed {
            let count = Read::read(&mut &terminal.keys, &mut read)?;
            // The terminal went away: there is nothing left to quit from, and the guard gives the
            // screen back all the same.
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

        // Not an idle frame: a live run's marker has stepped, so the frame that drew the last glyph
        // is now wrong.
        if !keyed && !woken && running.is_some() {
            paint(canvas, view)?;
        }
    }
}

struct Running {
    cancel: Cancel,
    pending: Arc<Mutex<Vec<Report>>>,
    thread: JoinHandle<()>,
    started: Instant,
}

impl Running {
    /// Drives `prompt` in its own thread: the fragment a wire lands is pushed where the loop takes
    /// it, and the loop is pinged through the pipe a signal uses, so nothing polls for it.
    fn start(launch: Launch, prompt: Prompt, wake: Arc<Wake>) -> Running {
        let cancel = Cancel::new();
        let pending = Arc::new(Mutex::new(Vec::new()));
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
            // `drive` failing leaves no stop reason, which the loop cannot do: the marker settles
            // on the refusal rather than turning for a run nobody is driving.
            let (turns, stop) =
                match launch.drive(prompt.text(), &mut reports, &mut turns, interrupted) {
                    Ok(outcome) => (outcome.turns(), outcome.stopped().clone()),
                    Err(refusal) => (
                        0,
                        Stop::Error(SecretSet::default().redact(refusal.message())),
                    ),
                };
            // Read here, where the run stopped, and carried to the frame: a frame that read the
            // clock when it painted would put a different number beside the same verdict.
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

    /// The user's word: the run stops when the request in flight does, and the command it is in
    /// flight with is killed now.
    fn cancel(&self) {
        self.cancel.now();
    }

    fn elapsed_ms(&self) -> u64 {
        millis(self.started.elapsed())
    }
}

fn millis(span: Duration) -> u64 {
    u64::try_from(span.as_millis()).unwrap_or(u64::MAX)
}

enum Report {
    Text(String),
    Reasoning(String),
    Told(Told),
    Facts(Facts),
    Record(SessionDocument),
    Stopped {
        stop: Stop,
        elapsed_ms: u64,
        turns: u32,
    },
}

struct Reports {
    pending: Arc<Mutex<Vec<Report>>>,
    wake: Arc<Wake>,
}

impl Reports {
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
    /// A turn that asked for calls keeps the run going, so the count a status bar wants moves on; a
    /// turn that asked for none ended the run, and the stop reason is the word a terminal is owed,
    /// since "asking the model" would be a status about a turn this run will never start.
    fn turn(&mut self, ended: &Exchange) {
        if !ended.calls.is_empty() {
            self.push(Report::Told(Told::Asking {
                turn: ended.turn + 1,
            }));
        }
    }

    fn facts(&mut self, facts: Facts) {
        self.push(Report::Facts(facts));
    }

    fn record(&mut self, document: &SessionDocument) {
        self.push(Report::Record(document.clone()));
    }
}

/// Takes everything the run left and puts it on the frame, and says whether the run ended. A run
/// that ended leaves the composer idle, so the next `Ctrl-C` clears the draft instead of
/// cancelling nothing, and its thread is joined rather than left for the process to exit under.
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
            // A status changes no cell: it is written by the one thing here that owns a tty.
            Report::Told(told) => telling.say(told)?,
            Report::Facts(facts) => {
                view.set_facts(facts);
                painted = true;
            }
            Report::Record(document) => {
                view.set_record(&document);
                painted = true;
            }
            Report::Stopped {
                stop,
                elapsed_ms,
                turns,
            } => stopped = Some((stop, elapsed_ms, turns)),
        }
    }
    if let Some((ref stop, elapsed_ms, turns)) = stopped {
        // Said at once, whatever the pace says about a turn: a stop reason is a change of state.
        telling.say(Told::Stopped {
            turn: turns,
            stop: stop.clone(),
        })?;
        // The last frame the marker is drawn on: the wait goes back to forever once the run is
        // gone, so settling draws once and nothing after.
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

/// Answers one event, and says whether the launch ends there: the one place an event becomes a
/// repaint, a run or an exit. Which key means what is the mode stack's decision, not this one's.
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
        // A paste is one edit and never a key, so nothing inside one can send: the composer is
        // handed the whole text with its line breaks in it.
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
        // The flag an outside `SIGINT` sets too: the keyboard and the signal are one path, and the
        // run cannot tell them apart.
        Outcome::Cancel => {
            if let Some(flight) = running {
                flight.cancel();
            }
            paint(canvas, view)?;
            Ok(None)
        }
        Outcome::Send(prompt) => {
            let sent = send(prompt, canvas, view, running, cli, config, wake, telling)?;
            Ok(sent)
        }
        // The keys a reader touched go through the core's owner-only atomic save, and the launch's
        // config is resolved again so the next prompt runs on what the visit left. A value the
        // schema refuses comes back naming its key, with nothing written at all.
        Outcome::Save(touched) => {
            match persist(environment, &touched, cli.flags()) {
                Ok(fresh) => {
                    *config = fresh;
                    // The one switch, as the visit left it: a `status` key turned off here stops
                    // the telling at the next report rather than at the next launch.
                    telling.switch(config.tells_status());
                    view.saved();
                }
                Err(error) => view.refuse(&error.to_string()),
            }
            paint(canvas, view)?;
            Ok(None)
        }
        // Written here because this module is the one that owns the tty, and nothing is repainted:
        // a copy changes no cell of the frame.
        Outcome::Copy(words) => {
            tell(&terminal.screen, &crate::osc::copy(&words))?;
            Ok(None)
        }
        // Written through the settings modal's own save — atomic, 0600, patching only the `model`
        // key — and the config resolved again. A file that will not take it refuses the launch: a
        // name silently not written is a silence this front end must not keep.
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
        // The frame holds no config, so the screen is seeded from the one the launch resolved.
        Outcome::Settings => {
            view.open_settings(config);
            paint(canvas, view)?;
            Ok(None)
        }
        Outcome::Served => {
            *asking = ask(config, wake);
            paint(canvas, view)?;
            Ok(None)
        }
        Outcome::Quit => Ok(Some(Ending::Quit)),
    }
}

/// A wire being asked what it serves, on a thread of its own: `/model` opens its screen at once
/// and the names land when they land, because the loop owes a key its answer now.
struct Asking {
    landing: Arc<Mutex<Option<Listing>>>,
    thread: JoinHandle<()>,
}

/// Asks the wire the config names what it serves, and pings the loop through the pipe a signal
/// uses when the answer is in: one more fd the loop waits on, and no timer.
fn ask(config: &Config, wake: &Arc<Wake>) -> Option<Asking> {
    let config = config.clone();
    let landing = Arc::new(Mutex::new(None));
    let answer = Arc::clone(&landing);
    let wake = Arc::clone(wake);
    let thread = thread::spawn(move || {
        // A launch with no wire is one `main` refused before the screen was taken, so the only
        // answer this thread can leave is a listing.
        if let Some(listing) = crate::launch::served(&config) {
            *lock(&answer) = Some(listing);
            wake.raise();
        }
    });
    // Joined by `served`, not here: nothing blocks, and what the frame paints now is the screen
    // saying it is asking.
    Some(Asking { landing, thread })
}

/// Puts the names a wire answered with on the frame, and drops the thread that asked them: the
/// `/model` screen is painted once with the listing it was waiting for.
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

/// Starts the run a sent draft asks for, on a thread of its own, and says whether the launch ends
/// instead. A launch that cannot run at all ends the launch rather than being drawn: a screen that
/// swallowed the reason nothing ran would be the one silence this front end must not keep.
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
        view.enqueue(prompt);
        paint(canvas, view)?;
        return Ok(None);
    }
    let launch = match Launch::open(cli, config, FrontEnd::Terminal) {
        Ok(launch) => launch,
        Err(refusal) => return Ok(Some(Ending::Refused(refusal))),
    };
    view.settle();
    view.set_state(State::Running);
    *running = Some(Running::start(launch, prompt, Arc::clone(wake)));
    // The marker before the run has said anything: the first step of the cycle and no time on the
    // clock yet, so the frame a prompt is sent from already says a run has begun.
    view.set_pulse(Some(Pulse::live(0)));
    paint(canvas, view)?;
    telling.say(Told::Asking { turn: 1 })?;
    Ok(None)
}

/// Starts the next prompt the queue is holding, once the run before it has stopped: one prompt
/// per stop, oldest first. A launch that cannot run at all ends the launch here as it does from
/// a key — the prompt is already out of the queue, and a reason nothing ran is still said.
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Key(Key),
    Paste(String),
}

enum Step {
    Event(Event),
    Taken,
    Waiting,
}

const ESCAPE: u8 = 0x1b;

/// `ESC [ 2 0 1 ~`: the sequence that ends a bracketed paste, so all before it is one paste.
const PASTE_END: &[u8] = b"\x1b[201~";

/// The bytes a key arrives as, kept until the sequence they start is complete: a terminal is
/// free to send one key in as many reads as it likes.
struct Decoder {
    pending: Vec<u8>,
    /// Whether the newest read was one byte, which is the only thing that tells the `Enter` key
    /// from a paste's line break, and a bare `ESC` from the first byte of a sequence: no keyboard
    /// puts two keys in one read.
    alone: bool,
    pasting: bool,
}

impl Decoder {
    fn new() -> Decoder {
        Decoder {
            pending: Vec::new(),
            alone: false,
            pasting: false,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        self.alone = bytes.len() == 1;
    }

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

    fn one(&mut self) -> Step {
        let Some(&byte) = self.pending.first() else {
            return Step::Waiting;
        };
        // An `ESC` that arrived alone is the Esc key: a sequence that opens with it brings the
        // bytes saying which, and waiting for one a keyboard may never send is a screen nobody can
        // close.
        if byte == ESCAPE && self.pending.len() == 1 && self.alone {
            self.take(1);
            return Step::Event(Event::Key(Key::Escape));
        }
        if byte == ESCAPE {
            return self.escape();
        }
        if byte >= 0x80 {
            if byte < 0xc0 {
                // A byte that starts no glyph: nothing types one, and a decoder that waited on it
                // would never answer the key behind it.
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
            0x15 => Key::Clear,
            0x18 => Key::Drop,
            b'\t' => Key::Tab,
            // `Ctrl-Q`, which is why flow control has to be off: a terminal left with `IXON` eats
            // this byte instead of sending it.
            0x11 => Key::Quit,
            0x19 => Key::Copy,
            0x7f | 0x08 => Key::Backspace,
            // `Ctrl-J` is the plain newline every terminal sends: it breaks a line rather than
            // sending one, and `Enter` is the carriage return below.
            b'\n' => Key::Newline,
            b'\r' if self.alone => Key::Enter,
            // A carriage return that came with more of its burst, before or after it, is a paste's
            // line break: a clipboard's newline reaches a terminal as the byte its Enter key would
            // send, and no line break in a paste may send.
            b'\r' => Key::Newline,
            0x20..=0x7e => Key::Char(char::from(byte)),
            _ => return Step::Taken,
        };
        Step::Event(Event::Key(key))
    }

    fn escape(&mut self) -> Step {
        match self.pending.get(1) {
            None => Step::Waiting,
            Some(b'[') => self.control(),
            Some(b'O') => {
                // The older single-shift arrows: `ESC O A` is the up arrow under a terminal that
                // does not speak the protocol.
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
                // Taken rather than held: holding it would stall every key behind it.
                self.take(1);
                Step::Taken
            }
        }
    }

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
            // A modified arrow says so between the bracket and its final byte: the one spelling
            // terminals use for `Shift+↑` and `Shift+↓`.
            (b'A', _) if shifted(&params) => Step::Event(Event::Key(Key::ShiftUp)),
            (b'B', _) if shifted(&params) => Step::Event(Event::Key(Key::ShiftDown)),
            (b'A', _) => Step::Event(Event::Key(Key::Up)),
            (b'B', _) => Step::Event(Event::Key(Key::Down)),
            (b'C', _) => Step::Event(Event::Key(Key::Right)),
            (b'D', _) => Step::Event(Event::Key(Key::Left)),
            // `Shift+Tab` is `CSI Z` on every terminal, and `CSI 1;2Z` under one that says which
            // modifier it held.
            (b'Z', _) => Step::Event(Event::Key(Key::BackTab)),
            (b'H', _) | (b'~', "1") | (b'~', "7") => Step::Event(Event::Key(Key::Home)),
            (b'F', _) | (b'~', "4") | (b'~', "8") => Step::Event(Event::Key(Key::End)),
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

    /// The bracketed paste that is arriving: everything up to the sequence that ends it, as one
    /// edit. Until that sequence lands there is no event at all, which is what makes a line break
    /// inside a paste unable to send it.
    fn pasted(&mut self) -> Step {
        let Some(end) = find(&self.pending, PASTE_END) else {
            return Step::Waiting;
        };
        let text = String::from_utf8_lossy(&self.pending[..end]).into_owned();
        self.take(end + PASTE_END.len());
        self.pasting = false;
        Step::Event(Event::Paste(text))
    }

    /// One glyph off the front of the bytes, or nothing while its last bytes are still on their
    /// way: a paste in a language this keyboard is not in arrives a read at a time, and half a
    /// glyph is not a glyph.
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

    fn take(&mut self, count: usize) {
        self.pending.drain(..count);
    }
}

/// Whether a control sequence's parameters say the shift key was held: the modifiers are the
/// last field, one more than a mask of shift, alt, ctrl and super.
fn shifted(params: &str) -> bool {
    const SHIFT: u32 = 1;

    let modifiers = params.rsplit(';').next().unwrap_or_default();
    let modifiers = modifiers.split(':').next().unwrap_or_default();
    modifiers
        .parse::<u32>()
        .is_ok_and(|held| held.saturating_sub(1) & SHIFT != 0)
}

fn arrow(last: u8) -> Option<Key> {
    match last {
        b'A' => Some(Key::Up),
        b'B' => Some(Key::Down),
        b'C' => Some(Key::Right),
        b'D' => Some(Key::Left),
        _ => None,
    }
}

/// The key a kitty-protocol report names: `CSI <code> ; <modifiers> u`. Only the reports this
/// frame's map needs are answered — the `Enter` that sends and the two that break a line —
/// because a key nobody answers is not a key.
fn reported(params: &str) -> Option<Key> {
    const SHIFT: u32 = 1;
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

fn find(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes
        .windows(needle.len())
        .position(|window| window == needle)
}

/// What the terminal is to be given back as: the state raw mode replaced and the two fds it is
/// put back through, held where a panic hook can reach them, because an unwinding path runs no
/// guard this module owns.
static GIVE_BACK: Mutex<Option<Restore>> = Mutex::new(None);

struct Restore {
    keys: File,
    screen: File,
    saved: Termios,
}

impl Restore {
    /// Puts the terminal back: the screen is left first, so nothing that follows — a panic message
    /// included — is drawn on a screen that is about to be somebody else's, and then the state raw
    /// mode replaced is restored.
    fn put_back(self) {
        let _ = tell(&self.screen, GIVE_BACK_SCREEN);
        let _ = tcsetattr(&self.keys, OptionalActions::Now, &self.saved);
    }
}

/// Gives the terminal back, exactly once, whichever of the guard and the panic hook asks first:
/// the state is taken out of the static, so the second ask finds nothing and a shell is never
/// handed a half-restored terminal twice.
fn give_back() {
    let taken = lock(&GIVE_BACK).take();
    if let Some(restore) = taken {
        restore.put_back();
    }
}

/// A panic hook, in the shape `std::panic` hands out and takes back: what the hook this module
/// installs keeps of the one it replaced, so a panic still gets reported.
type Hook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync>;

/// Installs, once, the hook that gives the terminal back when a panic unwinds past the guard: a
/// terminal left raw and blank is a shell nobody can type in.
fn guard_panics() {
    static INSTALLED: Once = Once::new();
    static REPLACED: OnceLock<Hook> = OnceLock::new();

    INSTALLED.call_once(|| {
        // The replaced hook is called rather than dropped: a terminal given back is no reason for a
        // panic to go unreported.
        let _ = REPLACED.set(panic::take_hook());
        panic::set_hook(Box::new(|raised| {
            give_back();
            if let Some(replaced) = REPLACED.get() {
                replaced(raised);
            }
        }));
    });
}

/// The pixel size of one cell, as this launch's terminal reports it, and the bytes that were not
/// the answer and are therefore a reader's keys. Three answers, cheapest first: `winsize`, then
/// `CSI 16 t`, then the nominal cell rather than a launch held open.
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

/// Writes a sequence out and flushes it: a frame that sat in a buffer is a frame nobody saw, and
/// a screen given back only at exit is a terminal left blank when the process dies.
fn tell(out: &File, sequence: &[u8]) -> io::Result<()> {
    let mut out = out;
    out.write_all(sequence)?;
    out.flush()
}

/// A lock held for the length of one statement: a panic hook that blocked on a poisoned lock
/// would nest the panic it was called to clean up after.
fn lock<T>(held: &Mutex<T>) -> MutexGuard<'_, T> {
    held.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    //! The one contract an integration test cannot reach from outside the process: a panic unwinds
    //! past no guard this module owns, so the hook is proved here, on a pseudo-terminal of this
    //! process's own, where a panic can be raised and caught and the terminal's state read back.

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

    struct Pty {
        master: File,
        slave: File,
    }

    impl Pty {
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

        fn drawn(&mut self) -> String {
            let mut drawn = [0u8; 4096];
            let mut master = &self.master;
            let count = master.read(&mut drawn).expect("the terminal's screen");
            String::from_utf8_lossy(&drawn[..count]).into_owned()
        }

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
        /// terminal nobody spoke to, and asking that question must not block the way a read would.
        fn silent(&mut self) -> bool {
            let mut fds = [PollFd::new(&self.master, PollFlags::IN)];
            let wait = Timespec::try_from(Duration::from_millis(20)).expect("a span");
            poll(&mut fds, Some(&wait)).is_ok() && !fds[0].revents().contains(PollFlags::IN)
        }

        fn attrs(&self) -> Termios {
            tcgetattr(&self.slave).expect("the terminal's state")
        }
    }

    fn painted_at() -> ratatui::layout::Rect {
        ratatui::layout::Rect::new(0, 0, 80, 24)
    }

    /// A canvas a test paints through is the shape the test gave it, not the shape the terminal
    /// reports: no `/dev/tty` is opened and no `tput` is spawned to answer a question a
    /// pseudo-terminal was never going to answer anyway.
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
    }

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

    /// The two facts the terminal gives reach the view, so a terminal that speaks Kitty is handed
    /// the pictures its frames draw from and one that speaks no protocol is handed none — the frame
    /// it paints is the words of a picture, whole.
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

        // The guard is dropped after the panic has already been cleaned up after, and a terminal
        // handed back twice is a shell with its state overwritten.
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

        // No mark, so `Ctrl-Y` copies the last answer: the words go down the wire the frame does,
        // which is what makes a copy work over SSH.
        let cli = Cli::parse_from(["sindri"]);
        let mut config = sindri_core::config::Config::default();
        let environment = std::collections::BTreeMap::new();
        let wake = Arc::new(Wake::open().expect("a wake"));
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
        //! The bytes a terminal really sends, read as the keys the composer's map answers:
        //! a sequence that arrives in pieces, a paste that arrives as a burst, and the
        //! bracketed paste one is wrapped in.

        use super::super::*;

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

        fn event(bytes: &[u8]) -> Event {
            let mut all = events(&[bytes]);
            assert_eq!(
                all.len(),
                1,
                "one key is one event, and these bytes made {all:?}"
            );
            all.pop().expect("the event")
        }

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
