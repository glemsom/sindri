//! The one place this crate owns a terminal: raw mode, the alternate screen,
//! the input thread and the render loop. Everything the loop draws comes from
//! [`App`]; everything it runs comes from a [`Driver`].

use std::io::IsTerminal;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event as TerminalEvent};
use ratatui::DefaultTerminal;
use sindri_core::cancel::Cancel;

use crate::app::App;
use crate::driver::{Driver, Tap, Task};
use crate::face::Face;
use crate::graphics::Graphics;
use crate::msg::Msg;
use crate::pulse;
use crate::rail::Launch;
use crate::render;
use crate::settings::Missing;

pub use crate::render::Card;

const CARD_POLL: Duration = Duration::from_millis(200);
/// How often the loop looks at the clock while there is something to look at:
/// a live run or a completion being walked.
const PULSE: Duration = pulse::TICK;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    Left,
    BootFailed,
}

/// Opens the frame on the alternate screen and runs it until the user leaves.
/// `cancel` is the launch's one flag: the front end has already pointed the
/// process's signals at it, so a `kill -INT` ends the run in flight exactly
/// as `Ctrl-C` does. `missing` is what the front door found absent
/// and could not fail on: a run with no key opens the frame with the settings
/// modal staged on it.
pub fn open(
    launch: Launch,
    driver: Box<dyn Driver>,
    cancel: Cancel,
    missing: Option<Missing>,
) -> Ending {
    match alternate_screen() {
        Some(mut terminal) => {
            let ending = session(&mut terminal, launch, driver, cancel, missing);
            release_mouse();
            ratatui::restore();
            ending
        }
        None => Ending::BootFailed,
    }
}

pub fn refuse(card: Card) -> Ending {
    match alternate_screen() {
        Some(mut terminal) => {
            let ending = show_card(&mut terminal, &card);
            release_mouse();
            ratatui::restore();
            ending
        }
        None => Ending::BootFailed,
    }
}

/// The alternate screen, or `None` where there is no terminal to open one on:
/// nothing is drawn to a stream that is not one, because a card that cannot
/// be seen is a card that cannot be dismissed.
fn alternate_screen() -> Option<DefaultTerminal> {
    if !std::io::stdout().is_terminal() {
        return None;
    }
    let terminal = ratatui::init();
    // The frame takes the mouse because it draws its own selection: a drag
    // copies rendered rows, and Shift hands the gesture back.
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture,);
    Some(terminal)
}

fn release_mouse() {
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture,);
}

/// The card stays until it is read: a key leaves it, the page keys move a
/// report taller than the screen, and a resize only lays it out again — a
/// `tmux` client attaching is not the reader saying they have seen it.
fn show_card(terminal: &mut DefaultTerminal, card: &Card) -> Ending {
    let mut view = crate::card::View::default();
    loop {
        if terminal
            .draw(|frame| render::draw_card(card, &mut view, frame))
            .is_err()
        {
            break;
        }
        if !event::poll(CARD_POLL).unwrap_or(false) {
            continue;
        }
        match event::read() {
            Ok(event) => match crate::card::answer(&event, view.more()) {
                crate::card::Answer::Page { up } => view.page(up),
                crate::card::Answer::Leave => break,
                crate::card::Answer::Redraw => {}
            },
            Err(_) => break,
        }
    }
    Ending::BootFailed
}

fn session(
    terminal: &mut DefaultTerminal,
    launch: Launch,
    driver: Box<dyn Driver>,
    cancel: Cancel,
    missing: Option<Missing>,
) -> Ending {
    // One terminal, one question, asked once: the face is drawn from this answer,
    // and the picture draws from the same one.
    let graphics = Graphics::detect();
    let face = graphics.as_ref().and_then(Face::new);
    let mut app = recorded(
        App::new(launch, face)
            .with_graphics(graphics)
            .with_cancel(cancel),
    );
    if let Some(missing) = missing {
        app = app.with_missing(missing);
    }
    let (tx, rx) = mpsc::channel::<Msg>();
    listen_for_keys(tx.clone());
    let jobs = drive(driver, tx.clone());
    // At most one walk of the workspace at a time: a keystroke during a walk
    // asks for the newest answer, and the frame asks again when that one
    // lands. Typing never queues ten walks.
    let mut walking = false;

    // Nothing has been said yet, so the first frame is drawn here.
    let root = std::env::current_dir().unwrap_or_default();
    let mut draw = |app: &mut App| paint(terminal, app);
    let mut painted = draw(&mut app);
    while painted {
        painted = turn_of_the_loop(&mut app, &rx, &jobs, &tx, &mut walking, &root, &mut draw);
    }
    Ending::Left
}

/// What woke the loop: something said, the clock, or nothing more to hear.
enum Wake {
    Said(Msg),
    Pulse(Duration),
    Gone,
}

/// Waits for the next thing to happen. A frame with something happening in it
/// is on the clock, so it is woken by it; a still frame waits for a message and
/// pays nothing for the wait.
fn wait(rx: &Receiver<Msg>, live: bool) -> Wake {
    if !live {
        return match rx.recv() {
            Ok(message) => Wake::Said(message),
            Err(_) => Wake::Gone,
        };
    }
    let asked = Instant::now();
    match rx.recv_timeout(PULSE) {
        Ok(message) => Wake::Said(message),
        // How long it actually waited, not how long it meant to: work
        // between two frames must not be missing from the run's clock.
        Err(RecvTimeoutError::Timeout) => Wake::Pulse(asked.elapsed()),
        Err(RecvTimeoutError::Disconnected) => Wake::Gone,
    }
}

/// One turn of the loop: wait, tell the frame what it heard, start whatever it
/// asked for, and draw. Returns whether the terminal is still there.
fn turn_of_the_loop(
    app: &mut App,
    rx: &Receiver<Msg>,
    jobs: &Sender<Task>,
    tx: &Sender<Msg>,
    walking: &mut bool,
    root: &Path,
    draw: &mut impl FnMut(&mut App) -> bool,
) -> bool {
    let live = app.is_running() || *walking;
    match wait(rx, live) {
        Wake::Said(message) => {
            if matches!(message, Msg::Paths(..)) {
                *walking = false;
            }
            apply(app, message);
        }
        Wake::Pulse(delta) => app.pulse(delta),
        Wake::Gone => return false,
    }
    if app.wants_quit() {
        return false;
    }
    begin(app, jobs);
    walk(app, tx, walking, root);
    draw(app)
}

/// Where the records live and where a default `/export` lands: the state
/// directory is read and never written by the frame, and the workspace is the
/// directory the frame was launched in.
fn recorded(app: App) -> App {
    let app = match std::env::current_dir() {
        Ok(workspace) => app.with_workspace(workspace),
        Err(_) => app,
    };
    match sindri_core::paths::state_dir() {
        Ok(state) => app.with_sessions(state.join("sessions")),
        Err(_) => app,
    }
}

/// The terminal's cursor is hidden while the frame is written and shown again
/// where the composer puts it. A frame that leaves it showing carries it
/// wherever the diff writes next, and a streamed answer repaints the
/// transcript every frame: a reader then watches the cursor leave the
/// composer and come back. `draw` shows it at the position the frame set and
/// never hides it, so the hiding is the loop's.
///
/// Which is also why an idle frame is drawn only because something happened:
/// every frame ends by showing the cursor and moving it to the composer, and a
/// terminal restarts its blink on every move, so a loop that redrew an
/// unchanged frame would leave an idle composer blinking at the frame rate.
/// While a run is live the frame is drawn on the pulse, and pays that blink
/// eight times a second for a marker that moves.
fn paint(terminal: &mut DefaultTerminal, app: &mut App) -> bool {
    let _ = terminal.hide_cursor();
    terminal.draw(|frame| render::draw(app, frame)).is_ok()
}

fn apply(app: &mut App, message: Msg) {
    match message {
        Msg::Key(key) => app.key(key),
        Msg::Mouse(mouse) => app.mouse(mouse),
        Msg::Resize => {}
        Msg::Event(event) => app.event(event),
        Msg::Output(side, text) => app.output(side, &text),
        Msg::Over(over, spilled) => app.over(over, spilled),
        Msg::Finished(finished) => app.finished(finished),
        Msg::Paths(generation, paths, capped) => app.paths(generation, paths, capped),
        Msg::Renewed(launch) => app.renewed(launch),
        Msg::Reloaded(reloaded) => app.reloaded(reloaded),
    }
}

fn begin(app: &mut App, jobs: &Sender<Task>) {
    if app.take_renew() && jobs.send(Task::Renew).is_err() {
        return;
    }
    if app.take_reload() && jobs.send(Task::Reload).is_err() {
        return;
    }
    while let Some(job) = app.start() {
        if jobs.send(Task::Run(job)).is_err() {
            break;
        }
    }
}

/// Walks the workspace on a thread of its own for the completion that just
/// asked for it: the composer never waits for a filesystem.
fn walk(app: &mut App, tx: &Sender<Msg>, walking: &mut bool, root: &std::path::Path) {
    if *walking {
        return;
    }
    let Some((generation, typed)) = app.take_walk() else {
        return;
    };
    *walking = true;
    let root = root.to_path_buf();
    let tx = tx.clone();
    std::thread::spawn(move || {
        let found = crate::completion::walk::scan(&root, crate::completion::walk::CAP);
        let paths: Vec<String> = found
            .paths
            .into_iter()
            .filter(|path| crate::completion::walk::visible(path, &typed))
            .collect();
        let _ = tx.send(Msg::Paths(generation, paths, found.capped));
    });
}

fn listen_for_keys(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        loop {
            match event::read() {
                Ok(TerminalEvent::Key(key)) => {
                    if tx.send(Msg::Key(key)).is_err() {
                        return;
                    }
                }
                Ok(TerminalEvent::Mouse(mouse)) => {
                    if tx.send(Msg::Mouse(mouse)).is_err() {
                        return;
                    }
                }
                // A resize has nothing to change in the frame, but it is the
                // one thing that asks for a frame: the draw reads the
                // terminal's new size.
                Ok(TerminalEvent::Resize(..)) => {
                    if tx.send(Msg::Resize).is_err() {
                        return;
                    }
                }
                Ok(_) => {}
                Err(_) => return,
            }
        }
    });
}

fn drive(mut driver: Box<dyn Driver>, tx: Sender<Msg>) -> Sender<Task> {
    let (tasks, queue) = mpsc::channel::<Task>();
    std::thread::spawn(move || {
        for task in queue {
            let sent = match task {
                Task::Run(job) => {
                    let finished = driver.run(&job.prompt, job.cancel, Tap::new(tx.clone()));
                    tx.send(Msg::Finished(finished))
                }
                Task::Renew => tx.send(Msg::Renewed(driver.renew())),
                Task::Reload => tx.send(Msg::Reloaded(driver.reload())),
            };
            if sent.is_err() {
                return;
            }
        }
    });
    tasks
}

#[cfg(test)]
mod tests;
