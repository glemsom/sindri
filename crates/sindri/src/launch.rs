use std::path::PathBuf;
use std::time::Instant;

use sindri::rail;
use sindri_core::asking::Asking;
use sindri_core::config::{Config, Process};
use sindri_core::outcome::{Facts as Reported, Outcome};
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::preamble::Preamble;
use sindri_core::provider::copilot::Store;
use sindri_core::provider::served::{Listing, Where};
use sindri_core::provider::{Ask, Client, Credential, Deltas, Route, Usage};
use sindri_core::records::{Facts, Record, SessionDocument};
use sindri_core::run::{Cancel, Exchange, Limits, Run, Shell};
use sindri_core::sandbox::{self, Proof};
use sindri_core::session::{self, Session};
use sindri_core::skills::Catalog;
use sindri_core::tools::{FrontEnd, ToolSet};
use sindri_core::wire::Wire;

use crate::cli::Cli;

/// The exit code for a launch that had nothing to run *with*.
const SETUP: u8 = 3;

/// The exit code for a command line or a config that holds no run.
const USAGE: u8 = 2;

/// What a front end says about a launch that cannot run, and exits with.
///
/// A refusal is not a stop reason: no run started, so nothing was recorded and stdout is empty.
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

pub trait Turns {
    fn turn(&mut self, ended: &Exchange);

    /// Handed when the run opens, as each turn ends, and once more when it stops.
    fn facts(&mut self, _facts: rail::Facts) {}

    /// The record as it stands, handed beside the facts so a screen never reads a file mid-run.
    fn record(&mut self, _document: &SessionDocument) {}
}

impl Turns for () {
    fn turn(&mut self, _ended: &Exchange) {}
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

pub struct Launch {
    session: Session,
    boundary: Boundary,
    limits: Limits,
    record: Record,
    asking: Asking,
    shell: Shell,
    tools: ToolSet,
    model: String,
    wire: Wire,
    route: Option<Route>,
    context_window: Option<u64>,
    proof: Proof,
    started: Instant,
}

impl Launch {
    pub fn open(cli: &Cli, config: &Config, front_end: FrontEnd) -> Result<Launch, Refusal> {
        let started = Instant::now();
        let (Some(wire), Some(model)) = (config.provider, config.model.clone()) else {
            return Err(Refusal::new(
                "a run needs a provider and a model: set them in the config or in the environment",
                USAGE,
            ));
        };
        let workspace = std::env::current_dir().map_err(|error| {
            Refusal::new(format!("cannot read the working directory: {error}"), SETUP)
        })?;
        let state_home = session::state_home(&Process)
            .map_err(|error| Refusal::new(error.to_string(), error.exit_code()))?;
        let session = Session::create(&state_home)
            .map_err(|error| Refusal::new(error.to_string(), error.exit_code()))?;
        let boundary = Boundary::from_config(config, &workspace, cli.enforcement())
            .map_err(|error| Refusal::new(error.to_string(), SETUP))?;
        // Before a turn or a record: the alternative is a run that proceeds unconfined.
        let proof = sandbox::prove(&boundary, session.temp())
            .map_err(|card| Refusal::new(card.to_string(), card.exit_code()))?;
        let limits = Limits::from_config(config, &boundary);
        let catalog = Catalog::discover(&workspace, home().as_deref());
        let tools = ToolSet::new(&boundary, &catalog, front_end);
        // The store's own literals join the run's secrets, so nothing it holds can reach a
        // record, the JSON or a line of chatter.
        let store = match wire {
            Wire::Copilot => Some(
                Store::resolve(&Process)
                    .map_err(|error| Refusal::new(error.to_string(), error.exit_code()))?,
            ),
            Wire::Anthropic | Wire::Openai => None,
        };
        let mut secrets = config.secrets();
        if let Some(store) = &store {
            secrets = secrets.union(store.secrets());
        }
        let mut client = Client::http(headers(config), session.guid(), secrets.clone());
        if let Some(store) = store {
            client = client.store(store);
        }
        let ask = ask(config, wire, model.clone(), &workspace);
        let route = ask.route_of();
        let thinking = ask.thinks();
        let asking = Asking::new(client, ask);
        let shell = Shell::new(boundary.clone(), session.temp(), catalog);

        let record = Record::open(
            &session,
            Facts {
                wire,
                model: &model,
                route,
                thinking,
                boundary: &boundary,
                limits: &limits,
                workspace: &workspace,
                secrets: &secrets,
            },
        )
        .map_err(|error| Refusal::new(error.to_string(), SETUP))?;

        Ok(Launch {
            session,
            boundary,
            limits,
            record,
            asking,
            shell,
            tools,
            model,
            wire,
            route,
            context_window: config.context_window,
            proof,
            started,
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

    pub fn max_turns(&self) -> u32 {
        self.limits.max_turns()
    }

    fn facts(&self, measured: &Usage) -> rail::Facts {
        rail::Facts::of(&self.session, self.record.document())
            .measured(measured, self.context_window)
    }

    pub fn drive(
        mut self,
        prompt: &str,
        deltas: &mut dyn Deltas,
        turns: &mut dyn Turns,
        cancel: Cancel,
    ) -> Result<Outcome, Refusal> {
        let mut run = Run::new(prompt, self.asking.clone(), self.shell.clone())
            .limits(self.limits)
            .tools(&self.tools)
            .cancel(cancel);
        let mut measured = Usage::default();
        hand(turns, &self, &measured);
        while run.state().is_running() {
            run.turn(deltas);
            // A turn is written whole as it ends: the loop hands a front end no hook per entry.
            if let Some(exchange) = run.history().last() {
                note(self.record.turn_started(exchange.turn));
                note(self.record.answered(exchange.turn, &exchange.text));
                // A turn whose report carries no size leaves the one the turn before stated.
                if exchange.usage.input.is_some() {
                    measured = exchange.usage;
                }
                turns.turn(exchange);
                hand(turns, &self, &measured);
            }
        }

        let state = run.state();
        let usage = self.asking.spent();
        note(self.record.spent(usage));
        note(self.record.stopped(&state));
        hand(turns, &self, &measured);
        let outcome = Outcome::of(
            &run,
            Reported {
                session: self.session.guid(),
                model: &self.model,
                enforcement: self.boundary.enforcement(),
                duration: self.started.elapsed(),
                usage: self.record.document().usage,
            },
        );
        // The temp is emptied and the rest kept, so an ended session still reads.
        note(self.record.finish());
        outcome.ok_or_else(|| Refusal::new("the run ended with no stop reason to report", SETUP))
    }
}

fn hand(turns: &mut dyn Turns, launch: &Launch, measured: &Usage) {
    turns.facts(launch.facts(measured));
    turns.record(launch.record.document());
}

pub fn served(config: &Config) -> Option<Listing> {
    let wire = config.provider?;
    let at = match &config.base_url {
        Some(base_url) => Where::at(wire, base_url.clone()),
        None => Where::own(wire),
    };
    let credential = match &config.api_key {
        Some(key) => Credential::new(key.clone()),
        None => Credential::none(),
    };
    // Not a run: with no session to expand `{session}` to, the empty one is what is left.
    let mut client = Client::http(headers(config), String::new(), config.secrets());
    if wire == Wire::Copilot {
        // Signed in to rather than handed a key: a store that will not open is left out, and
        // the listing's hint says to sign in.
        if let Ok(store) = Store::resolve(&Process) {
            client = client.store(store);
        }
    }
    Some(client.served(&at, &credential, &at))
}

fn ask(config: &Config, wire: Wire, model: String, workspace: &std::path::Path) -> Ask {
    let mut ask = Ask::new(wire, model).preamble(Preamble::for_workspace(workspace).render());
    if let Some(base_url) = &config.base_url {
        ask = ask.base_url(base_url);
    }
    if let Some(route) = config.route {
        ask = ask.route(route);
    }
    ask = match &config.api_key {
        Some(key) => ask.credential(Credential::new(key)),
        None => ask.credential(Credential::none()),
    };
    ask.thinking(config.thinking.unwrap_or(false))
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

pub(crate) fn home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// A record that would not write is a diagnostic: the run it belongs to goes on regardless.
fn note(written: Result<(), impl std::fmt::Display>) {
    if let Err(error) = written {
        eprintln!("sindri: {error}");
    }
}
