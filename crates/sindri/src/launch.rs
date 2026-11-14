use std::path::PathBuf;
use std::time::Instant;

use sindri::ledger;
use sindri_core::asking::Asking;
use sindri_core::config::{Config, Process};
use sindri_core::open::Ground;
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

/// What a front end says about a launch that cannot run: one a frame can draw leaves the launch
/// standing, and one no frame survives is the reader's shell line and the end of the launch. No run
/// started, so a refusal is no stop reason and nothing is recorded.
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

    /// Handed once, when the run stops: the prompt it was asked with beside the answer it got, which
    /// is what a conversation keeps for the prompt after it.
    fn kept(&mut self, _kept: &Exchange) {}

    /// Handed when the run opens, as each turn ends, and once more when it stops.
    fn facts(&mut self, _facts: ledger::Facts) {}

    /// The record as it stands, handed beside the facts so a screen never reads a file mid-run.
    fn record(&mut self, _document: &SessionDocument) {}
}

impl Turns for () {
    fn turn(&mut self, _ended: &Exchange) {}
}

/// The run a prompt will ask for, as a launch knows it before a session exists: the wire and the
/// model the config names, the workspace it stands in and the branch that workspace is on, and the
/// enforcement every call of the run will be held to.
pub fn boot(cli: &Cli, config: &Config, workspace: &std::path::Path) -> ledger::Boot {
    ledger::Boot {
        wire: config.provider,
        model: config.model.clone(),
        workspace: workspace.to_path_buf(),
        branch: sindri_core::records::branch(workspace),
        enforcement: cli.enforcement(),
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
    config: Config,
    context_window: Option<u64>,
    measured: Usage,
    proof: Proof,
}

impl Launch {
    pub fn open(cli: &Cli, config: &Config, front_end: FrontEnd) -> Result<Launch, Refusal> {
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
        Ground::new(
            &self.boundary,
            self.session.temp(),
            &self.record.document().workspace,
        )
        .ok()
    }

    pub fn max_turns(&self) -> u32 {
        self.limits.max_turns()
    }

    fn facts(&self) -> ledger::Facts {
        ledger::Facts::of(&self.session, self.record.document())
            .measured(&self.measured, self.context_window)
    }

    /// Drives `prompt` on the turns this launch has already ended. The session it writes is the one
    /// the prompt before it wrote: the record counts the turns of both, and the temp a command wrote
    /// in is still there.
    pub fn drive(
        &mut self,
        prompt: &str,
        earlier: &[Exchange],
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
        // The turns the session counted before this prompt, which this run's own are numbered after.
        let counted = self.record.document().turns;
        hand(turns, self);
        while run.state().is_running() {
            run.turn(deltas);
            // A turn is written whole as it ends: the loop hands a front end no hook per entry.
            if let Some(exchange) = run.history().last() {
                let turn = counted + exchange.turn;
                note(self.record.turn_started(turn));
                note(self.record.answered(turn, &exchange.text));
                // A turn whose report carries no size leaves the one the turn before stated.
                if exchange.usage.input.is_some() {
                    self.measured = exchange.usage;
                }
                turns.turn(exchange);
                hand(turns, self);
            }
        }

        turns.kept(&run.kept());
        let state = run.state();
        let usage = self.asking.spent();
        note(self.record.spent(usage));
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

fn hand(turns: &mut dyn Turns, launch: &Launch) {
    turns.facts(launch.facts());
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

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    fn git(workspace: &std::path::Path, args: &[&str]) {
        let ran = std::process::Command::new("git")
            .arg("-C")
            .arg(workspace)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "sindri")
            .env("GIT_AUTHOR_EMAIL", "sindri@example.invalid")
            .env("GIT_COMMITTER_NAME", "sindri")
            .env("GIT_COMMITTER_EMAIL", "sindri@example.invalid")
            .status()
            .expect("git should run");
        assert!(ran.success(), "git {args:?} failed");
    }

    #[test]
    fn the_boot_facts_of_a_config_are_the_run_a_prompt_will_ask_for() {
        let root = tempfile::tempdir().expect("a temp workspace");
        git(root.path(), &["init", "--quiet", "-b", "the-branch"]);
        std::fs::write(root.path().join("a.txt"), "one file\n").expect("a file");
        git(root.path(), &["add", "a.txt"]);
        git(root.path(), &["commit", "--quiet", "-m", "one commit"]);
        let config = Config {
            provider: Some(Wire::Anthropic),
            model: Some(String::from("claude-sonnet-4-5")),
            ..Config::default()
        };

        let boot = boot(&Cli::parse_from(["sindri"]), &config, root.path());

        assert_eq!(
            boot.wire,
            Some(Wire::Anthropic),
            "the dialect the config names"
        );
        assert_eq!(boot.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(boot.workspace, root.path());
        assert_eq!(
            boot.branch.as_deref(),
            Some("the-branch"),
            "read off the repository, which a boot frame asks before any session exists"
        );
        assert_eq!(boot.enforcement, Enforcement::Bubblewrap);
    }

    #[test]
    fn a_config_that_names_no_run_and_a_workspace_that_is_no_checkout_give_no_fact_at_all() {
        let root = tempfile::tempdir().expect("a temp workspace");
        let config = Config {
            provider: None,
            model: None,
            ..Config::default()
        };

        let boot = boot(&Cli::parse_from(["sindri"]), &config, root.path());

        assert_eq!(
            (boot.wire, boot.model, boot.branch),
            (None, None, None),
            "the door names nothing it cannot know, and a directory that is no checkout has no \
             branch to name"
        );
    }
}
