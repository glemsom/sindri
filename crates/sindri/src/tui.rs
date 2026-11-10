//! The launch: the loop that reads the terminal until it ends, what the terminal does about the
//! outcome a key hands back, where a prompt starts a run, and the endings a launch has.

use std::fs::File;
use std::io::{self, IsTerminal, Read};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustix::event::Timespec;
use rustix::event::{PollFd, PollFlags, poll};
use rustix::termios::{Winsize, tcgetwinsize};
use sindri::composer::{Outcome, Prompt, State};
use sindri::decode::{Decoder, Event};
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
use crate::term::{Canvas, Screen, Terminal, canvas, lock, paint, tell};

/// A launch that cannot be a run, or a screen it cannot draw on: exit 2.
const USAGE: u8 = 2;

/// A terminal that will not take the screen or will not give a key back: exit 3.
const SETUP: u8 = 3;

/// The user's own word from outside the keyboard: the code a `SIGINT` has always meant.
const CANCELLED: u8 = 130;

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
    while let Some(event) = decoder.next_event() {
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
            while let Some(event) = decoder.next_event() {
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
        // The last frame the run itself paints: once it is gone the wait goes back to forever,
        // and the settled marker stays on the frame for every later paint to draw.
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

/// Answers one event, and says whether the launch ends there. Which key means what is the mode
/// stack's decision; what is here is what the terminal does about the outcome it hands back.
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
    // Joined by `served`, not here: nothing blocks, and the frame paints the screen asking
    // until the answer lands.
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

#[cfg(test)]
mod tests {
    use std::os::fd::AsFd;

    use clap::Parser;
    use sindri::composer::Key;
    use sindri::rows::Scroll;
    use sindri_core::records::Entry;
    use sindri_core::redact::SecretSet;

    use crate::term::pty::Pty;
    use crate::term::{Terminal, canvas_at, painted_at};

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
}
