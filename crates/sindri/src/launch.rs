//! One launch's run, assembled the same way whichever front end asked for it: the session
//! it is recorded under (§13), the boundary its commands are confined by (§6), the wire it
//! asks and the loop that drives it to a stop reason — so batch mode and the terminal UI
//! cannot differ in what a run is (§3, ADR-0005).
//!
//! The assembly is the part that must not drift: one secret set, a first request that
//! proves the sandbox before a turn or a record, a catalog the tool descriptions are
//! derived from, a record opened before the first turn so a killed run still reads. A front
//! end that assembled its own would be a second opinion about all of it, so neither does.
//!
//! What is left to a front end is its voice and its picture: its own words about the run on
//! its own stream, and the answer drawn however it draws one. The loop is here because it is
//! the same loop — a turn asked, the record written as it goes, and a stop reason nobody
//! gets to disagree with (§15, ADR-0006) — and the fragments of the turn in flight and the
//! turns that end are handed to whoever the front end gave it.
//!
//! The cancellation arrives by value, so it exists before the run does: the flag the user's
//! word lands on is the one the loop reads and the one a command in flight hears (§4).

use std::path::PathBuf;
use std::time::Instant;

use sindri_core::asking::Asking;
use sindri_core::config::{Config, Process};
use sindri_core::outcome::{Facts as Reported, Outcome};
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::preamble::Preamble;
use sindri_core::provider::copilot::Store;
use sindri_core::provider::served::{Listing, Where};
use sindri_core::provider::{Ask, Client, Credential, Deltas, Route};
use sindri_core::records::{Facts, Record};
use sindri_core::run::{Cancel, Exchange, Limits, Run, Shell};
use sindri_core::sandbox::{self, Proof};
use sindri_core::session::{self, Session};
use sindri_core::skills::Catalog;
use sindri_core::tools::{FrontEnd, ToolSet};
use sindri_core::wire::Wire;

use crate::cli::Cli;

/// §15's setup failure: nothing to run *with* — a session that cannot be made, a boundary
/// that cannot be held, a sandbox that cannot be proved.
const SETUP: u8 = 3;

/// §15's usage error: no provider and no model to run against, which is a command line or
/// a config that holds no run.
const USAGE: u8 = 2;

/// Why a launch cannot run at all: the one line a front end says about it, and §15's code.
///
/// A refusal is not a stop reason: no run started, so no record was written and nothing is
/// on stdout — it is the launch saying it has nothing to run with (§15).
pub struct Refusal {
    /// What is missing or wrong, in one line safe to print.
    message: String,
    /// The exit code §15 gives it.
    code: u8,
}

impl Refusal {
    /// A refusal that says `message` and exits with `code`.
    pub fn new(message: impl Into<String>, code: u8) -> Refusal {
        Refusal {
            message: message.into(),
            code,
        }
    }

    /// What is missing or wrong, in one line.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The exit code §15 gives it.
    pub fn code(&self) -> u8 {
        self.code
    }
}

/// What a front end is told as a run goes, besides the fragments landing in its
/// [`Deltas`]: the turn that just ended, so the front end that speaks says so and the one
/// that draws takes what it needs of it.
pub trait Turns {
    /// One turn ended: what the model said, and the calls it asked for.
    fn turn(&mut self, ended: &Exchange);
}

/// A front end with nothing to say about a turn that ended.
impl Turns for () {
    fn turn(&mut self, _ended: &Exchange) {}
}

/// What a launch knows about itself before its first turn: the facts `-v` narrates and the
/// rail's ledger will show (#46), read off the values the run was handed rather than off
/// the config they came from.
pub struct Figure {
    /// The dialect the run asks.
    pub wire: Wire,
    /// The served name it asks for.
    pub model: String,
    /// The completion endpoint, for a wire that serves two (A10).
    pub route: Option<Route>,
    /// How its commands are confined: `bubblewrap` or `unsafe`.
    pub enforcement: Enforcement,
    /// The one directory it stands in.
    pub workspace: PathBuf,
    /// The directory its session is recorded in.
    pub session: PathBuf,
    /// What boot proved about the sandbox, as one line.
    pub proof: String,
}

/// Everything one run needs, in the order §13 says a session is opened and §6 says the
/// sandbox is proved: held together so the loop, the record, and the report its stop reason
/// is written from are one launch and not three readings of a config.
pub struct Launch {
    /// The session the run is recorded under, and the temp its commands see.
    session: Session,
    /// The boundary in force, which the tools were derived from and the record names.
    boundary: Boundary,
    /// The dials the run obeys.
    limits: Limits,
    /// The session the run writes itself into as it goes (§13).
    record: Record,
    /// The wire the run asks, folding each reply into the turn's answer.
    asking: Asking,
    /// The tools the run acts with.
    shell: Shell,
    /// The names the model is told it may call, from the one set the tools came from.
    tools: ToolSet,
    /// The served name the run asks for, which the report names too.
    model: String,
    /// The dialect it asks.
    wire: Wire,
    /// The completion endpoint, for a wire that serves two (A10).
    route: Option<Route>,
    /// What boot proved about the sandbox, as the one line a front end narrates.
    proof: Proof,
    /// When the run was opened, which is where its duration is measured from.
    started: Instant,
}

impl Launch {
    /// Everything one run needs, or the one line saying why this launch cannot run one.
    ///
    /// `front_end` is the set of tools to register: a terminal has somebody to hand a
    /// target to and batch mode has nobody, so `open` is in one set and absent from the
    /// other (§2's "absence over refusal", §3).
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
        // Boot proves the sandbox once, with a real bubblewrap invocation, before a turn
        // or a record: a sandbox that cannot be created stops the run here as a card,
        // because the alternative is running unconfined without being asked to (§6).
        let proof = sandbox::prove(&boundary, session.temp())
            .map_err(|card| Refusal::new(card.to_string(), card.exit_code()))?;
        let limits = Limits::from_config(config, &boundary);
        let catalog = Catalog::discover(&workspace, home().as_deref());
        let tools = ToolSet::new(&boundary, &catalog, front_end);
        // A wire that signs in is asked with the session its store holds, and the store's
        // own literals join the run's secrets, so nothing it holds can reach a record, the
        // JSON or a line of chatter (ADR-0011). The store is resolved here rather than by
        // the seam, because a run has no one to show a code to and a store that cannot be
        // opened is a setup failure and not a reply.
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
        // Which endpoint the run answers on is read before the first request leaves, so a
        // run that dies on its first ask still says where it was (A10).
        let route = ask.route_of();
        let asking = Asking::new(client, ask);
        let shell = Shell::new(boundary.clone(), session.temp(), catalog);

        let record = Record::open(
            &session,
            Facts {
                wire,
                model: &model,
                route,
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
            proof,
            started,
        })
    }

    /// What this launch knows before its first turn, as one value: the facts `-v` narrates
    /// and the rail's ledger shows.
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

    /// The turn cap the run obeys, which a front end says when a run stops on it.
    pub fn max_turns(&self) -> u32 {
        self.limits.max_turns()
    }

    /// Drives `prompt` to a stop reason: every fragment of the turn in flight lands in
    /// `deltas`, every turn that ends is handed to `turns`, and the session is written as it
    /// goes and ended whatever the run did (§13).
    ///
    /// The report's stop reason is read off the loop and never decided here, so no front end
    /// can hold a second opinion about how a run ended (§15, ADR-0006), and a cancellation
    /// beats every other trigger because the loop is what compares them (§4).
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
        while run.state().is_running() {
            run.turn(deltas);
            // The loop hands a front end no hook per entry, so a turn is written whole as
            // it ends: the turn it was, and what the assistant said. A call and its result
            // need the loop to say when they happen, which is #17's seam to grow; nothing
            // in this build's wire can ask for one (ADR-0016).
            if let Some(exchange) = run.history().last() {
                note(self.record.turn_started(exchange.turn));
                note(self.record.answered(exchange.turn, &exchange.text));
                turns.turn(exchange);
            }
        }

        let state = run.state();
        let usage = self.asking.spent();
        note(self.record.spent(usage));
        note(self.record.stopped(&state));
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
        // The temp is emptied and everything else kept, so a session that ended still reads
        // beside one that was killed (§13).
        note(self.record.finish());
        outcome.ok_or_else(|| Refusal::new("the run ended with no stop reason to report", SETUP))
    }
}

/// The names the wire in force serves, read off it and never invented: what the `/model`
/// screen offers a reader, kept beside the `(wire, base_url)` pair that produced them
/// (§10, §12).
///
/// It is assembled here for the reason the run's client is: which wire, which endpoint and
/// which credential are one reading of the config, and a terminal that built its own would
/// be a second opinion about them. A wire that cannot answer hands back a listing with no
/// names and one line saying why, which is §10's hint and never a refusal — `/model <name>`
/// still names a model and the run goes on.
///
/// `None` when this launch has no wire at all, which is a config `main` refuses before the
/// screen is taken: there is nothing to ask and nothing to show.
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
    // A listing is not a run, so there is no session to expand `{session}` to and no record
    // for the names to land in: the headers travel because a gateway may need them, and the
    // empty session is what a request outside a run has.
    let mut client = Client::http(headers(config), String::new(), config.secrets());
    if wire == Wire::Copilot {
        // A Copilot session is signed in to rather than handed a key. A store that will not
        // open is a wire that cannot be asked, and §10's hint is the line that says to sign
        // in rather than a screen that waits for ever.
        if let Ok(store) = Store::resolve(&Process) {
            client = client.store(store);
        }
    }
    Some(client.served(&at, &credential, &at))
}

/// The ask a run is built from: the dialect the config named, the served name, the endpoint
/// when one was set, the credential, and the preamble the workspace stands in front of,
/// re-sent every turn.
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

/// The run's extra request headers, as the config writes them: `Name: value`, split at the
/// first colon. A line with no colon is dropped rather than sent as a name with an empty
/// value.
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

/// The home the user-scope skills come from, when the environment names one. The frame
/// reads it through here too, so the catalog a `/skill:name` line asks and the catalog a
/// run's tool is derived from are one directory (§7).
pub(crate) fn home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// A record that would not write is a diagnostic and not a reason to stop a run that is
/// already doing what it was asked: the session that remains still reads (§13).
fn note(written: Result<(), impl std::fmt::Display>) {
    if let Err(error) = written {
        eprintln!("sindri: {error}");
    }
}
