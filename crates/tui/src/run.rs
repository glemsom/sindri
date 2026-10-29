//! The one place this crate owns a terminal: raw mode, the alternate screen,
//! the input thread and the render loop. Everything the loop draws comes from
//! [`App`]; everything it runs comes from a [`Driver`] (ADR-0011 §1).

use std::io::IsTerminal;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use crossterm::event::{self, Event as TerminalEvent};
use ratatui::DefaultTerminal;

use crate::app::App;
use crate::driver::{Driver, Tap, Task};
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
            release_mouse();
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
            release_mouse();
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
    if !std::io::stdout().is_terminal() {
        return None;
    }
    let terminal = ratatui::init();
    // The frame takes the mouse because it draws its own selection: a drag
    // copies rendered rows, and Shift hands the gesture back (ADR-0011 §7).
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture,);
    Some(terminal)
}

/// Hands the mouse back to the terminal, whatever happens next.
fn release_mouse() {
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture,);
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
    // At most one walk of the workspace at a time: a keystroke during a walk
    // asks for the newest answer, and the frame asks again when that one lands
    // (ADR-0013). Typing never queues ten walks.
    let mut walking = false;

    loop {
        match rx.recv_timeout(FRAME) {
            Ok(message) => {
                if matches!(message, Msg::Paths(..)) {
                    walking = false;
                }
                apply(&mut app, message);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if app.wants_quit() {
            break;
        }
        begin(&mut app, &jobs);
        let root = std::env::current_dir().unwrap_or_default();
        walk(&mut app, &tx, &mut walking, &root);
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
        Msg::Mouse(mouse) => app.mouse(mouse),
        Msg::Event(event) => app.event(event),
        Msg::Output(side, text) => app.output(side, &text),
        Msg::Over(over, spilled) => app.over(over, spilled),
        Msg::Finished(finished) => app.finished(finished),
        Msg::Paths(generation, paths, capped) => app.paths(generation, paths, capped),
        Msg::Renewed(launch) => app.renewed(launch),
    }
}

/// Starts the next prompt when the frame is idle: the queue first, then the
/// draft. A prompt typed mid-run waits here, not in the model's way.
fn begin(app: &mut App, jobs: &Sender<Task>) {
    if app.take_renew() && jobs.send(Task::Renew).is_err() {
        return;
    }
    while let Some(job) = app.start() {
        if jobs.send(Task::Run(job)).is_err() {
            break;
        }
    }
}

/// Walks the workspace on a thread of its own for the completion that just
/// asked for it: the composer never waits for a filesystem (ADR-0011 §16).
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
        let found = crate::completion::scan(&root, crate::completion::CAP);
        let paths: Vec<String> = found
            .paths
            .into_iter()
            .filter(|path| crate::completion::visible(path, &typed))
            .collect();
        let _ = tx.send(Msg::Paths(generation, paths, found.capped));
    });
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
                Ok(TerminalEvent::Mouse(mouse)) => {
                    if tx.send(Msg::Mouse(mouse)).is_err() {
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
fn drive(mut driver: Box<dyn Driver>, tx: Sender<Msg>) -> Sender<Task> {
    let (tasks, queue) = mpsc::channel::<Task>();
    std::thread::spawn(move || {
        for task in queue {
            let sent = match task {
                Task::Run(job) => {
                    let finished = driver.run(&job.prompt, job.cancel, Tap::new(tx.clone()));
                    tx.send(Msg::Finished(finished))
                }
                // `/new`: a fresh GUID and a fresh record, in the same frame.
                Task::Renew => tx.send(Msg::Renewed(driver.renew())),
            };
            if sent.is_err() {
                return;
            }
        }
    });
    tasks
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::walk;
    use crate::app::App;
    use crate::msg::Msg;
    use crate::rail::Launch;

    fn app() -> App {
        App::with_clipboard(
            Launch {
                session: "4f1c8f2e".to_string(),
                temp: "/tmp".to_string(),
                branch: None,
                sandbox: "bwrap".to_string(),
                max_turns: 1,
                context_limit: None,
            },
            None,
            Box::new(crate::copy::System::new(
                false,
                String::new(),
                Box::new(std::io::sink()),
            )),
        )
    }

    #[test]
    fn a_keystroke_starts_one_walk_and_its_answer_names_the_completion() {
        let root = tempfile::tempdir().expect("a workspace");
        std::fs::create_dir_all(root.path().join(".git")).expect("a git directory");
        std::fs::write(root.path().join("main.rs"), "").expect("a file");
        std::fs::write(root.path().join(".hidden"), "").expect("a dotfile");

        let mut app = app();
        for ch in "@ma".chars() {
            app.key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(ch),
                crossterm::event::KeyModifiers::NONE,
            ));
        }
        let (tx, rx) = mpsc::channel();
        let mut walking = false;
        walk(&mut app, &tx, &mut walking, root.path());
        assert!(walking, "a walk is in flight");
        walk(&mut app, &tx, &mut walking, root.path());
        assert!(
            rx.try_recv().is_err(),
            "a second keystroke does not start a second walk"
        );

        let Ok(Msg::Paths(generation, paths, capped)) = rx.recv() else {
            panic!("the walk answered with paths");
        };
        assert_eq!(Some(generation), app.completion().map(|c| c.generation));
        assert!(paths.contains(&"main.rs".to_string()), "found: {paths:?}");
        assert!(
            !paths.iter().any(|path| path.starts_with('.')),
            "dotfiles need a dot: {paths:?}"
        );
        assert!(!capped, "a small tree is walked whole");
    }
}
