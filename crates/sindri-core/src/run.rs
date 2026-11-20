//! The run is the agent loop and the stop reason that ended it. The model is a
//! [`Responder`] and the tools an [`Executor`], so the whole lifecycle is driven by
//! fakes with no network and no clock, and nothing here names a client library.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::bash::{self, Outcome};
use crate::config::Config;
use crate::open::{Ground, Spawner};
use crate::policy::{Boundary, DEFAULT_COMMAND_BUDGET_MS, DEFAULT_OUTPUT_BUDGET_BYTES};
use crate::provider::{Deltas, Failure};
use crate::redact::{Redacted, SecretSet};
use crate::skills::Catalog;
use crate::tools::{BASH, OPEN, SKILL, ToolSet};
use crate::turn::Usage;

pub use crate::turn::{Acted, Call, Exit, Turn};

pub const DEFAULT_MAX_TURNS: u32 = 250;

const SIGINT: i32 = 2;

/// The flag `^C` sets, written by the handler and read by every run that hears
/// it. Only an atomic store may happen there: anything that allocates or takes a
/// lock is undefined behaviour inside a signal handler.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// The handler `^C` runs: one store, so a signal arrives as a flag and never as an
/// interrupted request. Nothing here cuts a request in flight short, so the run stops when
/// that request ends.
extern "C" fn interrupted(_signal: i32) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

unsafe extern "C" {
    /// `signal(2)`, declared here because a cancellation is one flag and the core
    /// names no crate to hear it.
    fn signal(signum: i32, handler: usize) -> usize;
}

#[derive(Clone, Debug, Default)]
pub struct Cancel {
    flag: Arc<AtomicBool>,
    interrupt: bool,
}

impl Cancel {
    pub fn new() -> Cancel {
        Cancel::default()
    }

    pub fn on_sigint() -> Cancel {
        INTERRUPTED.store(false, Ordering::SeqCst);
        // Installed process-wide and never restored: a process has one `^C`
        // policy. A function pointer is cast to the integer `signal(2)` takes,
        // which is the cast that is defined.
        let handler: extern "C" fn(i32) = interrupted;
        unsafe { signal(SIGINT, handler as usize) };
        Cancel {
            flag: Arc::new(AtomicBool::new(false)),
            interrupt: true,
        }
    }

    pub fn now(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn is_set(&self) -> bool {
        self.flag.load(Ordering::SeqCst) || (self.interrupt && INTERRUPTED.load(Ordering::SeqCst))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Budget {
    Clock,
    Output,
}

impl Budget {
    pub fn word(self) -> &'static str {
        match self {
            Budget::Clock => "clock",
            Budget::Output => "output",
        }
    }
}

impl fmt::Display for Budget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.word())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stop {
    Final,
    MaxTurns,
    Budget(Budget),
    Cancelled,
    Error(Redacted),
}

impl Stop {
    pub fn word(&self) -> &'static str {
        match self {
            Stop::Final => "final",
            Stop::MaxTurns => "max_turns",
            Stop::Budget(_) => "budget",
            Stop::Cancelled => "cancelled",
            Stop::Error(_) => "error",
        }
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            Stop::Final => 0,
            Stop::MaxTurns => 100,
            Stop::Budget(_) => 101,
            Stop::Cancelled => 130,
            Stop::Error(_) => 1,
        }
    }

    pub fn detail(&self) -> Option<String> {
        match self {
            Stop::Budget(budget) => Some(String::from(budget.word())),
            Stop::Error(message) => Some(message.to_string()),
            Stop::Final | Stop::MaxTurns | Stop::Cancelled => None,
        }
    }
}

impl fmt::Display for Stop {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.word())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Running,
    Stopped(Stop),
}

impl State {
    pub fn word(&self) -> &'static str {
        match self {
            State::Running => "running",
            State::Stopped(stop) => stop.word(),
        }
    }

    pub fn stop(&self) -> Option<&Stop> {
        match self {
            State::Running => None,
            State::Stopped(stop) => Some(stop),
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self, State::Running)
    }
}

impl Serialize for State {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.word())
    }
}

/// The cap and the two budgets are separate facts, and the budgets are run-scoped:
/// batching hides work inside one script, so what it spends accumulates over the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    max_turns: u32,
    clock: Duration,
    output: u64,
}

impl Limits {
    pub fn new(max_turns: u32, clock: Duration, output: u64) -> Limits {
        Limits {
            max_turns,
            clock,
            output,
        }
    }

    pub fn max_turns(&self) -> u32 {
        self.max_turns
    }

    pub fn clock(&self) -> Duration {
        self.clock
    }

    pub fn output(&self) -> u64 {
        self.output
    }

    pub fn from_config(config: &Config, boundary: &Boundary) -> Limits {
        Limits {
            max_turns: config.max_turns.unwrap_or(DEFAULT_MAX_TURNS),
            clock: Duration::from_millis(boundary.command_budget_ms()),
            output: boundary.output_budget_bytes(),
        }
    }
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_turns: DEFAULT_MAX_TURNS,
            clock: Duration::from_millis(DEFAULT_COMMAND_BUDGET_MS),
            output: DEFAULT_OUTPUT_BUDGET_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spent {
    clock: Duration,
    output: u64,
}

impl Spent {
    pub fn clock(&self) -> Duration {
        self.clock
    }

    pub fn output(&self) -> u64 {
        self.output
    }

    pub fn exhausted(&self, limits: &Limits) -> Option<Budget> {
        if self.clock >= limits.clock {
            Some(Budget::Clock)
        } else if self.output >= limits.output {
            Some(Budget::Output)
        } else {
            None
        }
    }

    fn charge(&mut self, elapsed: Duration, bytes: u64) {
        self.clock += elapsed;
        self.output = self.output.saturating_add(bytes);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reply {
    pub text: String,
    pub reasoning: Option<String>,
    pub calls: Vec<Call>,
    pub usage: Usage,
}

impl Reply {
    pub fn answer(text: impl Into<String>) -> Reply {
        Reply::asking(text, [])
    }

    pub fn asking(text: impl Into<String>, calls: impl IntoIterator<Item = Call>) -> Reply {
        Reply {
            text: text.into(),
            reasoning: None,
            calls: calls.into_iter().collect(),
            usage: Usage::default(),
        }
    }

    pub fn spent(mut self, usage: Usage) -> Self {
        self.usage = usage;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    turn: u32,
    prompt: String,
    earlier: Vec<Turn>,
    history: Vec<Turn>,
    skills: Vec<String>,
    tools: Vec<String>,
}

impl Request {
    pub fn turn(&self) -> u32 {
        self.turn
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn earlier(&self) -> &[Turn] {
        &self.earlier
    }

    pub fn history(&self) -> &[Turn] {
        &self.history
    }

    pub fn skills(&self) -> &[String] {
        &self.skills
    }

    pub fn tools(&self) -> &[String] {
        &self.tools
    }
}

pub trait Responder {
    fn respond(&self, request: &Request, deltas: &mut dyn Deltas) -> Result<Reply, Failure>;
}

pub trait Executor {
    fn execute(&self, call: &Call, loaded: &mut Loaded) -> Result<Acted, Redacted>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Load {
    Loaded,
    AlreadyLoaded,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Loaded {
    names: Vec<String>,
}

impl Loaded {
    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names.iter().any(|loaded| loaded == name)
    }

    /// A name already in the set is `already_loaded` and carries no body: a tool result is
    /// re-sent with every later turn, so a second body would be paid for on every turn.
    pub fn load(&mut self, name: impl Into<String>) -> Load {
        let name = name.into();
        if self.contains(&name) {
            return Load::AlreadyLoaded;
        }
        self.names.push(name);
        Load::Loaded
    }

    pub fn clear(&mut self) {
        self.names.clear();
    }
}

pub struct Run<R, A> {
    prompt: String,
    responder: R,
    executor: A,
    limits: Limits,
    tools: Vec<String>,
    cancel: Cancel,
    spent: Spent,
    turns: u32,
    skills: Loaded,
    earlier: Vec<Turn>,
    history: Vec<Turn>,
    answer: Option<String>,
    state: State,
}

impl<R: Responder, A: Executor> Run<R, A> {
    pub fn new(prompt: impl Into<String>, responder: R, executor: A) -> Run<R, A> {
        Run {
            prompt: prompt.into(),
            responder,
            executor,
            limits: Limits::default(),
            tools: Vec::new(),
            cancel: Cancel::new(),
            spent: Spent::default(),
            turns: 0,
            skills: Loaded::default(),
            earlier: Vec::new(),
            history: Vec::new(),
            answer: None,
            state: State::Running,
        }
    }

    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    pub fn tools(mut self, tools: &ToolSet) -> Self {
        self.tools = tools
            .tools()
            .iter()
            .map(|tool| String::from(tool.name()))
            .collect();
        self
    }

    /// The turns that ended before this run, each beside the answer it got: a wire is asked with
    /// the turn the new prompt joins, not with its own words alone.
    pub fn earlier(mut self, earlier: impl IntoIterator<Item = Turn>) -> Self {
        self.earlier = earlier.into_iter().collect();
        self
    }

    pub fn cancel(mut self, cancel: Cancel) -> Self {
        self.cancel = cancel;
        self
    }

    pub fn canceller(&self) -> Cancel {
        self.cancel.clone()
    }

    pub fn state(&self) -> State {
        self.state.clone()
    }

    pub fn turns(&self) -> u32 {
        self.turns
    }

    pub fn spent(&self) -> Spent {
        self.spent
    }

    pub fn history(&self) -> &[Turn] {
        &self.history
    }

    /// The turn a conversation keeps for this run: the last turn it took, results and spend
    /// included, and the prompt alone when it took none.
    pub fn kept(&self) -> Turn {
        self.history
            .last()
            .cloned()
            .unwrap_or_else(|| Turn::answered(self.turns, self.prompt.clone(), ""))
    }

    pub fn skills(&self) -> &[String] {
        self.skills.names()
    }

    pub fn answer(&self) -> Option<&str> {
        match &self.state {
            State::Stopped(Stop::Final) => self.answer.as_deref(),
            _ => None,
        }
    }

    pub fn load_skill(&mut self, name: impl Into<String>) -> Load {
        self.skills.load(name)
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.earlier.clear();
        self.skills.clear();
    }

    pub fn turn(&mut self, deltas: &mut dyn Deltas) -> State {
        if !self.state.is_running() {
            return self.state();
        }
        if self.cancel.is_set() {
            self.settle(Stop::Cancelled);
            return self.state();
        }
        if let Some(budget) = self.spent.exhausted(&self.limits) {
            self.settle(Stop::Budget(budget));
            return self.state();
        }
        if self.turns >= self.limits.max_turns {
            self.settle(Stop::MaxTurns);
            return self.state();
        }

        self.turns += 1;
        let request = self.request();
        let reply = match self.responder.respond(&request, deltas) {
            Ok(reply) => reply,
            Err(failure) => {
                self.settle(Stop::Error(failure.message));
                return self.state();
            }
        };
        let mut results = Vec::with_capacity(reply.calls.len());
        for call in &reply.calls {
            let acted = match self.executor.execute(call, &mut self.skills) {
                Ok(acted) => acted,
                Err(message) => {
                    self.settle(Stop::Error(message));
                    break;
                }
            };
            self.spent.charge(acted.elapsed, acted.bytes);
            results.push(acted);
            if self.cancel.is_set() {
                self.settle(Stop::Cancelled);
                break;
            }
            if let Some(budget) = self.spent.exhausted(&self.limits) {
                self.settle(Stop::Budget(budget));
                break;
            }
        }

        // The turn is kept once its calls have run, so the turn a later ask replays carries what
        // each call answered rather than a call the model would read as never having run.
        self.history.push(Turn {
            turn: self.turns,
            prompt: self.prompt.clone(),
            text: reply.text.clone(),
            calls: reply.calls.clone(),
            results,
            reasoning: reply.reasoning.clone(),
            usage: reply.usage,
        });

        if self.state.is_running() && reply.calls.is_empty() {
            self.answer = Some(reply.text);
            self.settle(Stop::Final);
        }
        self.state()
    }

    pub fn drive(&mut self, deltas: &mut dyn Deltas) -> State {
        while self.state.is_running() {
            self.turn(deltas);
        }
        self.state()
    }

    fn request(&self) -> Request {
        Request {
            turn: self.turns,
            prompt: self.prompt.clone(),
            earlier: self.earlier.clone(),
            history: self.history.clone(),
            skills: self.skills.names().to_vec(),
            tools: self.tools.clone(),
        }
    }

    fn settle(&mut self, stop: Stop) {
        if self.cancel.is_set() {
            self.state = State::Stopped(Stop::Cancelled);
            return;
        }
        if self.state.is_running() {
            self.state = State::Stopped(stop);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Shell {
    boundary: Boundary,
    temp: PathBuf,
    catalog: Catalog,
    cancel: Cancel,
    ground: Option<Ground>,
    opener: Option<Arc<dyn Spawner>>,
}

impl Shell {
    pub fn new(boundary: Boundary, temp: impl Into<PathBuf>, catalog: Catalog) -> Shell {
        Shell {
            boundary,
            temp: temp.into(),
            catalog,
            cancel: Cancel::new(),
            ground: None,
            opener: None,
        }
    }

    pub fn cancel(mut self, cancel: Cancel) -> Shell {
        self.cancel = cancel;
        self
    }

    pub fn ground(mut self, ground: Option<Ground>) -> Shell {
        self.ground = ground;
        self
    }

    pub fn opener(mut self, opener: Option<Arc<dyn Spawner>>) -> Shell {
        self.opener = opener;
        self
    }

    pub fn grounded(&self) -> Option<&Ground> {
        self.ground.as_ref()
    }

    fn bash(&self, arguments: &str) -> Result<Acted, Redacted> {
        let command = command_of(arguments).ok_or_else(|| {
            SecretSet::default().redact(format!(
                "a `{BASH}` call names its command as {{\"command\": \"...\"}}, and this one was {arguments}"
            ))
        })?;

        let started = Instant::now();
        let outcome = bash::run(&self.boundary, &self.temp, &command, &self.cancel)
            .map_err(|error| SecretSet::default().redact(error.to_string()))?;
        Ok(Acted {
            text: rendered(&outcome),
            bytes: u64::try_from(outcome.stdout.len() + outcome.stderr.len()).unwrap_or(u64::MAX),
            elapsed: started.elapsed(),
            exit: exit_of(&outcome),
        })
    }

    fn open(&self, arguments: &str) -> Result<Acted, Redacted> {
        let opener = self.opener.as_ref().ok_or_else(|| {
            SecretSet::default().redact(format!(
                "this run cannot run an `{OPEN}` call: it was handed no opener to give a target to"
            ))
        })?;
        let ground = self.ground.as_ref().ok_or_else(|| {
            SecretSet::default().redact(format!(
                "this run cannot run an `{OPEN}` call: it stands on no ground to judge a target by"
            ))
        })?;
        let opening = ground
            .judge(arguments)
            .map_err(|refusal| SecretSet::default().redact(refusal.to_string()))?;

        let started = Instant::now();
        opener
            .spawn(&opening)
            .map_err(|reason| SecretSet::default().redact(reason))?;
        Ok(Acted {
            text: format!("opened {opening}"),
            bytes: 0,
            elapsed: started.elapsed(),
            exit: Exit::Code(0),
        })
    }

    fn skill(&self, arguments: &str, loaded: &mut Loaded) -> Result<Acted, Redacted> {
        let name = name_of(arguments).ok_or_else(|| {
            SecretSet::default().redact(format!(
                "a `{SKILL}` call names its skill as {{\"name\": \"...\"}}, and this one was {arguments}"
            ))
        })?;

        let skill = self
            .catalog
            .get(&name)
            .filter(|skill| skill.model_invocable())
            .ok_or_else(|| {
                SecretSet::default().redact(format!(
                    "no skill is named `{name}` in the catalog in force; it holds {}",
                    held(&self.catalog)
                ))
            })?;

        let text = match loaded.load(&name) {
            Load::Loaded => skill.wrapper(),
            Load::AlreadyLoaded => format!(
                "already_loaded: `{name}` was loaded earlier in this conversation, so its \
                 instructions are already in hand and are not repeated here."
            ),
        };
        Ok(Acted {
            text,
            bytes: 0,
            elapsed: Duration::ZERO,
            exit: Exit::Code(0),
        })
    }
}

impl Executor for Shell {
    fn execute(&self, call: &Call, loaded: &mut Loaded) -> Result<Acted, Redacted> {
        match call.name.as_str() {
            BASH => self.bash(&call.arguments),
            SKILL => self.skill(&call.arguments, loaded),
            OPEN => self.open(&call.arguments),
            other => {
                let mut named = vec![BASH, SKILL];
                if self.opener.is_some() {
                    named.push(OPEN);
                }
                Err(SecretSet::default().redact(format!(
                    "this run cannot run `{other}`: its tools are {}",
                    listed(&named)
                )))
            }
        }
    }
}

fn listed(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.split_last() {
        Some((last, held)) => match held.is_empty() {
            true => last.clone(),
            false => format!("{} and {last}", held.join(", ")),
        },
        None => String::new(),
    }
}

/// The names the model may call, as a refusal writes them. A skill the frontmatter
/// withholds is left out: a refusal that named it would be the way around the flag.
fn held(catalog: &Catalog) -> String {
    let names: Vec<String> = catalog
        .skills()
        .iter()
        .filter(|skill| skill.model_invocable())
        .map(|skill| format!("`{}`", skill.name))
        .collect();
    if names.is_empty() {
        return String::from("no skill at all");
    }
    names.join(", ")
}

fn name_of(arguments: &str) -> Option<String> {
    let arguments: Value = serde_json::from_str(arguments).ok()?;
    let name = arguments.get("name")?.as_str()?;
    Some(name.to_owned())
}

fn command_of(arguments: &str) -> Option<String> {
    let arguments: Value = serde_json::from_str(arguments).ok()?;
    let command = arguments.get("command")?.as_str()?;
    Some(command.to_owned())
}

fn rendered(outcome: &Outcome) -> String {
    let mut text = outcome.stdout.clone();
    text.push_str(&outcome.stderr);
    text
}

fn exit_of(outcome: &Outcome) -> Exit {
    if outcome.cancelled {
        return Exit::Cancelled;
    }
    if outcome.timed_out {
        return Exit::TimedOut;
    }
    match outcome.code {
        Some(code) => Exit::Code(code),
        None => Exit::Signalled,
    }
}
