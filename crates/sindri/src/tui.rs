use std::fs::File;
use std::io::{self, Read};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use rustix::event::Timespec;
use rustix::event::{PollFd, PollFlags, poll};
use sindri::capability::{Asked, Capabilities};
use sindri::composer::{Outcome, Prompt};
use sindri::decode::{Decoder, Event};
use sindri::facts::Facts;
use sindri::pointer;
use sindri::pulse::{self, Pulse};
use sindri::view::{Painted, Transcript, View};
use sindri_core::config::{Config, Environment, Process, Touched};
use sindri_core::launch::{Launch, Machine, Refusal};
use sindri_core::provider::Deltas;
use sindri_core::records::Told;
use sindri_core::session::Session;
use sindri_core::tools::FrontEnd;

use crate::cli::Cli;
use crate::open::HostOpener;
use crate::reports::{Asking, Report, Running, ask};
use crate::signal::{Causes, Wake};
use crate::term::{Canvas, Screen, Terminal, missing_terminal, paint, tell};
use crate::window::{self, Window};

const USAGE: u8 = 2;

const SETUP: u8 = 3;

const CANCELLED: u8 = 130;

const MODEL: &str = "model";

const GRACE: Duration = Duration::from_secs(2);

pub fn run(cli: &Cli, first_run: sindri_core::first_run::FirstRun) -> ExitCode {
    let config = first_run.config();
    if let Some(reason) = missing_terminal() {
        return crate::refuse(&reason, USAGE);
    }
    // Opened before the screen is taken: a `SIGINT` that arrives between raw mode and the loop
    // has to be a cancellation, not a process dying with the terminal left raw and the screen up.
    let wake = match Wake::installed() {
        Ok(wake) => wake,
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
    // The same directory `FirstRun::open` gives a run, read the same way, so a path a reader
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
    let boot = sindri::ledger::Boot::of(config, cli.enforcement(), &workspace);
    // Asked once, before the screen is taken: the terminal that takes the window's sequences is the
    // one asked to report its focus, and one nobody can vouch for is asked nothing at all.
    let windowed = window::speaks(
        Process.var("TERM").as_deref(),
        Process.var("TERM_PROGRAM").as_deref(),
    );
    let pointer = pointer::speaks(
        Process.var("TERM").as_deref(),
        Process.var("TERM_PROGRAM").as_deref(),
        Process.var("VTE_VERSION").as_deref(),
    );
    let screen = match Screen::take(&terminal, windowed, pointer) {
        Ok(screen) => screen,
        Err(error) => {
            return crate::refuse(&format!("the terminal would not go raw: {error}"), SETUP);
        }
    };
    let (capabilities, typed) = Capabilities::probed(&Process, &terminal, config.draws_face());
    let view = View::of(
        capabilities,
        Facts::boot(boot, sindri_core::skills::home(&Process)),
        Transcript::none(),
    );
    // One telling for the whole launch, so the pace a status is told at is the launch's: the
    // screen it writes on is a second handle on the descriptor a frame is drawn on.
    let telling = match terminal.screen.try_clone() {
        Ok(screen) => Window::new(screen, config.tells_status(), &workspace, windowed),
        Err(error) => {
            return crate::refuse(&format!("the terminal could not be told: {error}"), SETUP);
        }
    };
    let mut live = match Loop::new(
        &terminal,
        cli,
        config.clone(),
        first_run,
        &Process,
        view,
        telling,
        Arc::clone(&wake),
    ) {
        Ok(live) => live,
        Err(error) => {
            return crate::refuse(&format!("the frame could not be painted: {error}"), SETUP);
        }
    };
    if let Err(error) = live.paint() {
        return crate::refuse(&format!("the frame could not be painted: {error}"), SETUP);
    }
    if let Err(error) = live.told(Told::Idle) {
        return crate::refuse(&format!("the status could not be written: {error}"), SETUP);
    }
    let keys = live.keys(&typed);
    let launch = live.ending();
    drop(live);
    // Given back before a word is said about the ending: a refusal is a line for the shell, and
    // not a smear over a frame.
    drop(screen);
    if let Some(mut ending) = launch {
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

struct Loop<'a> {
    terminal: &'a Terminal,
    canvas: Canvas<'a>,
    view: View,
    cli: &'a Cli,
    config: Config,
    first_run: sindri_core::first_run::FirstRun,
    environment: &'a dyn Environment,
    telling: Window<File>,
    launch: Option<Launch>,
    session: Option<Session>,
    running: Option<Running>,
    asking: Option<Asking>,
    painted: Painted,
    wake: Arc<Wake>,
    opener: HostOpener,
}

impl<'a> Loop<'a> {
    #[cfg(not(test))]
    fn canvas<'t>(terminal: &'t Terminal, environment: &dyn Environment) -> io::Result<Canvas<'t>> {
        let (term, program, vte) = (
            environment.var("TERM"),
            environment.var("TERM_PROGRAM"),
            environment.var("VTE_VERSION"),
        );
        crate::term::canvas(
            terminal,
            sindri::link::speaks(term.as_deref(), program.as_deref(), vte.as_deref()),
        )
    }

    #[cfg(test)]
    fn canvas<'t>(
        terminal: &'t Terminal,
        _environment: &dyn Environment,
    ) -> io::Result<Canvas<'t>> {
        crate::term::canvas_at(terminal, crate::term::painted_at())
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        terminal: &'a Terminal,
        cli: &'a Cli,
        config: Config,
        first_run: sindri_core::first_run::FirstRun,
        environment: &'a dyn Environment,
        view: View,
        telling: Window<File>,
        wake: Arc<Wake>,
    ) -> io::Result<Loop<'a>> {
        Ok(Loop {
            terminal,
            canvas: Self::canvas(terminal, environment)?,
            view,
            cli,
            config,
            first_run,
            environment,
            telling,
            launch: None,
            session: None,
            running: None,
            asking: None,
            painted: Painted::none(),
            wake,
            opener: HostOpener::host(),
        })
    }

    fn paint(&mut self) -> io::Result<()> {
        self.view.celled(self.terminal.winsize());
        self.painted = paint(&mut self.canvas, &mut self.view)?;
        Ok(())
    }

    fn told(&mut self, told: Told) -> io::Result<()> {
        self.telling.say(told)
    }

    fn keys(&mut self, typed: &[u8]) -> io::Result<Ending> {
        let mut decoder = Decoder::new();
        let mut read = [0u8; 4096];
        let mut interrupted = false;
        // Owned, not borrowed from the loop: the reader the poll waits on is held across the
        // answer that borrows the loop mutably.
        let wake = Arc::clone(&self.wake);

        decoder.push(typed);
        while let Some(event) = decoder.next_event() {
            if let Some(ending) = self.answer(event)? {
                return Ok(ending);
            }
        }

        loop {
            let waiting = wake.reader();
            let mut fds = [
                PollFd::new(&self.terminal.keys, PollFlags::IN),
                PollFd::new(&waiting, PollFlags::IN),
            ];
            // A run in flight is the only clock this loop has: its marker steps once per 80 ms, so
            // that is how long the wait times out while one is live, and no timer runs while none
            // is — a window change's byte ends the wait either way.
            let step = self
                .running
                .is_some()
                .then(|| Timespec::try_from(Duration::from_millis(pulse::STEP_MS)).ok())
                .flatten();
            match poll(&mut fds, step.as_ref()) {
                Ok(_) | Err(rustix::io::Errno::INTR) => {}
                Err(error) => return Err(std::io::Error::from(error)),
            }
            let keyed = fds[0].revents().contains(PollFlags::IN);
            let woken = fds[1].revents().contains(PollFlags::IN);

            self.telling.due()?;

            if let Some(flight) = &self.running {
                let elapsed_ms = flight.elapsed_ms();
                let facts = self
                    .view
                    .facts()
                    .clone()
                    .pulsed(Some(Pulse::live(elapsed_ms)));
                self.view.handed(facts);
            }

            // Drained before the causes are read and not after: a cause can be raised while the loop
            // is painting, and a cancel whose byte this drain took away would otherwise wait for a
            // key that is never coming.
            wake.drain();
            let causes = wake.causes();

            if causes.holds(Causes::CANCEL) && !interrupted {
                interrupted = true;
                if let Some(flight) = &self.running {
                    flight.cancel();
                }
            }

            if woken {
                self.served()?;
                if self.take()? && !interrupted {
                    self.resume()?;
                }
            }

            // Given back only with nothing left to stop: leaving while a run is live is a screen
            // over a run that is not coming back.
            if interrupted && self.running.is_none() {
                return Ok(Ending::Cancelled);
            }

            if keyed {
                let count = Read::read(&mut &self.terminal.keys, &mut read)?;
                if count == 0 {
                    return Ok(Ending::Quit);
                }
                decoder.push(&read[..count]);
                while let Some(event) = decoder.next_event() {
                    if let Some(ending) = self.answer(event)? {
                        return Ok(ending);
                    }
                }
            }

            if causes.holds(Causes::RESIZE) || (!keyed && !woken && self.running.is_some()) {
                self.paint()?;
            }
        }
    }

    fn answer(&mut self, event: Event) -> io::Result<Option<Ending>> {
        let outcome = match event {
            // A paste is one edit and never a key, so nothing inside one can send: the composer
            // is handed the whole text with its line breaks in it.
            Event::Paste(text) => {
                self.view.paste(&text);
                self.paint()?;
                return Ok(None);
            }
            Event::Focus(focused) => {
                self.view.handed(self.view.facts().clone().focused(focused));
                self.telling.focused(focused);
                self.paint()?;
                return Ok(None);
            }
            Event::Key(key) => self.view.key(key),
            Event::Pointer(pointer) => self.view.pointer(&self.painted, pointer),
        };
        match outcome {
            Outcome::Unchanged => Ok(None),
            Outcome::Edited => {
                self.paint()?;
                Ok(None)
            }
            // The flag an outside `SIGINT` sets too: the keyboard and the signal are one path,
            // and the run cannot tell them apart.
            Outcome::Cancel => {
                if let Some(flight) = &self.running {
                    flight.cancel();
                }
                self.paint()?;
                Ok(None)
            }
            Outcome::Send(prompt) => {
                self.send(prompt)?;
                Ok(None)
            }
            Outcome::Save(touched) => {
                match self
                    .first_run
                    .persist(&self.environment, &touched, self.cli.flags())
                {
                    Ok(fresh) => {
                        self.config = fresh;
                        self.telling.switch(self.config.tells_status())?;
                        let capabilities = self
                            .view
                            .capabilities()
                            .clone()
                            .faced(self.config.draws_face());
                        self.view.drawn_on(capabilities);
                        self.view.saved();
                    }
                    Err(error) => self.view.refuse(&error.to_string()),
                }
                self.paint()?;
                Ok(None)
            }
            Outcome::Copy(words) => {
                tell(&self.terminal.screen, &crate::osc::copy(&words))?;
                self.paint()?;
                Ok(None)
            }
            // Written through the settings card's own save — atomic, 0600, patching only the `model`
            // key — and the config resolved again. A file that will not take it refuses
            // the launch: a name silently not written is a silence this front end must not keep.
            Outcome::Model(name) => {
                let mut touched = Touched::new();
                touched.set(MODEL, name);
                match self
                    .first_run
                    .persist(&self.environment, &touched, self.cli.flags())
                {
                    Ok(fresh) => {
                        self.config = fresh;
                        self.paint()?;
                        Ok(None)
                    }
                    Err(error) => Ok(Some(Ending::Refused(Refusal::new(
                        error.to_string(),
                        error.exit_code(),
                    )))),
                }
            }
            Outcome::Settings => {
                self.view.open_settings(&self.config);
                self.paint()?;
                Ok(None)
            }
            Outcome::Served => {
                let config = if self.view.mode() == sindri::keys::Mode::Settings {
                    match self.view.settings().draft(&self.config) {
                        Ok(draft) => draft,
                        Err(error) => {
                            self.view.refuse(&error.to_string());
                            self.paint()?;
                            return Ok(None);
                        }
                    }
                } else {
                    self.config.clone()
                };
                self.asking = ask(&config, &self.wake);
                self.paint()?;
                Ok(None)
            }
            Outcome::Quit => Ok(Some(Ending::Quit)),
        }
    }

    fn send(&mut self, prompt: Prompt) -> io::Result<()> {
        if self.running.is_some() {
            self.view.enqueue(prompt);
            self.paint()?;
            return Ok(());
        }
        let launching = match self.launch.take() {
            Some(held) if held.runs_on(&self.config) => held,
            ended => {
                if let Some(mut ended) = ended {
                    ended.close();
                }
                let opened = self.first_run.open(
                    &Machine::this(),
                    &self.config,
                    self.cli.enforcement(),
                    FrontEnd::Terminal,
                    Some(Arc::new(self.opener.clone())),
                );
                match opened {
                    Ok(opened) => opened,
                    Err(refusal) => {
                        self.view.run_refused(refusal.message(), prompt);
                        self.paint()?;
                        return Ok(());
                    }
                }
            }
        };
        self.session = Some(launching.session());
        self.view.settle();
        let ground = launching.ground();
        self.view
            .handed(self.view.facts().clone().grounded(ground).running());
        let earlier = self.view.earlier();
        self.running = Some(Running::start(
            launching,
            prompt,
            earlier,
            Arc::clone(&self.wake),
        ));
        self.view
            .handed(self.view.facts().clone().pulsed(Some(Pulse::live(0))));
        self.paint()
    }

    fn ending(&mut self) -> Option<Launch> {
        if let Some(flight) = self.running.take() {
            flight.cancel();
            if flight.finished_within(GRACE) {
                self.launch = flight.handed_back();
            }
        }
        if let Some(mut session) = self.session.take()
            && let Err(error) = session.finish()
        {
            eprintln!("sindri: {error}");
        }
        self.launch.take()
    }

    fn resume(&mut self) -> io::Result<()> {
        let Some(prompt) = self.view.dequeue() else {
            return Ok(());
        };
        self.send(prompt)
    }

    fn take(&mut self) -> io::Result<bool> {
        let Some(flight) = self.running.as_mut() else {
            return Ok(false);
        };
        let landed = flight.landed();
        let mut painted = false;
        let mut stopped = None;
        for report in landed {
            match report {
                Report::Text(fragment) => {
                    self.view.text(&fragment);
                    painted = true;
                }
                Report::Reasoning(fragment) => {
                    self.view.reasoning(&fragment);
                    painted = true;
                }
                Report::Told(told) => self.told(told)?,
                Report::Facts(rail) => {
                    for name in self.view.facts().loaded().names() {
                        rail.loaded.load(name);
                    }
                    self.view.handed(self.view.facts().clone().railed(rail));
                    painted = true;
                }
                Report::Record {
                    document,
                    transcript,
                } => {
                    self.view.handed(
                        self.view
                            .facts()
                            .clone()
                            .recorded(&document)
                            .transcribed(transcript),
                    );
                    painted = true;
                }
                Report::Kept(kept) => self.view.kept(&kept),
                Report::Stopped { stop, elapsed_ms } => stopped = Some((stop, elapsed_ms)),
            }
        }
        if let Some((ref stop, elapsed_ms)) = stopped {
            let facts = self
                .view
                .facts()
                .clone()
                .pulsed(Some(Pulse::settled(stop, elapsed_ms)))
                .idle();
            self.view.handed(facts);
            self.view.stopped(stop);
            painted = true;
            if let Some(flight) = self.running.take()
                && let Some(taken) = flight.handed_back()
            {
                self.launch = Some(taken);
            }
        }
        if painted {
            self.paint()?;
        }
        Ok(stopped.is_some())
    }

    fn served(&mut self) -> io::Result<()> {
        let landed = self.asking.as_mut().and_then(Asking::landed);
        let Some(listing) = landed else {
            return Ok(());
        };
        if self.view.mode() != sindri::keys::Mode::Settings
            || self
                .view
                .settings()
                .draft(&self.config)
                .ok()
                .and_then(|draft| draft.endpoint())
                .as_ref()
                == Some(listing.at())
        {
            self.view
                .handed(self.view.facts().clone().served_by(listing));
        }
        self.paint()?;
        if let Some(flight) = self.asking.take() {
            flight.joined();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
