//! The launch: the boundary, the sandbox proof, the session, the record, the tools, the secrets
//! and the client, joined in the one order that holds — nothing is recorded and no turn is asked
//! before the sandbox has proved the boundary.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::asking::Asking;
use crate::bash::BWRAP;
use crate::config::{Config, Environment, Process};
use crate::open::{Ground, Opener};
use crate::outcome::{Facts as Reported, Outcome};
use crate::policy::{Boundary, Enforcement};
use crate::preamble::Preamble;
use crate::provider::copilot::Signing;
use crate::provider::endpoint::Endpoint;
use crate::provider::served::Listing;
use crate::provider::{Ask, Client, Deltas, Route, Usage};
use crate::records::{Entry, Facts, Record, SessionDocument};
use crate::run::{Cancel, Limits, Responder, Run, Shell, Turn};
use crate::sandbox::{self, Proof};
use crate::session::{self, Session};
use crate::skills::{self, Catalog, Diagnostic, home};
use crate::tools::{BASH, FrontEnd, Tool, ToolSet};
use crate::wire::Wire;

/// The exit code for a launch that had nothing to run *with*.
const SETUP: u8 = 3;

/// The exit code for a command line or a config that holds no run.
const USAGE: u8 = 2;

/// Why no run could start: no credential, a sandbox that will not build, a config the schema
/// refuses. No run started, so a refusal is no stop reason and nothing is recorded.
pub struct Refusal {
    message: String,
    code: u8,
}

impl Refusal {
    pub fn new(message: impl Into<String>, code: u8) -> Refusal {
        Refusal {
            message: message.into(),
            code,
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn code(&self) -> u8 {
        self.code
    }
}

/// The machine a launch stands on: the environment its homes and its sign-in are read from, and the
/// program its sandbox is proved with. One value, so the launch reads the outside world through a
/// seam, and a test stands a launch on a machine that refuses on demand.
pub struct Machine<'a> {
    environment: &'a dyn Environment,
    bwrap: PathBuf,
}

impl Machine<'static> {
    /// The machine this process stands on.
    pub fn this() -> Machine<'static> {
        Machine::new(&Process, BWRAP)
    }
}

impl<'a> Machine<'a> {
    pub fn new(environment: &'a dyn Environment, bwrap: impl Into<PathBuf>) -> Machine<'a> {
        Machine {
            environment,
            bwrap: bwrap.into(),
        }
    }
}

pub struct Figure {
    pub wire: Wire,
    pub model: String,
    pub route: Option<Route>,
    pub enforcement: Enforcement,
    pub workspace: PathBuf,
    pub session: PathBuf,
    pub proof: String,
}

pub struct Launch<R = Asking> {
    session: Session,
    boundary: Boundary,
    limits: Limits,
    record: Record,
    asking: R,
    shell: Shell,
    tools: ToolSet,
    catalog: Catalog,
    model: String,
    wire: Wire,
    route: Option<Route>,
    config: Config,
    context_window: Option<u64>,
    measured: Usage,
    proof: Proof,
}

impl Launch<Asking> {
    pub fn open(
        machine: &Machine<'_>,
        config: &Config,
        enforcement: Enforcement,
        front_end: FrontEnd,
        opener: Option<Arc<dyn Opener>>,
    ) -> Result<Launch<Asking>, Refusal> {
        Launch::answered_by(machine, config, enforcement, front_end, opener, Asking::new)
    }
}

impl<R> Launch<R> {
    /// Where a launch's replies come from, for a launch that must not ask the wire: `adapter` is the
    /// one responder every run on it asks, so a test drives the run's loop with a script under it.
    pub fn answered_by(
        machine: &Machine<'_>,
        config: &Config,
        enforcement: Enforcement,
        front_end: FrontEnd,
        opener: Option<Arc<dyn Opener>>,
        adapter: impl FnOnce(Client, Ask, ToolSet) -> R,
    ) -> Result<Launch<R>, Refusal> {
        let (Some(endpoint), Some(model)) = (config.endpoint(), config.model.clone()) else {
            return Err(Refusal::new(
                "a run needs a provider and a model: set them in the config or in the environment",
                USAGE,
            ));
        };
        let state_home = session::state_home(&machine.environment)
            .map_err(|error| Refusal::new(error.to_string(), error.exit_code()))?;
        let session = Session::create(&state_home)
            .map_err(|error| Refusal::new(error.to_string(), error.exit_code()))?;
        Launch::assemble(
            machine,
            config,
            enforcement,
            front_end,
            endpoint,
            model,
            &session,
            opener,
            adapter,
        )
        .inspect_err(|_| session.discard())
    }

    #[allow(clippy::too_many_arguments)]
    fn assemble(
        machine: &Machine<'_>,
        config: &Config,
        enforcement: Enforcement,
        front_end: FrontEnd,
        endpoint: Endpoint,
        model: String,
        session: &Session,
        opener: Option<Arc<dyn Opener>>,
        adapter: impl FnOnce(Client, Ask, ToolSet) -> R,
    ) -> Result<Launch<R>, Refusal> {
        let wire = endpoint.wire();
        let workspace = std::env::current_dir().map_err(|error| {
            Refusal::new(format!("cannot read the working directory: {error}"), SETUP)
        })?;
        let environment = machine.environment;
        let boundary = Boundary::from_config(config, &workspace, enforcement)
            .map_err(|error| Refusal::new(error.to_string(), SETUP))?;
        // Before a turn or a record: the alternative is a run that proceeds unconfined.
        let proof = sandbox::prove_with(&machine.bwrap, &boundary, session.temp())
            .map_err(|card| Refusal::new(card.to_string(), card.exit_code()))?;
        let limits = Limits::from_config(config, &boundary);
        let catalog = Catalog::discover(&workspace, home(&environment).as_deref());
        let tools = ToolSet::new(&boundary, &catalog, front_end);
        let client = Client::for_endpoint(
            &endpoint,
            headers(config),
            session.guid(),
            config.secrets(),
            &environment,
        );
        // A sign-in that cannot be read is a setup fault, refused here rather than at the first
        // turn: nothing has been recorded yet, and a run that cannot be signed in writes nothing.
        if let Some(Signing::Unreadable(reason)) = client.sign_in() {
            return Err(Refusal::new(reason.to_string(), USAGE));
        }
        let ask = ask(config, endpoint, model.clone(), &workspace);
        let route = ask.route_of();
        let thinking = ask.thinks();
        let record = Record::open(
            session,
            Facts {
                wire,
                model: &model,
                route,
                thinking,
                boundary: &boundary,
                limits: &limits,
                workspace: &workspace,
                secrets: client.secrets(),
            },
        )
        .map_err(|error| Refusal::new(error.to_string(), SETUP))?;
        let asking = adapter(client, ask, tools.clone());
        // Resolved once: the ground a frame draws a path against is the one the run judges by.
        let ground = Ground::new(&boundary, session.temp(), &workspace).ok();
        let shell = Shell::new(boundary.clone(), session.temp(), catalog.clone())
            .ground(ground)
            .opener(opener);

        Ok(Launch {
            session: session.clone(),
            boundary,
            limits,
            record,
            asking,
            shell,
            tools,
            catalog,
            model,
            wire,
            route,
            config: config.clone(),
            context_window: config.context_window,
            measured: Usage::default(),
            proof,
        })
    }

    pub fn figure(&self) -> Figure {
        Figure {
            wire: self.wire,
            model: self.model.clone(),
            route: self.route,
            enforcement: self.boundary.enforcement(),
            workspace: self.record.document().workspace.clone(),
            session: self.session.directory().to_path_buf(),
            proof: self.proof.to_string(),
        }
    }

    /// Where this run stands when it judges a target, or nothing when a session has no temp to
    /// resolve: a frame handed none draws every path as the words it was written as.
    pub fn ground(&self) -> Option<Ground> {
        self.shell.grounded().cloned()
    }

    /// The words a run on this launch hands the model about `bash`, the one tool every run
    /// registers.
    pub fn bash_description(&self) -> &str {
        self.tools
            .get(BASH)
            .expect("`bash` is the one tool every run registers")
            .description()
    }

    /// The tools this launch registers, in the order its front end offers them.
    pub fn tools(&self) -> &[Tool] {
        self.tools.tools()
    }

    /// The executor a run on this launch answers the calls those tools name with.
    pub fn executor(&self) -> &Shell {
        &self.shell
    }

    /// The skills a run on this launch would be offered, by name and with the scope each came
    /// from.
    pub fn listing(&self) -> Vec<skills::Listing> {
        self.catalog.listing()
    }

    /// The skill files discovery could not read, and where each one lies.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.catalog.diagnostics()
    }

    /// A launch nobody drove recorded no run, so the session it made goes whole.
    pub fn discard(self) {
        self.session.discard();
    }

    pub fn max_turns(&self) -> u32 {
        self.limits.max_turns()
    }

    /// The session the run on this launch uses, held by whoever started the run: a run owns the
    /// launch while it is in flight, so the temp of the session has to be reachable without it.
    pub fn session(&self) -> Session {
        self.session.clone()
    }

    /// The conversation the session held is over: its temp goes and the directory, the record and the
    /// spill are kept, so a launch that ended still reads.
    pub fn close(&mut self) {
        note(self.record.finish());
    }

    /// Whether a run on this session would still be the run its record names. A session records one
    /// wire, one model, one route, one boundary and one set of dials, so a config that decides
    /// another run is the end of it.
    pub fn runs_on(&self, config: &Config) -> bool {
        self.config.decides_the_same_run(config)
    }
}

impl<R: Responder + Clone> Launch<R> {
    /// Drives `prompt` on the turns this launch has already ended. The session it writes is the one
    /// the prompt before it wrote: the record counts the turns of both, and the temp a command wrote
    /// in is still there.
    pub fn drive(
        &mut self,
        prompt: &str,
        earlier: &[Turn],
        deltas: &mut dyn Deltas,
        turns: &mut dyn Turns,
        cancel: Cancel,
    ) -> Result<Outcome, Refusal> {
        let started = Instant::now();
        let mut run = Run::new(prompt, self.asking.clone(), self.shell.clone())
            .limits(self.limits)
            .tools(&self.tools)
            .earlier(earlier.iter().cloned())
            .cancel(cancel);
        // Ahead of the turn loop, so the transcript opens with the words it answers.
        note(self.record.asked(prompt));
        note(self.record.began());
        hand(turns, self);
        while run.state().is_running() {
            run.turn(deltas);
            if let Some(ended) = run.history().last() {
                note(self.record.wrote(ended));
                // A turn whose report carries no size leaves the one the turn before stated.
                if ended.usage.input.is_some() {
                    self.measured = ended.usage;
                }
                turns.turn(ended);
                hand(turns, self);
            }
        }

        turns.kept(&run.kept());
        let state = run.state();
        note(self.record.charged(&spent(run.history())));
        note(self.record.stopped(&state));
        hand(turns, self);
        let outcome = Outcome::of(
            &run,
            Reported {
                session: self.session.guid(),
                model: &self.model,
                enforcement: self.boundary.enforcement(),
                duration: started.elapsed(),
                usage: self.record.document().usage,
            },
        );
        outcome.ok_or_else(|| Refusal::new("the run ended with no stop reason to report", SETUP))
    }
}

/// What a launch hands whoever drives it when its record moves: the session it writes, the document
/// and transcript it holds, and the size the newest report measured against the window the reader
/// named.
pub struct Handed<'a> {
    pub session: &'a Session,
    pub document: &'a SessionDocument,
    pub transcript: &'a [Entry],
    pub measured: Usage,
    pub window: Option<u64>,
}

pub trait Turns {
    fn turn(&mut self, ended: &Turn);

    /// Handed once, when the run stops: the prompt it was asked with beside the answer it got, which
    /// is what a conversation keeps for the prompt after it.
    fn kept(&mut self, _kept: &Turn) {}

    /// Handed when the run opens, as each turn ends, and once more when it stops.
    fn facts(&mut self, _facts: Handed<'_>) {}
}

impl Turns for () {
    fn turn(&mut self, _ended: &Turn) {}
}

/// What the turns of one run reported spending, as the record's running total takes it: a responder
/// that spends nothing of its own still writes what each reply said it cost.
fn spent(history: &[Turn]) -> Usage {
    let mut spent = Usage::default();
    for turn in history {
        spent.charge(&turn.usage);
    }
    spent
}

fn hand<R>(turns: &mut dyn Turns, launch: &Launch<R>) {
    turns.facts(Handed {
        session: &launch.session,
        document: launch.record.document(),
        transcript: launch.record.transcript(),
        measured: launch.measured,
        window: launch.context_window,
    });
}

/// The names a wire serves, read off the endpoint a run would ask: not a run, so the client a
/// listing signs in with is the one a run builds, with no session to expand.
pub fn served(machine: &Machine<'_>, config: &Config) -> Option<Listing> {
    let endpoint = config.endpoint()?;
    let client = Client::for_endpoint(
        &endpoint,
        headers(config),
        String::new(),
        config.secrets(),
        &machine.environment,
    );
    Some(client.served(&endpoint, &config.credential(), &endpoint))
}

fn ask(config: &Config, endpoint: Endpoint, model: String, workspace: &std::path::Path) -> Ask {
    Ask::of(endpoint, model)
        .preamble(Preamble::for_workspace(workspace).render())
        .credential(config.credential())
        .thinking(config.thinking.unwrap_or(false))
}

/// A line with no colon is dropped rather than sent as a header with an empty value.
fn headers(config: &Config) -> Vec<(String, String)> {
    config
        .extra_headers
        .iter()
        .flatten()
        .filter_map(|header| {
            let (name, value) = header.split_once(':')?;
            Some((name.trim().to_owned(), value.trim().to_owned()))
        })
        .collect()
}

/// A record that would not write is a diagnostic: the run it belongs to goes on regardless.
fn note(written: Result<(), impl std::fmt::Display>) {
    if let Err(error) = written {
        eprintln!("sindri: {error}");
    }
}
