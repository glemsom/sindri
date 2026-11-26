use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::asking::Asking;
use crate::bash::BWRAP;
use crate::config::{Config, Environment, Process};
use crate::helpers::Helpers;
use crate::open::{Ground, Opener};
use crate::outcome::{Facts as Reported, Outcome};
use crate::policy::{Boundary, Enforcement};
use crate::preamble::Preamble;
use crate::provider::copilot::Signing;
use crate::provider::endpoint::Endpoint;
use crate::provider::served::Listing;
use crate::provider::{Ask, Client, Deltas, Route, Usage};
use crate::records::{Entry, Facts, Record, SessionDocument, Told};
use crate::run::{Cancel, Limits, Loaded, Responder, Run, Settled, Shell, Turn, Witness};
use crate::sandbox::{self, Proof};
use crate::session::{self, Session};
use crate::skills::{self, Catalog, Diagnostic, home};
use crate::tools::{BASH, FrontEnd, Tool, ToolSet};
use crate::wire::Wire;

const SETUP: u8 = 3;

const USAGE: u8 = 2;

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

pub(crate) struct Preparation {
    config: Config,
    inherited: Option<crate::handoff::Handoff>,
}

impl Preparation {
    pub(crate) fn resolve(
        environment: &impl Environment,
        overrides: Config,
    ) -> Result<Self, crate::config::ConfigError> {
        let inherited = crate::handoff::Handoff::read(environment).map_err(|reason| {
            crate::config::ConfigError::BadValue {
                origin: "inherited launch".to_owned(),
                reason,
            }
        })?;
        let config = match &inherited {
            Some(handoff) => handoff.config.clone(),
            None => Config::resolve_ambient(environment, overrides)?,
        };
        Ok(Self { config, inherited })
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    pub(crate) fn refresh(
        &self,
        environment: &impl Environment,
        overrides: Config,
    ) -> Result<Config, crate::config::ConfigError> {
        match &self.inherited {
            Some(_) => Ok(self.config.clone()),
            None => Config::resolve_ambient(environment, overrides),
        }
    }

    pub(crate) fn into_config(self) -> Config {
        self.config
    }

    pub(crate) fn environment<'a>(&'a self, ambient: &'a impl Environment) -> &'a dyn Environment {
        match &self.inherited {
            Some(handoff) => &handoff.environment,
            None => ambient,
        }
    }
}

pub struct Machine<'a> {
    environment: &'a dyn Environment,
    bwrap: PathBuf,
}

impl Machine<'static> {
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
    loaded: Loaded,
    proof: Proof,
    helpers: Helpers,
}

impl<R> Launch<R> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepared_by(
        machine: &Machine<'_>,
        preparation: &Preparation,
        config: &Config,
        enforcement: Enforcement,
        front_end: FrontEnd,
        opener: Option<Arc<dyn Opener>>,
        adapter: impl FnOnce(Client, Ask, ToolSet) -> R,
    ) -> Result<Launch<R>, Refusal> {
        let config = if preparation.inherited.is_some() {
            preparation.config()
        } else {
            config
        };
        let (Some(endpoint), Some(model)) = (config.endpoint(), config.model.clone()) else {
            return Err(Refusal::new(
                "a run needs a provider and a model: set them in the config or in the environment",
                USAGE,
            ));
        };
        let environment = preparation.environment(&machine.environment);
        let state_home = session::state_home(&environment)
            .map_err(|error| Refusal::new(error.to_string(), error.exit_code()))?;
        let session = Session::create(&state_home)
            .map_err(|error| Refusal::new(error.to_string(), error.exit_code()))?;
        Launch::assemble(
            machine,
            preparation,
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
        preparation: &Preparation,
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
        let boundary = match &preparation.inherited {
            Some(handoff) => handoff.boundary.clone(),
            None => Boundary::from_config(config, &workspace, enforcement)
                .map_err(|error| Refusal::new(error.to_string(), SETUP))?,
        };
        let discovery_workspace = preparation
            .inherited
            .as_ref()
            .map(|handoff| handoff.workspace.as_path())
            .unwrap_or(&workspace);
        let environment = preparation.environment(&machine.environment);
        // Proved before a turn is asked or a record written: nothing here proceeds unconfined.
        let proof = sandbox::prove_with(&machine.bwrap, &boundary, session.temp())
            .map_err(|card| Refusal::new(card.to_string(), card.exit_code()))?;
        let limits = Limits::from_config(config, &boundary);
        let catalog = Catalog::discover(discovery_workspace, home(&environment).as_deref());
        let tools = ToolSet::new(&boundary, &catalog, front_end);
        let client = Client::for_endpoint(
            &endpoint,
            headers(config),
            session.guid(),
            config.secrets(),
            &environment,
        );
        if let Some(Signing::Unreadable(reason)) = client.sign_in() {
            return Err(Refusal::new(reason.to_string(), USAGE));
        }
        let helpers = Helpers::probe(&boundary, session.temp(), &machine.bwrap);
        let preamble = Preamble::for_workspace(&workspace)
            .helpers(helpers.clone())
            .render();
        let ask = ask(config, endpoint, model.clone(), preamble);
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
        let ground = Ground::new(&boundary, session.temp(), &workspace).ok();
        let handoff = crate::handoff::Handoff::write(
            config,
            &boundary,
            discovery_workspace,
            session.temp(),
            &environment,
        )
        .map_err(|reason| Refusal::new(reason, SETUP))?;
        let executable = std::env::current_exe()
            .map_err(|_| Refusal::new("cannot locate child executable", SETUP))?;
        let shell = Shell::new(boundary.clone(), session.temp(), catalog.clone())
            .handoff(crate::handoff::ChildLaunch {
                executable,
                transport: handoff,
            })
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
            loaded: Loaded::default(),
            proof,
            helpers,
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

    pub fn ground(&self) -> Option<Ground> {
        self.shell.grounded().cloned()
    }

    pub fn helpers(&self) -> &Helpers {
        &self.helpers
    }

    pub fn bash_description(&self) -> &str {
        self.tools
            .get(BASH)
            .expect("`bash` is the one tool every run registers")
            .description()
    }

    pub fn tools(&self) -> &[Tool] {
        self.tools.tools()
    }

    pub fn executor(&self) -> &Shell {
        &self.shell
    }

    pub fn loaded(&self) -> &Loaded {
        &self.loaded
    }

    pub fn listing(&self) -> Vec<skills::Listing> {
        self.catalog.listing()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.catalog.diagnostics()
    }

    pub fn discard(self) {
        self.session.discard();
    }

    pub fn max_turns(&self) -> u32 {
        self.limits.max_turns()
    }

    pub fn session(&self) -> Session {
        self.session.clone()
    }

    pub fn close(&mut self) {
        note(self.record.finish());
    }

    pub fn runs_on(&self, config: &Config) -> bool {
        self.config.decides_the_same_run(config)
    }
}

impl<R: Responder + Clone> Launch<R> {
    pub fn drive(
        &mut self,
        prompt: &str,
        earlier: &[Turn],
        deltas: &mut dyn Deltas,
        turns: &mut dyn Turns,
        cancel: Cancel,
    ) -> Result<Outcome, Refusal> {
        let started = Instant::now();
        let injected = self.loaded.missing(earlier);
        turns.retained(&injected);
        let earlier: Vec<Turn> = earlier.iter().chain(&injected).cloned().collect();
        let mut run = Run::new(prompt, self.asking.clone(), self.shell.clone())
            .limits(self.limits)
            .tools(&self.tools)
            .earlier(earlier.iter().cloned())
            .loaded(self.loaded.clone())
            .cancel(cancel);
        note(self.record.asked(prompt));
        run.drive(
            deltas,
            &mut Recording {
                launch: self,
                turns,
            },
        );
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

pub struct Handed<'a> {
    pub session: &'a Session,
    pub document: &'a SessionDocument,
    pub transcript: &'a [Entry],
    pub measured: Usage,
    pub window: Option<u64>,
    pub loaded: &'a Loaded,
}

pub trait Turns {
    fn turn(&mut self, ended: &Turn);

    fn told(&mut self, _told: Told) {}

    fn kept(&mut self, _kept: &Turn) {}

    fn retained(&mut self, _retained: &[Turn]) {}

    fn facts(&mut self, _facts: Handed<'_>) {}
}

impl Turns for () {
    fn turn(&mut self, _ended: &Turn) {}
}

struct Recording<'a, R> {
    launch: &'a mut Launch<R>,
    turns: &'a mut dyn Turns,
}

impl<R> Witness for Recording<'_, R> {
    fn began(&mut self) {
        note(self.launch.record.began());
        hand(self.turns, self.launch);
    }

    fn ended(&mut self, ended: &Turn) {
        note(self.launch.record.wrote(ended));
        if ended.usage.input.is_some() {
            self.launch.measured = ended.usage;
        }
        self.turns.turn(ended);
        hand(self.turns, self.launch);
    }

    fn told(&mut self, told: Told) {
        note(self.launch.record.told(told.clone()));
        self.turns.told(told);
    }

    fn settled(&mut self, settled: &Settled<'_>) {
        self.turns.kept(settled.kept);
        self.turns.retained(settled.retained);
        note(self.launch.record.charged(settled.spent));
        note(self.launch.record.stopped(settled.state));
        hand(self.turns, self.launch);
    }
}

fn hand<R>(turns: &mut dyn Turns, launch: &Launch<R>) {
    turns.facts(Handed {
        session: &launch.session,
        document: launch.record.document(),
        transcript: launch.record.transcript(),
        measured: launch.measured,
        window: launch.context_window,
        loaded: &launch.loaded,
    });
}

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

fn ask(config: &Config, endpoint: Endpoint, model: String, preamble: String) -> Ask {
    Ask::of(endpoint, model)
        .capacity(
            config.context_window,
            config.output_headroom.unwrap_or(4096),
            config.tool_result_headroom.unwrap_or(4096),
        )
        .preamble(preamble)
        .credential(config.credential())
        .thinking(config.thinking.unwrap_or(false))
}

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

fn note(written: Result<(), impl std::fmt::Display>) {
    if let Err(error) = written {
        eprintln!("sindri: {error}");
    }
}
