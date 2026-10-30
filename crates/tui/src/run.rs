//! The one place this crate owns a terminal: raw mode, the alternate screen,
//! the input thread and the render loop. Everything the loop draws comes from
//! [`App`]; everything it runs comes from a [`Driver`] (ADR-0011).

use std::io::IsTerminal;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use crossterm::event::{self, Event as TerminalEvent};
use ratatui::DefaultTerminal;
use sindri_core::cancel::Cancel;

use crate::app::App;
use crate::driver::{Driver, Tap, Task};
use crate::face::Face;
use crate::msg::Msg;
use crate::rail::Launch;
use crate::render;
use crate::settings::Missing;

pub use crate::render::Card;

const CARD_POLL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    Left,
    BootFailed,
}

/// Opens the frame on the alternate screen and runs it until the user leaves.
/// `cancel` is the launch's one flag: the front end has already pointed the
/// process's signals at it, so a `kill -INT` ends the run in flight exactly
/// as `Ctrl-C` does (ADR-0015). `missing` is what the front door found absent
/// and could not fail on: a run with no key opens the frame with the settings
/// modal staged on it (ADR-0004).
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
    // copies rendered rows, and Shift hands the gesture back (ADR-0011).
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture,);
    Some(terminal)
}

fn release_mouse() {
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture,);
}

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

fn session(
    terminal: &mut DefaultTerminal,
    launch: Launch,
    driver: Box<dyn Driver>,
    cancel: Cancel,
    missing: Option<Missing>,
) -> Ending {
    let mut app = recorded(App::new(launch, Face::detect()).with_cancel(cancel));
    if let Some(missing) = missing {
        app = app.with_missing(missing);
    }
    let (tx, rx) = mpsc::channel::<Msg>();
    listen_for_keys(tx.clone());
    let jobs = drive(driver, tx.clone());
    // At most one walk of the workspace at a time: a keystroke during a walk
    // asks for the newest answer, and the frame asks again when that one
    // lands (ADR-0013). Typing never queues ten walks.
    let mut walking = false;

    // The frame is not on a clock: it is drawn when there is something to
    // say, and nothing has been said yet — so the first frame is drawn here.
    let mut painted = paint(terminal, &mut app);
    while painted {
        match rx.recv() {
            Ok(message) => {
                if matches!(message, Msg::Paths(..)) {
                    walking = false;
                }
                apply(&mut app, message);
            }
            Err(_) => break,
        }
        if app.wants_quit() {
            break;
        }
        begin(&mut app, &jobs);
        let root = std::env::current_dir().unwrap_or_default();
        walk(&mut app, &tx, &mut walking, &root);
        painted = paint(terminal, &mut app);
    }
    Ending::Left
}

/// Where the records live and where a default `/export` lands: the state
/// directory is read and never written by the frame, and the workspace is the
/// directory the frame was launched in (ADR-0006).
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
/// never hides it, so the hiding is the loop's (ADR-0011).
///
/// Which is also why a frame is drawn only because something happened: every
/// frame ends by showing the cursor and moving it to the composer, and a
/// terminal restarts its blink on every move. A loop that redrew an unchanged
/// frame would leave an idle composer blinking at the frame rate (ADR-0011).
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
/// asked for it: the composer never waits for a filesystem (ADR-0011).
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
                // terminal's new size (ADR-0011).
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
                workspace: "~/git/sindri".to_string(),
                model: "claude-sonnet-4-6".to_string(),
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
