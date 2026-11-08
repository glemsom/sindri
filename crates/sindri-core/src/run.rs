//! The run is the agent loop, its turns and the stop reason that ended it, from
//! the first request to the last: a turn is one request and the calls it asks
//! for, the turn cap and the two run-scoped budgets are what stop it, and the
//! loaded-skill set and the history are carried from turn to turn.
//!
//! The stop set is the product, not an error path. [`Stop`] is a closed set of
//! five — `final`, `max_turns`, `budget`, `cancelled`, `error` — and there is no
//! other variant to reach, so the record, the JSON, the rail and the exit code
//! all read one value and no front end gets a second opinion (§15). [`State`] is
//! that same fact before it lands: a run is `Running` until a stop reason does,
//! which is what lets a record say `running` while the run is still going (#18).
//!
//! Precedence is ADR-0006, and it is written here rather than left to a caller:
//! a cancellation always wins, because it is the user's own word, and otherwise
//! the first trigger in time wins. Two guards make that unambiguous — the loop
//! starts no turn once a budget is spent, so `budget` cannot hide behind
//! `max_turns`, and a turn that produced a complete answer stops as `final`, so
//! the cap never steals a success.
//!
//! The two budgets are run-scoped and not turn-scoped: batching hides work
//! inside one script, so the command wall clock and the bytes of command output
//! fed back accumulate over the whole run, and no turn starts once one is spent.
//! Deciding that a budget is spent is here; writing an overflow to `spill/` is
//! the session temp's (#16), so the two never own the same fact.
//!
//! Nothing here names a client library. The model is a [`Responder`] — one
//! request in, one reply or one failure out — and the tools are an [`Executor`],
//! so the whole of the lifecycle is driven by fakes, with no network and no
//! clock, and the wire stays behind the provider seam.

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
use crate::policy::{Boundary, DEFAULT_COMMAND_BUDGET_MS, DEFAULT_OUTPUT_BUDGET_BYTES};
use crate::provider::{Deltas, Failure, Usage};
use crate::redact::{Redacted, SecretSet};
use crate::skills::Catalog;
use crate::tools::{BASH, SKILL, ToolSet};

/// The turn cap a run gets when the config names none (§4).
pub const DEFAULT_MAX_TURNS: u32 = 250;

/// The signal a terminal's `^C` sends, which is the user's word (ADR-0006).
const SIGINT: i32 = 2;

/// The flag `^C` sets, written by the handler and read by every run that hears
/// it. Only an atomic store may happen there: anything that allocates or takes a
/// lock is undefined behaviour inside a signal handler.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// The handler `^C` runs: one store, so a signal arrives in the loop as a flag
/// and never as an interrupted request. The request in flight is not cut short —
/// a wire in rig-core 0.46 constructs no cancellation error — so the run stops
/// when the in-flight request does (§4, ADR-0015's "not yet").
extern "C" fn interrupted(_signal: i32) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

unsafe extern "C" {
    /// `signal(2)`, declared here rather than pulled in as a dependency: a
    /// cancellation is one flag and the core names no crate to hear it.
    fn signal(signum: i32, handler: usize) -> usize;
}

/// The cancellation a run observes: a flag set by whatever owns the user's word,
/// from a front end's `Ctrl-C` or from `^C` itself, and read by the loop before
/// every turn and after every call.
#[derive(Clone, Debug, Default)]
pub struct Cancel {
    /// Set by a front end that asked the run to stop.
    flag: Arc<AtomicBool>,
    /// Whether this handle also hears `^C`, which is a fact about the process
    /// and not about the flag.
    interrupt: bool,
}

impl Cancel {
    /// A cancel a front end sets itself, and that hears no signal.
    pub fn new() -> Cancel {
        Cancel::default()
    }

    /// A cancel that also hears `^C`: the handler is installed once, and the
    /// flag it writes is cleared, so a run starts listening rather than
    /// inheriting a signal nobody is left to observe.
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

    /// The user's word, from a front end: nothing is asked of the run again.
    pub fn now(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Whether a cancellation has arrived, from either door.
    pub fn is_set(&self) -> bool {
        self.flag.load(Ordering::SeqCst) || (self.interrupt && INTERRUPTED.load(Ordering::SeqCst))
    }
}

/// Which of the two run-scoped limits a run spent: a budget, and never the turn
/// cap (GLOSSARY). The pair differ in what they count and agree in being spent
/// once for the whole run rather than once per turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget {
    /// The run's command wall clock.
    Clock,
    /// The bytes of command output the run fed back.
    Output,
}

impl Budget {
    /// The word a record keeps beside `budget`, so a reader knows which one.
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

/// Why a run ended, from the closed set of §4.
///
/// The set is closed in the only way a set can be: [`Stop::word`] and
/// [`Stop::exit_code`] match it with no catch-all arm, so a sixth reason cannot
/// be added without every surface being made to say it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// A turn produced a complete answer: the only reason a run writes stdout.
    Final,
    /// The turn cap was reached with no answer in hand.
    MaxTurns,
    /// One of the two run-scoped budgets is spent.
    Budget(Budget),
    /// The user's word, which always wins (ADR-0006).
    Cancelled,
    /// A request or a call failed, in the one line safe to print about it.
    Error(Redacted),
}

impl Stop {
    /// The word the record, the JSON and the rail use, which is §4's own.
    pub fn word(&self) -> &'static str {
        match self {
            Stop::Final => "final",
            Stop::MaxTurns => "max_turns",
            Stop::Budget(_) => "budget",
            Stop::Cancelled => "cancelled",
            Stop::Error(_) => "error",
        }
    }

    /// The exit code §15 gives this stop reason: one mapping table, so a script
    /// never sees a second opinion about how a run ended.
    pub fn exit_code(&self) -> u8 {
        match self {
            Stop::Final => 0,
            Stop::MaxTurns => 100,
            Stop::Budget(_) => 101,
            Stop::Cancelled => 130,
            Stop::Error(_) => 1,
        }
    }

    /// What the stop reason was made of, when it was made of more than its word:
    /// which budget ran out, or the one line about what failed.
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

/// Where a run stands: running until a stop reason lands.
///
/// A caller asks for this rather than for a result, because a run is watched
/// while it goes: an entry written between two turns says `running`, and the same
/// value says the terminal word once one lands (§4, #18).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// No stop reason has landed; the loop may still start a turn.
    Running,
    /// A stop reason has landed, and cannot be replaced except by the user's
    /// own word.
    Stopped(Stop),
}

impl State {
    /// The word a record writes: `running` until a stop reason lands, and the
    /// stop reason's own word after that.
    pub fn word(&self) -> &'static str {
        match self {
            State::Running => "running",
            State::Stopped(stop) => stop.word(),
        }
    }

    /// The stop reason, when one has landed.
    pub fn stop(&self) -> Option<&Stop> {
        match self {
            State::Running => None,
            State::Stopped(stop) => Some(stop),
        }
    }

    /// Whether a turn may still start.
    pub fn is_running(&self) -> bool {
        matches!(self, State::Running)
    }
}

impl Serialize for State {
    /// The word a record writes: `running` until a stop reason lands, and the
    /// stop reason's own word after that.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.word())
    }
}

/// The dials a run obeys (§4, §13): the turn cap, the command wall clock it may
/// spend, and the bytes of command output it may feed back.
///
/// The cap and the two budgets are separate fields because they are separate
/// facts: a turn cap cannot see work a single command does, and a budget cannot
/// see a model that asks again and again in no time at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The ceiling on turns, whose exhaustion is a stop reason and not an error.
    max_turns: u32,
    /// The run's command wall clock.
    clock: Duration,
    /// The run's budget of command output, in bytes.
    output: u64,
}

impl Limits {
    /// The dials: a turn cap, a wall clock and a byte count.
    pub fn new(max_turns: u32, clock: Duration, output: u64) -> Limits {
        Limits {
            max_turns,
            clock,
            output,
        }
    }

    /// The turn cap in force.
    pub fn max_turns(&self) -> u32 {
        self.max_turns
    }

    /// The run's command wall clock.
    pub fn clock(&self) -> Duration {
        self.clock
    }

    /// The run's budget of command output, in bytes.
    pub fn output(&self) -> u64 {
        self.output
    }

    /// The dials in force for a run: the turn cap from the config, and the two
    /// budgets from the boundary, which has already resolved their defaults so
    /// that the model's description and the loop's accounting cannot disagree.
    pub fn from_config(config: &Config, boundary: &Boundary) -> Limits {
        Limits {
            max_turns: config.max_turns.unwrap_or(DEFAULT_MAX_TURNS),
            clock: Duration::from_millis(boundary.command_budget_ms()),
            output: boundary.output_budget_bytes(),
        }
    }
}

impl Default for Limits {
    /// §4's documented defaults: two hundred and fifty turns, one hour of
    /// command wall clock and eight mebibytes of command output.
    fn default() -> Limits {
        Limits {
            max_turns: DEFAULT_MAX_TURNS,
            clock: Duration::from_millis(DEFAULT_COMMAND_BUDGET_MS),
            output: DEFAULT_OUTPUT_BUDGET_BYTES,
        }
    }
}

/// What a run has spent of its budgets, accumulated over the whole run.
///
/// Nothing here is per turn: two turns that each run one script spend what one
/// turn running both would have spent, which is the whole reason a turn cap is
/// not enough.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spent {
    /// The command wall clock spent so far.
    clock: Duration,
    /// The command output bytes fed back so far.
    output: u64,
}

impl Spent {
    /// The command wall clock spent so far.
    pub fn clock(&self) -> Duration {
        self.clock
    }

    /// The command output bytes fed back so far.
    pub fn output(&self) -> u64 {
        self.output
    }

    /// Which budget `limits` leaves nothing of, if one is: the wall clock first,
    /// so a call that spends both says one thing rather than two.
    pub fn exhausted(&self, limits: &Limits) -> Option<Budget> {
        if self.clock >= limits.clock {
            Some(Budget::Clock)
        } else if self.output >= limits.output {
            Some(Budget::Output)
        } else {
            None
        }
    }

    /// Charge one command's wall clock and output bytes to the run.
    fn charge(&mut self, elapsed: Duration, bytes: u64) {
        self.clock += elapsed;
        self.output = self.output.saturating_add(bytes);
    }
}

/// One call the model asked for: the tool it names and the arguments the wire
/// wrote, left as they arrived because a tool reads its own arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    /// The tool's name, as the schema the model was given names it.
    pub name: String,
    /// The arguments, as the wire wrote them.
    pub arguments: String,
}

impl Call {
    /// A call of `name` with `arguments`.
    pub fn new(name: impl Into<String>, arguments: impl Into<String>) -> Self {
        Call {
            name: name.into(),
            arguments: arguments.into(),
        }
    }
}

/// How one call ended, from the closed set §12's rows are drawn from: a command
/// that exited with a code, one the per-call timeout killed, one the user's word
/// stopped before it ran, and the case `std::process` reports as no code at all, a
/// command a signal killed.
///
/// This is a fact and never a rendering (ADR-0007), which is the whole reason it
/// exists: the row that says `✗ exit 1` reads this value, so the picture cannot
/// disagree with what happened by reading a sentence out of the text beside it —
/// and the sentence the model is told is derived from the same value, so the two
/// cannot drift apart either.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exit {
    /// The command ran and exited with this code.
    Code(i32),
    /// A signal killed it, so the platform names no code to report.
    Signalled,
    /// The per-call timeout ran out and the command was killed (§4).
    TimedOut,
    /// The user's word stopped the call: before it ran, so it never did, or while
    /// it ran, so the command and everything it started were killed (§4).
    Cancelled,
}

impl Exit {
    /// The verdict a command's own [`Outcome`] states: the user's word first,
    /// because a cancellation beats every other trigger (ADR-0006), and then the
    /// timeout, because a command the timeout killed has no code that means
    /// anything.
    pub fn of(outcome: &Outcome) -> Exit {
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

    /// The words a row says it in and the line the model is told, in one table:
    /// `✓ exit 0` and the text fed back to the model are the same phrasing because
    /// they are the same fact.
    pub fn word(self) -> String {
        match self {
            Exit::Code(code) => format!("exit {code}"),
            Exit::Signalled => String::from("killed by a signal"),
            Exit::TimedOut => String::from("timed out"),
            Exit::Cancelled => String::from("cancelled"),
        }
    }

    /// Whether this is a thing that finished well, which is the one fact the check
    /// and the cross stand for: an exit code of zero, and nothing else.
    pub fn is_ok(self) -> bool {
        self == Exit::Code(0)
    }
}

/// What one call did: the text the model is handed back, the output bytes it
/// cost the run, how long it held the run's clock, and how it ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acted {
    /// What the model is told about this call: what it printed, and nothing about how
    /// it ended — `exit` is that half of the fact.
    pub text: String,
    /// The command output bytes it cost, which is what a budget counts.
    pub bytes: u64,
    /// How long it took, which is what the other budget counts.
    pub elapsed: Duration,
    /// How the call ended, which is what its row says and what a budget cannot see.
    pub exit: Exit,
}

/// What one request answered: the text, the calls it asked for, and what the ask
/// spent.
///
/// A reply that asks for no call is a complete answer and stops the run as
/// `final`; a reply that asks for calls hands them back to the loop, which runs
/// them and asks again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reply {
    /// The text the model answered with.
    pub text: String,
    /// The calls it asked for, in the order it asked.
    pub calls: Vec<Call>,
    /// The tokens this ask spent, as much of it as the wire reported.
    pub usage: Usage,
}

impl Reply {
    /// A reply that answers in words and asks for nothing: a complete answer.
    pub fn answer(text: impl Into<String>) -> Reply {
        Reply {
            text: text.into(),
            calls: Vec::new(),
            usage: Usage::default(),
        }
    }

    /// A reply that answers in words and asks for `calls`.
    pub fn asking(text: impl Into<String>, calls: impl IntoIterator<Item = Call>) -> Reply {
        Reply {
            text: text.into(),
            calls: calls.into_iter().collect(),
            usage: Usage::default(),
        }
    }

    /// This reply, carrying the tokens its own ask spent.
    pub fn spent(mut self, usage: Usage) -> Self {
        self.usage = usage;
        self
    }
}

/// One exchange in a run's history: the turn it belongs to, what the reply said,
/// what it asked for and what its ask spent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exchange {
    /// The turn that produced it, counted from one.
    pub turn: u32,
    /// The text the reply answered with.
    pub text: String,
    /// The calls the reply asked for.
    pub calls: Vec<Call>,
    /// The tokens this turn's ask spent: one ask's report, never a running total.
    pub usage: Usage,
}

/// One turn's request, as the model seam receives it: the human's own words, the
/// history so far, the tools this run registers and the skills already loaded.
///
/// The loaded-skill set and the history ride on the request rather than on the
/// text of the prompt, so a second turn is a second turn of one conversation
/// (§4) and a later projection onto a wire's own messages has one value to read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// Which turn this is, counted from one.
    turn: u32,
    /// The human's own words, as the run was asked.
    prompt: String,
    /// What has been asked and answered, oldest first.
    history: Vec<Exchange>,
    /// The skills loaded so far this run, by the name a load names.
    skills: Vec<String>,
    /// The tools this run registers, by the name a call names.
    tools: Vec<String>,
}

impl Request {
    /// Which turn this is, counted from one.
    pub fn turn(&self) -> u32 {
        self.turn
    }

    /// The human's own words, as the run was asked.
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// What has been asked and answered, oldest first.
    pub fn history(&self) -> &[Exchange] {
        &self.history
    }

    /// The skills loaded so far this run, in the order they were loaded.
    pub fn skills(&self) -> &[String] {
        &self.skills
    }

    /// The tools this run registers, in the order it registers them.
    pub fn tools(&self) -> &[String] {
        &self.tools
    }
}

/// The model, as the loop needs it: one request, its fragments handed to
/// `deltas` as they land, and the reply it answers with or the failure safe to
/// print about it.
///
/// This is the seam the whole lifecycle is tested at: a fake answerer drives the
/// loop with no network, and whoever implements it is the only thing that knows
/// which wire spoke.
pub trait Responder {
    /// Answer `request`, streaming every fragment into `deltas`.
    fn respond(&self, request: &Request, deltas: &mut dyn Deltas) -> Result<Reply, Failure>;
}

/// The tools, as the loop needs them: one call in, what it did out, or the
/// failure that stops the run as `error`.
///
/// The loaded-skill set rides on the call rather than on the executor, because
/// it is the run's bookkeeping and not a tool's: a `skill` call is answered by
/// the tool that holds the catalog, and what has been loaded is decided in one
/// place for both doors into it (§5).
pub trait Executor {
    /// Run `call`, offering a skill it loads to `loaded`, and say what it cost
    /// the run.
    fn execute(&self, call: &Call, loaded: &mut Loaded) -> Result<Acted, Redacted>;
}

/// The answer §5 gives a second load of one skill: the body, or the word
/// `already_loaded` and no body, because a tool result is re-sent every turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Load {
    /// The skill was not loaded this run, so this call loads it.
    Loaded,
    /// The skill was already loaded this run, so this call returns no body.
    AlreadyLoaded,
}

/// The skills loaded so far in a run's history, in the order they were loaded:
/// the one set every load consults, and the one clearing the history empties.
///
/// Both doors into it write here and neither keeps a set of its own — the
/// human's `/skill:name` and the model's `skill` call — which is what makes a
/// human load answer a later model load with `already_loaded` (§4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Loaded {
    /// The names, in load order, which is the order a request carries them in.
    names: Vec<String>,
}

impl Loaded {
    /// The names loaded so far, in load order.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Whether `name` has been loaded already.
    pub fn contains(&self, name: &str) -> bool {
        self.names.iter().any(|loaded| loaded == name)
    }

    /// Offer `name` to the set, once per conversation: a name already in it is
    /// `already_loaded` and carries no body (§5). Two identical loads in one
    /// turn land here one after the other, so the second is one load too.
    pub fn load(&mut self, name: impl Into<String>) -> Load {
        let name = name.into();
        if self.contains(&name) {
            return Load::AlreadyLoaded;
        }
        self.names.push(name);
        Load::Loaded
    }

    /// Forget every name, because the history was cleared and the conversation
    /// is the history (§4).
    pub fn clear(&mut self) {
        self.names.clear();
    }
}

/// One run: the loop that makes a turn of each request, holds the history and
/// the loaded-skill set, spends the budgets and settles on exactly one stop
/// reason.
///
/// It is generic over its two seams and over nothing else, so a run of two
/// hundred and fifty turns against a scripted model costs no network and no
/// wall clock.
pub struct Run<R, A> {
    /// The human's own words, carried into every request.
    prompt: String,
    /// Where a request goes.
    responder: R,
    /// Where a turn's calls go.
    executor: A,
    /// The dials in force.
    limits: Limits,
    /// The tools a request names, derived from the set the front end registered.
    tools: Vec<String>,
    /// The flag the user's word arrives on.
    cancel: Cancel,
    /// What the run has spent.
    spent: Spent,
    /// Turns started so far.
    turns: u32,
    /// The skills loaded in this history, in the order they were loaded.
    skills: Loaded,
    /// What has been asked and answered, oldest first.
    history: Vec<Exchange>,
    /// The text of the turn that produced a complete answer.
    answer: Option<String>,
    /// Where the run stands.
    state: State,
}

impl<R: Responder, A: Executor> Run<R, A> {
    /// A run of `prompt` against `responder`, acting through `executor`, with
    /// §4's defaults in force and no turn started yet.
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
            history: Vec::new(),
            answer: None,
            state: State::Running,
        }
    }

    /// The dials this run obeys: the turn cap and the two run-scoped budgets.
    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// The tools this run registers, taken from the set the front end built so
    /// the schema the model was given and the names a request carries are one
    /// list rather than two that could drift.
    pub fn tools(mut self, tools: &ToolSet) -> Self {
        self.tools = tools
            .tools()
            .iter()
            .map(|tool| String::from(tool.name()))
            .collect();
        self
    }

    /// The flag the user's word arrives on, so a front end can stop the run and
    /// a `^C` can stop it too.
    pub fn cancel(mut self, cancel: Cancel) -> Self {
        self.cancel = cancel;
        self
    }

    /// A handle on the flag this run observes, for whoever owns the user's word.
    pub fn canceller(&self) -> Cancel {
        self.cancel.clone()
    }

    /// Where the run stands: `running` until a stop reason lands.
    pub fn state(&self) -> State {
        self.state.clone()
    }

    /// Turns started so far.
    pub fn turns(&self) -> u32 {
        self.turns
    }

    /// What the run has spent of its budgets.
    pub fn spent(&self) -> Spent {
        self.spent
    }

    /// What has been asked and answered, oldest first.
    pub fn history(&self) -> &[Exchange] {
        &self.history
    }

    /// The skills loaded in this history, in the order they were loaded.
    pub fn skills(&self) -> &[String] {
        self.skills.names()
    }

    /// The answer a front end may print: the text of the turn that produced a
    /// complete answer, and nothing at all for every other stop reason, because
    /// stdout carries an answer and never a failure or a cancellation (§15).
    pub fn answer(&self) -> Option<&str> {
        match &self.state {
            State::Stopped(Stop::Final) => self.answer.as_deref(),
            _ => None,
        }
    }

    /// Offer `name` to the loaded-skill set, once per conversation: a name
    /// already loaded this run is `already_loaded` and carries no body (§5).
    pub fn load_skill(&mut self, name: impl Into<String>) -> Load {
        self.skills.load(name)
    }

    /// Clear the history, and the loaded-skill set with it: a load is once per
    /// conversation and the conversation is the run's history (§4).
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.skills.clear();
    }

    /// One turn: one request to the model and the calls it asks for.
    ///
    /// A stopped run starts nothing, and a run looking at a spent budget or a
    /// reached cap stops instead of starting, so no turn begins once the run has
    /// already ended.
    pub fn turn(&mut self, deltas: &mut dyn Deltas) -> State {
        if !self.state.is_running() {
            return self.state();
        }
        // A cancellation beats every other stop, so it settles first.
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
        self.history.push(Exchange {
            turn: self.turns,
            text: reply.text.clone(),
            calls: reply.calls.clone(),
            usage: reply.usage,
        });

        // Each call is charged as it lands, so a spent budget stops the rest of the turn.
        for call in &reply.calls {
            let acted = match self.executor.execute(call, &mut self.skills) {
                Ok(acted) => acted,
                Err(message) => {
                    self.settle(Stop::Error(message));
                    break;
                }
            };
            self.spent.charge(acted.elapsed, acted.bytes);
            if self.cancel.is_set() {
                self.settle(Stop::Cancelled);
                break;
            }
            if let Some(budget) = self.spent.exhausted(&self.limits) {
                self.settle(Stop::Budget(budget));
                break;
            }
        }

        if self.state.is_running() && reply.calls.is_empty() {
            self.answer = Some(reply.text);
            self.settle(Stop::Final);
        }
        self.state()
    }

    /// Turns until a stop reason lands, handing every fragment to `deltas` as it
    /// arrives, and handing back the reason the run ended.
    pub fn drive(&mut self, deltas: &mut dyn Deltas) -> State {
        while self.state.is_running() {
            self.turn(deltas);
        }
        self.state()
    }

    /// The request one turn makes, built from what the run holds now.
    fn request(&self) -> Request {
        Request {
            turn: self.turns,
            prompt: self.prompt.clone(),
            history: self.history.clone(),
            skills: self.skills.names().to_vec(),
            tools: self.tools.clone(),
        }
    }

    /// Record `stop` as why the run ends, unless a stop reason has already
    /// landed: the first trigger in time wins. A cancellation always wins over
    /// whatever else fired, because it is the user's own word (ADR-0006).
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

/// The tools a run really has, as the loop needs them: `bash`, one command
/// string run under the boundary in force and in the session temp handed in,
/// timed and counted as it is handed back; and `skill`, one name answered out of
/// the catalog discovery built.
///
/// The session temp is an input: nothing here creates, adopts or clears it away,
/// and nothing here writes a spill file either. What a command cost is decided
/// here; where an overflow of it goes is the session temp's (#16), so the two
/// never own the same fact.
///
/// The catalog is an input for the same reason: the skill tool names what
/// discovery found, so the description the model reads, the name a call may use
/// and the files a load lists are one catalog rather than three readings of a
/// directory.
#[derive(Clone, Debug)]
pub struct Shell {
    /// The boundary in force, which the argv is rendered from and from nowhere
    /// else.
    boundary: Boundary,
    /// The session temp a sandboxed command sees as `TMPDIR`.
    temp: PathBuf,
    /// The catalog in force, which is what a `skill` call names a value from.
    catalog: Catalog,
    /// The user's word, which a command in flight hears: a `bash` call killed by a
    /// cancellation is a call the run was in when it ended (§4).
    cancel: Cancel,
}

impl Shell {
    /// The tools over `boundary` and `catalog`, with `temp` as the session temp.
    pub fn new(boundary: Boundary, temp: impl Into<PathBuf>, catalog: Catalog) -> Shell {
        Shell {
            boundary,
            temp: temp.into(),
            catalog,
            cancel: Cancel::new(),
        }
    }

    /// The flag the user's word arrives on, which is the run's own: one cancellation
    /// stops the loop and the command it was in flight with, so the two doors into it
    /// cannot disagree about when the run ended (§4, ADR-0006).
    pub fn cancel(mut self, cancel: Cancel) -> Shell {
        self.cancel = cancel;
        self
    }

    /// The `bash` half: one command, run under the boundary and in the temp.
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
            exit: Exit::of(&outcome),
        })
    }

    /// The `skill` half: a name from the catalog, answered once per conversation
    /// with the body in its wrapper and no second time with the body at all.
    ///
    /// A name the catalog does not hold is a refusal, not `already_loaded`: the
    /// two are different failures and each says which one it is. A load runs no
    /// command, so it spends neither of the run's budgets.
    ///
    /// A name the catalog holds but the model may not invoke — a skill saying
    /// `disable-model-invocation: true` — is refused the same way and answered
    /// with the names this door does open, because the schema's absence is §2's
    /// first line and a call that reaches past it is §2's second.
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
            // A load runs no command, so there is no code to report.
            exit: Exit::Code(0),
        })
    }
}

impl Executor for Shell {
    fn execute(&self, call: &Call, loaded: &mut Loaded) -> Result<Acted, Redacted> {
        match call.name.as_str() {
            BASH => self.bash(&call.arguments),
            SKILL => self.skill(&call.arguments, loaded),
            other => Err(SecretSet::default().redact(format!(
                "this run cannot run `{other}`: its tools are `{BASH}` and `{SKILL}`"
            ))),
        }
    }
}

/// The names the model may call, as a refusal writes them, so a call that
/// invented a name is answered with the values it could have used instead. A
/// skill the frontmatter withholds is left out: a refusal that named it would be
/// the way around the flag.
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

/// The skill name a `skill` call wrote, out of the arguments the wire handed
/// over: `{"name": "..."}`, and nothing else.
fn name_of(arguments: &str) -> Option<String> {
    let arguments: Value = serde_json::from_str(arguments).ok()?;
    let name = arguments.get("name")?.as_str()?;
    Some(name.to_owned())
}

/// The command string a `bash` call wrote, out of the arguments the wire handed
/// over: `{"command": "..."}`, and nothing else.
fn command_of(arguments: &str) -> Option<String> {
    let arguments: Value = serde_json::from_str(arguments).ok()?;
    let command = arguments.get("command")?.as_str()?;
    Some(command.to_owned())
}

/// What one command printed: its output and nothing else. How it ended is not
/// written here, because it is not a line of the output — [`Exit`] carries it, the
/// record keeps it as its own field, and whoever hands the result back to the model
/// writes the one line from that fact rather than reading it back out of prose.
fn rendered(outcome: &Outcome) -> String {
    let mut text = outcome.stdout.clone();
    text.push_str(&outcome.stderr);
    text
}
