//! The one place this crate owns a terminal: raw mode, the alternate screen,
//! the input thread and the render loop. Everything the loop draws comes from
//! [`App`]; everything it runs comes from a [`Driver`] (ADR-0011 §1).

use std::io::IsTerminal;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use crossterm::event::{self, Event as TerminalEvent};
use ratatui::DefaultTerminal;

use crate::app::App;
use crate::driver::{Driver, Job, Tap};
use crate::face::Face;
use crate::msg::Msg;
use crate::rail::Launch;
use crate::render;

pub use crate::render::Card;

/// How long the frame waits for something to happen before it looks again.
const FRAME: Duration = Duration::from_millis(16);
/// How often a card that is waiting to be dismissed looks for a key.
const CARD_POLL: Duration = Duration::from_millis(200);

/// How the session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// The user quit.
    Left,
    /// Nothing could run, so nothing did.
    BootFailed,
}

/// Opens the frame on the alternate screen and runs it until the user leaves.
pub fn open(launch: Launch, driver: Box<dyn Driver>) -> Ending {
    match alternate_screen() {
        Some(mut terminal) => {
            let ending = session(&mut terminal, launch, driver);
            ratatui::restore();
            ending
        }
        None => Ending::BootFailed,
    }
}

/// Draws the card a failed boot leaves behind, and holds it until the user has
/// read it: nothing else is ever drawn.
pub fn refuse(card: Card) -> Ending {
    match alternate_screen() {
        Some(mut terminal) => {
            let ending = show_card(&mut terminal, &card);
            ratatui::restore();
            ending
        }
        None => Ending::BootFailed,
    }
}

/// The alternate screen, or `None` where there is no terminal to open one on:
/// nothing is drawn to a stream that is not one, because a card that cannot be
/// seen is a card that cannot be dismissed.
fn alternate_screen() -> Option<DefaultTerminal> {
    std::io::stdout().is_terminal().then(ratatui::init)
}

/// The card a failed boot draws, held until the user has read it.
fn show_card(terminal: &mut DefaultTerminal, card: &Card) -> Ending {
    let mut dismissed = false;
    while !dismissed {
        if terminal
            .draw(|frame| render::draw_card(card, frame))
            .is_err()
        {
            break;
        }
        if event::poll(CARD_POLL).unwrap_or(false) {
            match event::read() {
                Ok(TerminalEvent::Key(_)) | Ok(TerminalEvent::Resize(..)) => dismissed = true,
                Ok(_) => {}
                Err(_) => break,
            }
        }
    }
    Ending::BootFailed
}

/// The session itself: keys in, events in, one frame out.
fn session(terminal: &mut DefaultTerminal, launch: Launch, driver: Box<dyn Driver>) -> Ending {
    let mut app = App::new(launch, Face::detect());
    let (tx, rx) = mpsc::channel::<Msg>();
    listen_for_keys(tx.clone());
    let jobs = drive(driver, tx.clone());

    loop {
        match rx.recv_timeout(FRAME) {
            Ok(message) => apply(&mut app, message),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if app.wants_quit() {
            break;
        }
        begin(&mut app, &jobs);
        if terminal
            .draw(|frame| render::draw(&mut app, frame))
            .is_err()
        {
            break;
        }
    }
    Ending::Left
}

/// One message from anywhere, applied to the frame.
fn apply(app: &mut App, message: Msg) {
    match message {
        Msg::Key(key) => app.key(key),
        Msg::Event(event) => app.event(event),
        Msg::Output(side, text) => app.output(side, &text),
        Msg::Over(over) => app.over(over),
        Msg::Finished(finished) => app.finished(finished),
    }
}

/// Starts the next prompt when the frame is idle: the queue first, then the
/// draft. A prompt typed mid-run waits here, not in the model's way.
fn begin(app: &mut App, jobs: &Sender<Job>) {
    while let Some(job) = app.start() {
        if jobs.send(job).is_err() {
            break;
        }
    }
}

/// Reads keys on their own thread, so the frame never blocks on a terminal that
/// says nothing.
fn listen_for_keys(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        loop {
            match event::read() {
                Ok(TerminalEvent::Key(key)) => {
                    if tx.send(Msg::Key(key)).is_err() {
                        return;
                    }
                }
                // A resize redraws because the next frame asks the terminal
                // its size; there is nothing for the frame to be told.
                Ok(TerminalEvent::Resize(..)) => {}
                Ok(_) => {}
                Err(_) => return,
            }
        }
    });
}

/// Runs prompts, one at a time, on their own thread: the frame keeps drawing
/// while the model is thinking.
fn drive(mut driver: Box<dyn Driver>, tx: Sender<Msg>) -> Sender<Job> {
    let (jobs, queue) = mpsc::channel::<Job>();
    std::thread::spawn(move || {
        for job in queue {
            let finished = driver.run(&job.prompt, job.cancel, Tap::new(tx.clone()));
            if tx.send(Msg::Finished(finished)).is_err() {
                return;
            }
        }
    });
    jobs
}
