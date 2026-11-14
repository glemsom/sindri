//! The launch: the loop that reads the terminal until it ends, what the terminal does about the
//! outcome a key hands back, where a prompt starts a run, and the endings a launch has.

use std::fs::File;
use std::io::{self, Read};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use rustix::event::Timespec;
use rustix::event::{PollFd, PollFlags, poll};
use sindri::composer::{Outcome, Prompt, State};
use sindri::decode::{Decoder, Event};
use sindri::link;
use sindri::palette::Palette;
use sindri::pointer;
use sindri::pulse::{self, Pulse};
use sindri::settings::persist;
use sindri::size;
use sindri::view::View;
use sindri_core::config::{Config, Environment, Process, Touched};
use sindri_core::records::Told;
use sindri_core::tools::FrontEnd;

use crate::cell::probed_cell;
use crate::cli::Cli;
use crate::launch::{Launch, Refusal};
use crate::reports::{Asking, Running, ask, served, take};
use crate::signal::{Signals, Wake};
use crate::sizing::probed_sizing;
use crate::term::{Canvas, Screen, Terminal, canvas, hand_pictures, missing_terminal, paint, tell};
use crate::window::{self, Window};

/// A launch that cannot be a run, or a screen it cannot draw on: exit 2.
const USAGE: u8 = 2;

/// A terminal that will not take the screen or will not give a key back: exit 3.
const SETUP: u8 = 3;

/// The user's own word from outside the keyboard: the code a `SIGINT` has always meant.
const CANCELLED: u8 = 130;

const MODEL: &str = "model";

/// The frame a launch opens on, as far as the config alone decides it.
fn opened(config: &Config) -> View {
    let mut view = View::boot();
    view.set_face(config.draws_face());
    view
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
    // The same directory `Launch::open` gives a run, read the same way, so a path a reader
    // picks is one the run's own commands can read. Read before the screen is taken, so a
    // refusal is a line on the reader's own shell.
    let workspace = match std::env::current_dir() {
        Ok(workspace) => workspace,
        Err(error) => {
            return crate::refuse(
                &format!("cannot read the working directory: {error}"),
                SETUP,
            );
        }
    };
    // What the run the prompt will ask for is, read behind the reader's own shell rather than
    // behind a blank frame: the repository is asked for its branch once, before the first paint.
    let boot = crate::launch::boot(cli, config, &workspace);
    let named = |key: &str| std::env::var(key).ok();
    // Asked once, before the screen is taken: the terminal that takes the window's sequences is the
    // one asked to report its focus, and one nobody can vouch for is asked nothing at all.
    let windowed = window::speaks(named("TERM").as_deref(), named("TERM_PROGRAM").as_deref());
    // Asked once, before the screen is taken: a terminal nobody can vouch for is handed a frame
    // that takes no pointer, exactly as it is drawn today.
    let pointers = pointer::speaks(named("TERM").as_deref(), named("TERM_PROGRAM").as_deref());
    let screen = match Screen::take(&terminal, windowed, pointers) {
        Ok(screen) => screen,
        Err(error) => {
            return crate::refuse(&format!("the terminal would not go raw: {error}"), SETUP);
        }
    };
    // The cell size is asked before the first frame, and a key typed while the launch waited is
    // handed to the loop rather than dropped.
    let (cell_size, mut typed) = probed_cell(&terminal);
    let mut view = opened(config);
    view.set_cell_size(cell_size);
    // Resolved once, before the first frame: what a terminal can do is a fact of the launch and
    // not a question every paint asks again.
    view.set_palette(Palette::probed(
        named("NO_COLOR").as_deref(),
        named("TERM").as_deref(),
        named("COLORTERM").as_deref(),
    ));
    hand_pictures(
        &mut view,
        cell_size,
        named("TERM").as_deref(),
        named("TERM_PROGRAM").as_deref(),
    );
    // A terminal that names itself after one that draws large text is asked whether it does, and
    // one that says nothing is handed the spelling its glyphs are.
    let sizing = match size::speaks(named("TERM").as_deref(), named("TERM_PROGRAM").as_deref()) {
        true => {
            let (sizing, keys) = probed_sizing(&terminal);
            typed.extend(keys);
            sizing
        }
        false => false,
    };
    view.set_sizing(sizing);
    view.set_pointer(pointers);
    // Resolved once with the palette: a terminal that speaks no hyperlink is handed the words
    // and no sequence, and nothing asks again on a key.
    let places = link::speaks(
        named("TERM").as_deref(),
        named("TERM_PROGRAM").as_deref(),
        named("VTE_VERSION").as_deref(),
    );
    view.set_workspace(workspace.clone());
    // Read the way `launch` reads it, so the name `/skill:name` accepts and a name a run could
    // call come from one catalog.
    view.set_home(crate::launch::home());
    view.door(boot);
    let mut canvas = match canvas(&terminal, places) {
        Ok(canvas) => canvas,
        Err(error) => {
            return crate::refuse(&format!("the frame could not be painted: {error}"), SETUP);
        }
    };
    if let Err(error) = paint(&mut canvas, &mut view) {
        return crate::refuse(&format!("the frame could not be painted: {error}"), SETUP);
    }
    // One telling for the whole launch, so the pace a status is told at is the launch's: the
    // screen it writes on is a second handle on the descriptor a frame is drawn on.
    let mut telling = match terminal.screen.try_clone() {
        Ok(screen) => Window::new(screen, config.tells_status(), &workspace, windowed),
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
    // The session is the launch's and not a run's: the first prompt opens it, every prompt after runs
    // in it, and it ends with the launch.
    let mut launch: Option<Launch> = None;
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
        &mut launch,
    );
    // Given back before a word is said about the ending: a refusal is a line for the shell, and
    // not a smear over a frame.
    drop(screen);
    // The conversation the session held is over, so its temp goes before the process does.
    if let Some(mut ending) = launch.take() {
        ending.close();
    }
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

/// Reads the terminal until the launch ends, one event at a time, and paints only when an
/// event changed the frame: the wait sleeps in `poll` and draws nothing meanwhile, so an idle
/// frame is never drawn. A key, a signal and a live run's fragment are all fds in that wait.
#[allow(
    clippy::too_many_arguments,
    reason = "the one seam every mode's key is handed through, the telling a run is announced to, and the launch the session lasts on"
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
    telling: &mut Window<File>,
    launch: &mut Option<Launch>,
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
            launch,
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
            if take(canvas, view, &mut running, telling, launch)? && !interrupted {
                // A launch the user's own word ended starts nothing: the queue waits for a reader
                // who still wants it.
                resume(
                    canvas,
                    view,
                    &mut running,
                    launch,
                    cli,
                    config,
                    &wake,
                    telling,
                )?;
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
                    launch,
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
    telling: &mut Window<File>,
    launch: &mut Option<Launch>,
) -> io::Result<Option<Ending>> {
    let outcome = match event {
        // A paste is one edit and never a key, so nothing inside one can send: the composer is
        // handed the whole text with its line breaks in it.
        Event::Paste(text) => {
            view.paste(&text);
            paint(canvas, view)?;
            return Ok(None);
        }
        // The one report a reader never sent: the frame recedes while nobody is reading it, and a
        // run that ends while it is away is the one that raises a notification.
        Event::Focus(focused) => {
            view.set_focused(focused);
            telling.focused(focused);
            paint(canvas, view)?;
            return Ok(None);
        }
        // The one seam a pointer's act and a key's act both come through: what the frame makes of
        // each, and what the terminal does about it, is the same.
        Event::Key(key) => view.key(key),
        Event::Pointer(pointer) => view.pointer(pointer),
    };
    match outcome {
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
            send(
                prompt, canvas, view, running, launch, cli, config, wake, telling,
            )?;
            Ok(None)
        }
        // The keys a reader touched go through the core's owner-only atomic save, and the launch's
        // config is resolved again so the next prompt runs on what the visit left. A value the
        // schema refuses comes back naming its key, with nothing written at all.
        Outcome::Save(touched) => {
            match persist(environment, &touched, cli.flags()) {
                Ok(fresh) => {
                    *config = fresh;
                    // Both switches, as the visit left them: `status` turned off here stops the
                    // telling at the next report, and `face` turned off repaints the frame without
                    // one, rather than at the next launch.
                    telling.switch(config.tells_status())?;
                    view.set_face(config.draws_face());
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

/// Starts the run a sent draft asks for, on a thread of its own. A session the launch already holds
/// is the one it runs in, and one that cannot run at all is drawn rather than being the end of the
/// launch: a screen that swallowed the reason would be one silence.
#[allow(
    clippy::too_many_arguments,
    reason = "the one seam a prompt starts a run through, and the telling it is announced to"
)]
fn send(
    prompt: Prompt,
    canvas: &mut Canvas<'_>,
    view: &mut View,
    running: &mut Option<Running>,
    launch: &mut Option<Launch>,
    cli: &Cli,
    config: &Config,
    wake: &Arc<Wake>,
    telling: &mut Window<File>,
) -> io::Result<()> {
    if running.is_some() {
        view.enqueue(prompt);
        paint(canvas, view)?;
        return Ok(());
    }
    let launching = match launch.take() {
        Some(held) if held.runs_on(config) => held,
        // The session a launch holds records one wire, model, route and boundary: a config that names
        // another is the end of that session, and the prompt after it opens the next.
        ended => {
            if let Some(mut ended) = ended {
                ended.close();
            }
            match Launch::open(cli, config, FrontEnd::Terminal) {
                Ok(opened) => opened,
                Err(refusal) => {
                    view.run_refused(refusal.message(), prompt);
                    paint(canvas, view)?;
                    return Ok(());
                }
            }
        }
    };
    view.settle();
    // The ground a path a call names is drawn against is the one the run it starts will judge by.
    view.set_ground(launching.ground());
    view.set_state(State::Running);
    *running = Some(Running::start(
        launching,
        prompt,
        view.earlier(),
        Arc::clone(wake),
    ));
    // The marker before the run has said anything: the first step of the cycle and no time on the
    // clock yet, so the frame a prompt is sent from already says a run has begun.
    view.set_pulse(Some(Pulse::live(0)));
    paint(canvas, view)?;
    telling.say(Told::Asking { turn: 1 })?;
    Ok(())
}

/// Starts the next prompt the queue is holding, once the run before it has stopped: one prompt
/// per stop, oldest first. A launch that cannot run is the row `send` draws, and the prompt it
/// takes with it out of the queue is the frame's again.
#[allow(
    clippy::too_many_arguments,
    reason = "the same seam `send` is, plus the prompt a run that stopped left queued"
)]
fn resume(
    canvas: &mut Canvas<'_>,
    view: &mut View,
    running: &mut Option<Running>,
    launch: &mut Option<Launch>,
    cli: &Cli,
    config: &mut Config,
    wake: &Arc<Wake>,
    telling: &mut Window<File>,
) -> io::Result<()> {
    let Some(prompt) = view.dequeue() else {
        return Ok(());
    };
    send(
        prompt, canvas, view, running, launch, cli, config, wake, telling,
    )
}

#[cfg(test)]
mod tests;
