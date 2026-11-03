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
use sindri_core::status::Channel;

use crate::app::App;
use crate::driver::{Driver, Task};
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
            release_terminal();
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
            release_terminal();
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
    // And it asks for bracketed paste, because a prompt copied from elsewhere
    // is one draft: without the mode a terminal sends the clipboard raw, and
    // every line break in it arrives as an `Enter` that runs a prompt of its
    // own. Asked for where the mouse is, and given back where the mouse is.
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableBracketedPaste);
    take_keys();
    Some(terminal)
}

/// What the frame asked the terminal for, and gives back when it closes: the
/// mouse, bracketed paste, and keys it can tell apart.
fn release_terminal() {
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::event::PopKeyboardEnhancementFlags,
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableBracketedPaste,
    );
}

/// `Shift+Enter` and `Enter` are the same byte (`0x0d`) until the terminal is
/// asked to disambiguate its keys, and the composer breaks the line on the
/// first of them: the frame asks on entry, and the request is a frame of the
/// keyboard protocol's stack, so it is popped again as the frame closes — a
/// shell opened underneath never inherits it. A terminal that does not speak
/// the protocol ignores the sequence and keeps the keys it always sent.
fn take_keys() {
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::event::PushKeyboardEnhancementFlags(
            crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        ),
    );
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
    let jobs = tasks::drive(driver, tx.clone());
    // At most one walk of the workspace at a time: a keystroke during a walk
    // asks for the newest answer, and the frame asks again when that one
    // lands. Typing never queues ten walks.
    let mut walking = false;

    // Nothing has been said yet, so the first frame is drawn here.
    let root = std::env::current_dir().unwrap_or_default();
    let title = app.rail.title();
    let mut screen = Screen {
        terminal,
        reports: Channel::new(title, app.rail.launch().status, Box::new(std::io::stdout())),
    };
    let mut painted = screen.show(&mut app);
    while painted {
        painted = turn_of_the_loop(&mut app, &rx, &jobs, &tx, &mut walking, &root, &mut screen);
    }
    // `done` and `error` are meant to survive a shell prompt; the frame's own
    // record is not, and a tab that has moved on should not hold one for a run
    // that has gone. Written here, so it is in the stream before the alternate
    // screen is left.
    screen.reports.clear();
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
/// asked for, and draw — unless what it heard is something the frame says
/// nothing with. Returns whether the terminal is still there.
fn turn_of_the_loop(
    app: &mut App,
    rx: &Receiver<Msg>,
    jobs: &Sender<Task>,
    tx: &Sender<Msg>,
    walking: &mut bool,
    root: &Path,
    surface: &mut impl Paint,
) -> bool {
    // A report the last second is withholding is a reason to keep the clock
    // running, or the word a settled run owes would wait for a keystroke. So is
    // a run in flight, whose marker counts: nothing else asks for a tick, so a
    // still frame is never drawn for its own sake.
    let live = app.ticks() || *walking || surface.owes(app);
    let draws = match wait(rx, live) {
        Wake::Said(message) => {
            if matches!(message, Msg::Paths(..)) {
                *walking = false;
            }
            apply(app, message)
        }
        Wake::Pulse(delta) => {
            app.pulse(delta);
            true
        }
        Wake::Gone => return false,
    };
    if app.wants_quit() {
        return false;
    }
    // A walk the frame asked for is started whether or not this message draws:
    // it lays nothing out, and the answer it walks for is what a later frame
    // reads — so a draft that moved on is still answered for, whichever
    // generation's paths arrive first.
    walk(app, tx, walking, root);
    // What the frame says nothing with is never a frame drawn — a draw shows
    // the cursor in the composer again and restarts its blink — and the work
    // between a message and its frame goes with it: nothing is laid out twice
    // for a report nothing reads.
    if !draws {
        return true;
    }
    tasks::begin(app, jobs);
    let painted = surface.show(app);
    // The frame just drawn is what asked: a fence it laid out as its panel asked
    // for its diagram, and the drawing is made off this thread so the next frame
    // costs what a frame with no picture in it costs.
    draw_off_thread(app, tx);
    painted
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

/// What the loop paints on: the terminal this crate owns, and — in a test — a
/// counter, so that the loop's own clock is proven without a pty. The reports
/// are part of it because their order against a draw is the point.
trait Paint {
    /// Draws the frame, and says what the frame is doing once the draw has
    /// returned.
    fn show(&mut self, app: &mut App) -> bool;

    /// Whether the terminal is owed a report the second is still withholding.
    fn owes(&self, app: &App) -> bool;
}

/// The one thing that touches the terminal: the frame's draws, and the reports
/// it says into the same screen between them. The two are one value because the
/// order between them is the point — a report is written where a draw has come
/// back, and a report written from inside one is a sequence ratatui would
/// repaint cells around.
struct Screen<'a> {
    terminal: &'a mut DefaultTerminal,
    reports: Channel,
}

impl Paint for Screen<'_> {
    /// The terminal's cursor is hidden while the frame is written and shown
    /// again where the composer puts it. A frame that leaves it showing carries
    /// it wherever the diff writes next, and a streamed answer repaints the
    /// transcript every frame: a reader then watches the cursor leave the
    /// composer and come back. The draw shows it at the position the frame set
    /// and never hides it, so the hiding is the loop's.
    ///
    /// Which is also why an idle frame is drawn only because something
    /// happened: every frame ends by showing the cursor and moving it to the
    /// composer, and a terminal restarts its blink on every move, so a loop that
    /// redrew an unchanged frame would leave an idle composer blinking at the
    /// frame rate. While a run is live the frame is drawn on the pulse, and pays
    /// that blink eight times a second for a marker that moves.
    ///
    /// And what the frame is doing is said here, once the draw has returned:
    /// the reports are the frame's business only in where they are written.
    fn show(&mut self, app: &mut App) -> bool {
        let _ = self.terminal.hide_cursor();
        let painted = self.terminal.draw(|frame| render::draw(app, frame)).is_ok();
        self.reports.say(&app.status(), Instant::now());
        painted
    }

    fn owes(&self, app: &App) -> bool {
        self.reports.owe(&app.status())
    }
}

/// Tells the frame what it heard, and answers whether the frame has anything to
/// draw for it: the gate, and the only one. There is no list of kinds here to
/// keep up to date — a list is a place a later kind is forgotten — because the
/// answer is the message's own: every verb on [`App`] returns whether what it
/// was told left a row, a mark, a cursor or a clock saying something it did not
/// say before, and this hands that answer back unchanged. A kind that turns out
/// to say nothing is a `false` in the module that knows the kind, and the loop
/// draws nothing for it.
fn apply(app: &mut App, message: Msg) -> bool {
    match message {
        Msg::Mouse(mouse) => app.mouse(mouse),
        Msg::Key(key) => app.key(key),
        Msg::Paste(text) => app.paste(&text),
        // The one message the frame holds nothing for and still draws: a resize
        // changes no state at all, and every row of it is laid out at a width
        // the next frame asks the terminal for.
        Msg::Resize => true,
        Msg::Event(event) => app.event(event),
        Msg::Output(side, text) => app.output(side, &text),
        Msg::Over(result) => app.over(result),
        Msg::Finished(finished) => app.finished(finished),
        Msg::Paths(generation, paths, capped) => app.paths(generation, paths, capped),
        Msg::Drawn(body, drawn) => app.drawn(body, drawn),
        Msg::Loaded(loaded) => app.loaded(loaded),
        Msg::Renewed(launch) => app.renewed(launch.map(|launch| *launch)),
        Msg::Reloaded(reloaded) => app.reloaded(reloaded.map(|launch| *launch)),
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

/// Draws the diagrams the frame has asked for on threads of its own, and hands
/// each answer back on the loop's own channel: a mermaid layout is tens of
/// milliseconds, and a frame that waits for one is a keystroke behind a figure.
/// A frame asks for what it lays out and no more, so these are a screenful of
/// threads at most, and each body is asked for once. Nothing is joined — the
/// frame asked, and an answer that arrives after the thread it was asked for has
/// gone is dropped where it lands.
fn draw_off_thread(app: &mut App, tx: &Sender<Msg>) {
    for body in app.take_drawings() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let drawn = crate::picture::draw(&body);
            let _ = tx.send(Msg::Drawn(body, drawn));
        });
    }
}

fn listen_for_keys(tx: Sender<Msg>) {
    std::thread::spawn(move || heard(tx, event::read));
}

/// The loop that hears the terminal, over whatever `read` is: the terminal's
/// own, or a test's. A frame that has stopped hearing cannot be typed at again,
/// so nothing it reads ends the loop — a sequence it cannot make an event of is
/// bytes dropped and a frame still listening, and a paste arriving in some
/// shape it does not know is dropped the same way. The loop ends where the
/// frame does: when the channel it speaks on has no listener left.
fn heard(tx: Sender<Msg>, mut read: impl FnMut() -> std::io::Result<TerminalEvent>) {
    loop {
        let message = match read() {
            Ok(TerminalEvent::Key(key)) => Msg::Key(key),
            Ok(TerminalEvent::Mouse(mouse)) => Msg::Mouse(mouse),
            // A resize has nothing to change in the frame, but it is the one
            // thing that asks for a frame: the draw reads the terminal's new
            // size.
            Ok(TerminalEvent::Resize(..)) => Msg::Resize,
            // A paste arrives whole, and is a draft rather than a keystroke:
            // its line breaks are the composer's rows and never an `Enter`.
            Ok(TerminalEvent::Paste(text)) => Msg::Paste(text),
            // Focus gained or lost is a fact the frame draws nothing with.
            Ok(_) => continue,
            // Bytes no event could be made of: dropped, never the hearing.
            Err(_) => continue,
        };
        if tx.send(message).is_err() {
            return;
        }
    }
}

mod tasks;

#[cfg(test)]
mod tests;
