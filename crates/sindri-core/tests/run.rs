//! What the run loop promises: a turn is one request and the calls it asks for,
//! a complete answer stops it as `final`, the turn cap and the two run-scoped
//! budgets stop it as their own reason, and the reason is one of five words.
//!
//! The `skill` tool is proved here too, at the seam the loop hands a call to:
//! one load per conversation, the files beside the body in the wrapper, and
//! `already_loaded` and no body after that, whichever door loaded it.
//!
//! Nothing here reaches a network or spends a second: the model is a script of
//! replies and the tools are a script of costs, so a run of two hundred and
//! fifty turns and a run that exhausts an hour of wall clock both finish inside
//! one test. The budgets are proved by the costs a test states, not by sleeping
//! through them.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sindri_core::config::Config;
use sindri_core::policy::{
    Boundary, DEFAULT_COMMAND_BUDGET_MS, DEFAULT_OUTPUT_BUDGET_BYTES, Enforcement,
};
use sindri_core::provider::{Deltas, Failure};
use sindri_core::redact::{Redacted, SecretSet};
use sindri_core::run::{
    Acted, Budget, Call, Cancel, Executor, Exit, Limits, Load, Loaded, Reply, Request, Responder,
    Run, Shell, State, Stop,
};
use sindri_core::skills::Catalog;
use sindri_core::tools::{FrontEnd, ToolSet};
use sindri_core::wire::Wire;

/// A sink that keeps what streamed, so a test can see the fragments a front end
/// would have drawn and not only the folded answer.
#[derive(Debug, Default)]
struct Transcript {
    /// Every text fragment, joined.
    text: String,
}

impl Deltas for Transcript {
    fn text(&mut self, fragment: &str) {
        self.text.push_str(fragment);
    }

    fn reasoning(&mut self, _fragment: &str) {}
}

/// A model under a test's control: the replies a test canned, in order, the
/// last one repeating so "every turn asks for one more command" is said once;
/// every request it was given, kept; and every reply streamed to the sink.
struct Scripted {
    /// The replies still to hand out.
    replies: Mutex<VecDeque<Reply>>,
    /// Every request, in order, shared with the test.
    seen: Arc<Mutex<Vec<Request>>>,
}

impl Scripted {
    /// A model answering `replies` in order, then repeating the last.
    fn answering(replies: impl IntoIterator<Item = Reply>) -> Scripted {
        Scripted {
            replies: Mutex::new(replies.into_iter().collect()),
            seen: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The log of requests, shared with the test so it can read them after the
    /// loop has taken the model.
    fn log(&self) -> Arc<Mutex<Vec<Request>>> {
        Arc::clone(&self.seen)
    }
}

impl Responder for Scripted {
    fn respond(&self, request: &Request, deltas: &mut dyn Deltas) -> Result<Reply, Failure> {
        self.seen
            .lock()
            .expect("the log is not poisoned")
            .push(request.clone());
        let mut replies = self.replies.lock().expect("the script is not poisoned");
        let reply = match replies.len() {
            0 => return Err(Failure::unconfigured(Wire::Openai)),
            1 => replies.front().cloned(),
            _ => replies.pop_front(),
        }
        .expect("a reply is scripted");
        deltas.text(&reply.text);
        Ok(reply)
    }
}

/// One command the loop is asked to run, as a test asks for one.
fn command(line: &str) -> Call {
    Call::new("bash", format!(r#"{{"command":"{line}"}}"#))
}

/// A model that always asks for one more command, so no turn is ever final.
fn loopy() -> Reply {
    Reply::asking("more", [command("true")])
}

/// The tools a test hands the loop: what one call costs the run, whether calls
/// fail instead of acting, and what runs when a call lands.
struct Tools {
    /// The wall clock one call costs.
    elapsed: Duration,
    /// The output bytes one call costs.
    bytes: u64,
    /// Whether every call fails instead of acting.
    broken: bool,
    /// What runs when a call lands, which is how a test cancels a run mid-turn.
    during: Option<Box<dyn Fn() + Send + Sync>>,
    /// Every call, in order.
    seen: Arc<Mutex<Vec<Call>>>,
}

impl Tools {
    /// Tools that act at no cost.
    fn cheap() -> Tools {
        Tools::costing(Duration::ZERO, 0)
    }

    /// Tools where one call costs `elapsed` and `bytes`.
    fn costing(elapsed: Duration, bytes: u64) -> Tools {
        Tools {
            elapsed,
            bytes,
            broken: false,
            during: None,
            seen: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Tools that fail every call.
    fn broken() -> Tools {
        Tools {
            broken: true,
            ..Tools::cheap()
        }
    }

    /// This executor, with `during` run when a call lands.
    fn during(mut self, during: impl Fn() + Send + Sync + 'static) -> Tools {
        self.during = Some(Box::new(during));
        self
    }

    /// The log of calls, shared with the test so it can read them after the
    /// loop has taken the executor.
    fn ledger(&self) -> Arc<Mutex<Vec<Call>>> {
        Arc::clone(&self.seen)
    }
}

/// Every call an executor was given, in order, read off the shared log.
fn calls(ledger: &Arc<Mutex<Vec<Call>>>) -> Vec<Call> {
    ledger.lock().expect("the log is not poisoned").clone()
}

impl Executor for Tools {
    fn execute(&self, call: &Call, _loaded: &mut Loaded) -> Result<Acted, Redacted> {
        self.seen
            .lock()
            .expect("the log is not poisoned")
            .push(call.clone());
        if let Some(during) = &self.during {
            during();
        }
        if self.broken {
            return Err(SecretSet::default().redact("the command could not be run"));
        }
        Ok(Acted {
            text: String::from("exit 0\n"),
            bytes: self.bytes,
            elapsed: self.elapsed,
            exit: Exit::Code(0),
        })
    }
}

/// Every request a model was given, in order, read off the shared log.
fn requests(log: &Arc<Mutex<Vec<Request>>>) -> Vec<Request> {
    log.lock().expect("the log is not poisoned").clone()
}

#[test]
fn a_turn_is_one_request_and_the_calls_it_asks_for() {
    let model = Scripted::answering([Reply::asking("running it", [command("true")])]);
    let mut run = Run::new("count the files", model, Tools::cheap());

    let state = run.turn(&mut Transcript::default());

    assert_eq!(
        run.turns(),
        1,
        "one turn is one request, however many calls it asked for"
    );
    assert_eq!(
        state,
        State::Running,
        "a turn that asked for a call has not answered yet"
    );
}

#[test]
fn a_complete_answer_stops_the_run_as_final_with_exit_zero() {
    let mut run = Run::new(
        "say hi",
        Scripted::answering([Reply::answer("hello")]),
        Tools::cheap(),
    );
    let mut transcript = Transcript::default();

    let state = run.turn(&mut transcript);

    assert_eq!(state.word(), "final");
    assert_eq!(state.stop().map(Stop::exit_code), Some(0));
    assert_eq!(run.answer(), Some("hello"));
    assert_eq!(
        transcript.text, "hello",
        "the fragments reached the sink as they landed"
    );
}

#[test]
fn the_turn_cap_defaults_to_two_hundred_and_fifty_and_exits_one_hundred() {
    let mut run = Run::new("loop", Scripted::answering([loopy()]), Tools::cheap());

    let state = run.drive(&mut Transcript::default());

    assert_eq!(run.turns(), 250, "the cap is §4's own default");
    assert_eq!(state.word(), "max_turns");
    assert_eq!(state.stop().map(Stop::exit_code), Some(100));
    assert!(
        run.answer().is_none(),
        "a run that never answered writes nothing to stdout"
    );
}

#[test]
fn the_turn_cap_obeys_the_dial_it_is_given() {
    let mut run = Run::new("loop", Scripted::answering([loopy()]), Tools::cheap())
        .limits(Limits::new(3, Duration::from_secs(3_600), 8 * 1024 * 1024));

    let state = run.drive(&mut Transcript::default());

    assert_eq!(run.turns(), 3);
    assert_eq!(state.word(), "max_turns");
}

#[test]
fn a_failed_request_stops_the_run_as_error_with_exit_one() {
    let mut run = Run::new(
        "ask",
        Scripted::answering(std::iter::empty()),
        Tools::cheap(),
    );

    let state = run.turn(&mut Transcript::default());

    assert_eq!(state.word(), "error");
    assert_eq!(state.stop().map(Stop::exit_code), Some(1));
    let detail = state
        .stop()
        .and_then(Stop::detail)
        .expect("a failure says what failed");
    assert!(
        detail.contains("needs a credential"),
        "the failure keeps its one safe line: {detail}"
    );
    assert!(run.answer().is_none());
}

#[test]
fn a_tool_that_cannot_run_stops_the_run_as_error() {
    let mut run = Run::new(
        "run it",
        Scripted::answering([Reply::asking("trying", [command("true")])]),
        Tools::broken(),
    );

    let state = run.drive(&mut Transcript::default());

    assert_eq!(state.word(), "error");
    assert_eq!(
        state.stop().and_then(Stop::detail).as_deref(),
        Some("the command could not be run")
    );
}

#[test]
fn a_running_run_says_running_and_the_stop_set_is_five_words() {
    let mut run = Run::new(
        "say hi",
        Scripted::answering([Reply::answer("hello")]),
        Tools::cheap(),
    );

    assert_eq!(
        run.state().word(),
        "running",
        "a record says running until a stop reason lands"
    );
    assert_eq!(run.state().stop(), None);

    run.turn(&mut Transcript::default());
    assert_eq!(run.state().word(), "final");

    // The closed set, in the words the record and the JSON use, against §15's
    // one mapping from a stop reason to an exit code.
    let closed: Vec<(State, &str, u8)> = vec![
        (State::Running, "running", 0),
        (State::Stopped(Stop::Final), "final", 0),
        (State::Stopped(Stop::MaxTurns), "max_turns", 100),
        (State::Stopped(Stop::Budget(Budget::Clock)), "budget", 101),
        (State::Stopped(Stop::Budget(Budget::Output)), "budget", 101),
        (State::Stopped(Stop::Cancelled), "cancelled", 130),
        (
            State::Stopped(Stop::Error(SecretSet::default().redact("boom"))),
            "error",
            1,
        ),
    ];
    for (state, word, code) in &closed {
        assert_eq!(state.word(), *word);
        if let Some(stop) = state.stop() {
            assert_eq!(
                stop.exit_code(),
                *code,
                "{} maps to §15's code",
                stop.word()
            );
        }
    }
    assert_eq!(
        closed
            .iter()
            .filter(|(state, ..)| state.stop().is_some())
            .map(|(state, ..)| state.word())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        5,
        "five stop reasons, and no sixth word to reach"
    );
}

/// A model that answers completely, however often it is asked.
struct Complete;

impl Responder for Complete {
    fn respond(&self, _request: &Request, deltas: &mut dyn Deltas) -> Result<Reply, Failure> {
        deltas.text("an answer");
        Ok(Reply::answer("an answer"))
    }
}

// `raise(3)`, so a test can send itself the signal a terminal's `^C` sends: the
// one use of the C library here, because a signal cannot be faked and a fake is
// not what the rule about `^C` is about.
unsafe extern "C" {
    fn raise(signal: i32) -> i32;
}

/// The signal a terminal sends when the user presses `^C`.
const SIGINT: i32 = 2;

#[test]
fn the_command_wall_clock_is_a_budget_of_the_whole_run() {
    let limits = Limits::new(250, Duration::from_secs(60 * 60), 8 * 1024 * 1024);
    let tools = Tools::costing(Duration::from_secs(40 * 60), 0);
    let ledger = tools.ledger();
    let mut run = Run::new(
        "work",
        Scripted::answering([Reply::asking("working", [command("one"), command("two")])]),
        tools,
    )
    .limits(limits);

    let state = run.drive(&mut Transcript::default());

    assert_eq!(state.word(), "budget");
    assert_eq!(state.stop().map(Stop::exit_code), Some(101));
    assert_eq!(
        state.stop().and_then(Stop::detail).as_deref(),
        Some("clock")
    );
    assert_eq!(run.turns(), 1, "no turn starts once the clock is spent");
    assert_eq!(
        run.spent().clock(),
        Duration::from_secs(80 * 60),
        "the second call alone was inside the budget; the two together were not"
    );
    assert_eq!(
        calls(&ledger).len(),
        2,
        "no call is run after the budget is spent"
    );
}

#[test]
fn the_output_bytes_are_a_budget_of_the_whole_run() {
    let limits = Limits::new(250, Duration::from_secs(3_600), 8 * 1024 * 1024);
    let tools = Tools::costing(Duration::ZERO, 5 * 1024 * 1024);
    let mut run = Run::new(
        "read everything",
        Scripted::answering([Reply::asking("reading", [command("one"), command("two")])]),
        tools,
    )
    .limits(limits);

    let state = run.drive(&mut Transcript::default());

    assert_eq!(state.word(), "budget");
    assert_eq!(state.stop().map(Stop::exit_code), Some(101));
    assert_eq!(
        state.stop().and_then(Stop::detail).as_deref(),
        Some("output")
    );
    assert_eq!(run.spent().output(), 10 * 1024 * 1024);
}

#[test]
fn two_turns_of_half_an_hour_spend_the_run_an_hour_budget() {
    // Turn-scoped budgets would let this run go on forever: every call is half
    // an hour, which is inside the hour. Run-scoped, the second turn spends the
    // last of it and there is no third.
    let limits = Limits::new(250, Duration::from_secs(60 * 60), 8 * 1024 * 1024);
    let model = Scripted::answering([Reply::asking("working", [command("true")])]);
    let log = model.log();
    let mut run = Run::new(
        "work",
        model,
        Tools::costing(Duration::from_secs(30 * 60), 0),
    )
    .limits(limits);

    let state = run.drive(&mut Transcript::default());

    assert_eq!(state.word(), "budget");
    assert_eq!(run.turns(), 2, "the third turn never starts");
    assert_eq!(
        requests(&log).len(),
        2,
        "a stopped run asks the model nothing more"
    );
}

#[test]
fn cancellation_wins_over_a_budget_spent_in_the_same_turn() {
    let limits = Limits::new(250, Duration::from_secs(60 * 60), 8 * 1024 * 1024);
    let cancel = Cancel::new();
    let interrupted = cancel.clone();
    let tools = Tools::costing(Duration::from_secs(60 * 60), 0).during(move || interrupted.now());
    let ledger = tools.ledger();
    let mut run = Run::new(
        "work",
        Scripted::answering([Reply::asking("working", [command("true")])]),
        tools,
    )
    .limits(limits)
    .cancel(cancel);

    let state = run.drive(&mut Transcript::default());

    assert_eq!(
        run.spent().exhausted(&limits),
        Some(Budget::Clock),
        "the budget was spent in the same turn, and still lost"
    );
    assert_eq!(state.word(), "cancelled");
    assert_eq!(state.stop().map(Stop::exit_code), Some(130));
    assert_eq!(run.answer(), None, "a cancelled run writes no answer");
    assert_eq!(calls(&ledger).len(), 1, "the run stopped where it stood");
}

#[test]
fn the_budget_that_fired_first_wins_over_the_turn_cap() {
    // The budget is spent inside turn one, so it lands before the next turn's
    // cap check can speak.
    let limits = Limits::new(1, Duration::from_secs(60 * 60), 8 * 1024 * 1024);
    let mut run = Run::new(
        "work",
        Scripted::answering([Reply::asking("working", [command("one"), command("two")])]),
        Tools::costing(Duration::from_secs(40 * 60), 0),
    )
    .limits(limits);

    let state = run.drive(&mut Transcript::default());

    assert_eq!(state.word(), "budget");
    assert_eq!(run.spent().exhausted(&limits), Some(Budget::Clock));
}

#[test]
fn the_turn_cap_that_fired_first_wins_over_an_unspent_budget() {
    let limits = Limits::new(2, Duration::from_secs(60 * 60), 8 * 1024 * 1024);
    let mut run = Run::new(
        "work",
        Scripted::answering([Reply::asking("working", [command("true")])]),
        Tools::costing(Duration::from_secs(10 * 60), 0),
    )
    .limits(limits);

    let state = run.drive(&mut Transcript::default());

    assert_eq!(state.word(), "max_turns");
    assert_eq!(run.turns(), 2);
    assert_eq!(run.spent().exhausted(&limits), None);
}

#[test]
fn a_loaded_skill_is_named_in_every_later_turn() {
    let model = Scripted::answering([Reply::asking("working", [command("true")])]);
    let log = model.log();
    let mut run = Run::new("use the skill", model, Tools::cheap());

    assert_eq!(run.load_skill("code-review"), Load::Loaded);
    run.turn(&mut Transcript::default());
    run.turn(&mut Transcript::default());

    let seen = requests(&log);
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].skills(), ["code-review".to_owned()]);
    assert_eq!(
        seen[1].skills(),
        ["code-review".to_owned()],
        "the loaded set carries across turns"
    );
    assert_eq!(
        seen[1].history().len(),
        1,
        "and so does the history it belongs to"
    );
}

#[test]
fn clearing_the_history_forgets_the_loaded_skills() {
    let model = Scripted::answering([Reply::asking("working", [command("true")])]);
    let log = model.log();
    let mut run = Run::new("use the skill", model, Tools::cheap());

    run.turn(&mut Transcript::default());
    assert_eq!(run.load_skill("code-review"), Load::Loaded);
    run.clear_history();

    assert!(
        run.skills().is_empty(),
        "the set is cleared with the history"
    );
    assert!(run.history().is_empty());

    run.turn(&mut Transcript::default());

    let seen = requests(&log);
    assert!(seen[1].skills().is_empty(), "the next turn loads nothing");
    assert!(seen[1].history().is_empty(), "and remembers nothing");
}

#[test]
fn a_second_load_of_one_skill_is_already_loaded() {
    let mut run = Run::new("ask", Complete, Tools::cheap());

    assert_eq!(run.load_skill("code-review"), Load::Loaded);
    assert_eq!(
        run.load_skill("code-review"),
        Load::AlreadyLoaded,
        "a load happens once per conversation"
    );
    assert_eq!(run.skills(), ["code-review".to_owned()]);
}

#[test]
fn a_sigint_stops_the_run_as_cancelled_with_nothing_on_stdout() {
    // `^C` lands while the request is in flight, and the wire then hands back a
    // complete answer: the user's word still wins, so the answer is not printed.
    struct Interrupted;

    impl Responder for Interrupted {
        fn respond(&self, _request: &Request, _deltas: &mut dyn Deltas) -> Result<Reply, Failure> {
            let raised = unsafe { raise(SIGINT) };
            assert_eq!(raised, 0, "the signal was delivered to this process");
            Ok(Reply::answer("an answer nobody is waiting for"))
        }
    }

    let mut run = Run::new("ask", Interrupted, Tools::cheap()).cancel(Cancel::on_sigint());

    let state = run.drive(&mut Transcript::default());

    assert_eq!(state.word(), "cancelled");
    assert_eq!(state.stop().map(Stop::exit_code), Some(130));
    assert_eq!(
        run.answer(),
        None,
        "a cancelled run leaves stdout empty (§15)"
    );
}

#[test]
fn a_stopped_run_starts_no_further_turn() {
    let model = Scripted::answering([Reply::answer("hello")]);
    let log = model.log();
    let mut run = Run::new("say hi", model, Tools::cheap());

    let first = run.drive(&mut Transcript::default());
    let again = run.drive(&mut Transcript::default());

    assert_eq!(first, again, "a settled stop reason is not replaced");
    assert_eq!(run.turns(), 1);
    assert_eq!(requests(&log).len(), 1, "no request after the stop landed");
}

/// A boundary over a real directory, because the writable set is canonicalised
/// through the filesystem rather than tidied as a string.
fn ground() -> (tempfile::TempDir, PathBuf, Boundary) {
    let root = tempfile::tempdir().expect("a temp ground");
    let workspace = std::fs::canonicalize(root.path()).expect("the ground resolves");
    let boundary = Boundary::from_config(&Config::default(), &workspace, Enforcement::Unsafe)
        .expect("the boundary holds");
    (root, workspace, boundary)
}

#[test]
fn the_config_names_the_turn_cap_and_the_boundary_names_the_budgets() {
    let (_root, _workspace, boundary) = ground();
    let configured = Config {
        max_turns: Some(7),
        ..Config::default()
    };

    let limits = Limits::from_config(&configured, &boundary);

    assert_eq!(limits.max_turns(), 7, "the cap is the config's to set");
    // §4's two numbers, written out rather than restated from the constants the
    // code uses, so this test can disagree with the code.
    assert_eq!(limits.clock(), Duration::from_secs(60 * 60));
    assert_eq!(limits.output(), 8 * 1024 * 1024);
    assert_eq!(
        Duration::from_millis(DEFAULT_COMMAND_BUDGET_MS),
        Duration::from_secs(60 * 60),
        "the boundary's own default is §4's hour"
    );
    assert_eq!(DEFAULT_OUTPUT_BUDGET_BYTES, 8 * 1024 * 1024);

    let silent = Limits::from_config(&Config::default(), &boundary);
    assert_eq!(
        silent.max_turns(),
        250,
        "a silent config leaves §4's default in force"
    );
    assert_eq!(
        Limits::default(),
        silent,
        "and so does a run that was given no dials at all"
    );
}

#[test]
fn the_request_names_the_tools_the_front_end_registered() {
    let (_root, _workspace, boundary) = ground();
    let registered = ToolSet::new(&boundary, &Catalog::default(), FrontEnd::Terminal);
    let model = Scripted::answering([Reply::answer("hello")]);
    let log = model.log();
    let mut run = Run::new("hi", model, Tools::cheap()).tools(&registered);

    run.turn(&mut Transcript::default());

    assert_eq!(
        requests(&log)[0].tools(),
        ["bash".to_owned(), "open".to_owned()],
        "an empty catalog registers no skill tool, and the names come from the set"
    );
}

#[test]
fn the_bash_executor_counts_the_bytes_its_command_wrote() {
    let (_root, workspace, boundary) = ground();
    let shell = Shell::new(boundary, &workspace, Catalog::default());

    let acted = shell
        .execute(
            &Call::new("bash", r#"{"command":"printf hi"}"#),
            &mut Loaded::default(),
        )
        .expect("the command runs");

    assert_eq!(acted.bytes, 2, "two bytes of output were fed back");
    assert_eq!(acted.text, "hi", "the text is what the command printed");
    assert_eq!(acted.exit, Exit::Code(0), "and the verdict is its own fact");
    assert!(acted.elapsed > Duration::ZERO, "a command took some time");
}

/// The executor knows how a call ended, so the verdict is a value the run hands on and
/// never a sentence a front end reads back out of the text: the command's own code, a
/// code that is not zero, a signal that leaves no code at all, and the timeout that
/// killed the command it names (§4, §12).
#[test]
fn the_bash_executor_says_how_the_call_ended() {
    let (_root, workspace, boundary) = ground();
    let shell = Shell::new(boundary.clone(), &workspace, Catalog::default());
    let mut loaded = Loaded::default();
    let mut run = |command: &str| {
        shell
            .execute(
                &Call::new("bash", format!(r#"{{"command":"{command}"}}"#)),
                &mut loaded,
            )
            .expect("the command runs")
            .exit
    };

    assert_eq!(run("true"), Exit::Code(0), "a command that finished well");
    assert_eq!(
        run("exit 3"),
        Exit::Code(3),
        "and one that did not, as it said"
    );
    assert_eq!(
        run("kill -KILL $$"),
        Exit::Signalled,
        "a signal leaves no code to report"
    );

    // One second is the shortest per-call timeout a policy allows, and a command that
    // outlives it is killed rather than waited out.
    let impatient = Boundary::from_config(
        &Config {
            timeout: Some(1),
            ..Config::default()
        },
        &workspace,
        Enforcement::Unsafe,
    )
    .expect("the boundary holds");
    let killed = Shell::new(impatient, &workspace, Catalog::default())
        .execute(&Call::new("bash", r#"{"command":"sleep 30"}"#), &mut loaded)
        .expect("the command runs");

    assert_eq!(killed.exit, Exit::TimedOut, "the timeout says so");
}

#[test]
fn a_bash_call_that_names_no_command_is_an_error() {
    let (_root, workspace, boundary) = ground();
    let shell = Shell::new(boundary, &workspace, Catalog::default());

    let missing = shell
        .execute(&Call::new("bash", "{}"), &mut Loaded::default())
        .expect_err("no command was written, so nothing can run");
    assert!(
        missing.to_string().contains("command"),
        "the refusal says what a call has to write: {missing}"
    );

    let unimplemented = shell
        .execute(&Call::new("open", "{}"), &mut Loaded::default())
        .expect_err("this executor answers bash and skill and nothing else");
    assert!(
        unimplemented.to_string().contains("open"),
        "the refusal names the tool it cannot run: {unimplemented}"
    );
}

/// A workspace that holds one skill on disk, with files beside its `SKILL.md`,
/// so a load can be checked for the body it hands back and the neighbours it
/// names. The skill is in the repository scope, one directory above the
/// workspace, which is where discovery has to find it.
struct SkillGround {
    /// Keeps the tree alive for as long as the test.
    _root: tempfile::TempDir,
    /// The directory the run stands in.
    workspace: PathBuf,
    /// The boundary in force over that workspace.
    boundary: Boundary,
    /// The session temp a sandboxed command sees.
    temp: PathBuf,
}

impl SkillGround {
    /// A tree with one skill named `deploy`, whose body refers to two files
    /// beside it: one at its top and one in a subdirectory.
    fn new() -> SkillGround {
        let root = tempfile::tempdir().expect("a temp ground");
        let repo = root.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).expect("the git marker");
        let workspace = repo.join("work");
        std::fs::create_dir_all(&workspace).expect("the workspace");
        let temp = root.path().join("temp");
        std::fs::create_dir_all(&temp).expect("the session temp");

        let skill = repo.join(".agents/skills/deploy");
        std::fs::create_dir_all(skill.join("scripts")).expect("the skill's scripts");
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: deploy\ndescription: ship the branch\n---\n\n\
             Run `scripts/release.sh`, then follow `checklist.md`.\n",
        )
        .expect("the skill file");
        std::fs::write(skill.join("scripts/release.sh"), "#!/bin/sh\n").expect("a script");
        std::fs::write(skill.join("checklist.md"), "- [ ] tag the branch\n").expect("a page");

        let workspace = std::fs::canonicalize(&workspace).expect("the ground resolves");
        let boundary = Boundary::from_config(&Config::default(), &workspace, Enforcement::Unsafe)
            .expect("the boundary holds");
        SkillGround {
            _root: root,
            workspace,
            boundary,
            temp,
        }
    }

    /// The catalog a run standing in this workspace discovers.
    fn catalog(&self) -> Catalog {
        Catalog::discover_with_builtin(&self.workspace, None, &[])
    }

    /// The `skill` tool over this ground's catalog.
    fn shell(&self) -> Shell {
        Shell::new(self.boundary.clone(), &self.temp, self.catalog())
    }

    /// A `skill` call naming `name`.
    fn load(&self, name: &str) -> Call {
        Call::new("skill", format!(r#"{{"name":"{name}"}}"#))
    }

    /// The `skill` tool with a recorder around it, so a test can read what the
    /// model was told and not only what it asked for: the loop keeps the calls,
    /// and the answer is what this ticket is about.
    fn recorded(&self) -> (Recorded, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        (
            Recorded {
                shell: self.shell(),
                seen: Arc::clone(&seen),
            },
            seen,
        )
    }
}

/// The `skill` tool, with every answer it gave kept in a log the test reads
/// afterwards.
struct Recorded {
    /// The tool itself, which is the seam under test.
    shell: Shell,
    /// What each call answered, in order.
    seen: Arc<Mutex<Vec<String>>>,
}

impl Executor for Recorded {
    fn execute(&self, call: &Call, loaded: &mut Loaded) -> Result<Acted, Redacted> {
        let acted = self.shell.execute(call, loaded)?;
        self.seen
            .lock()
            .expect("the log is not poisoned")
            .push(acted.text.clone());
        Ok(acted)
    }
}

/// Every answer the skill tool gave, in order.
fn answers(log: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    log.lock().expect("the log is not poisoned").clone()
}

#[test]
fn a_first_load_hands_back_the_body_with_the_files_beside_it() {
    let ground = SkillGround::new();
    let shell = ground.shell();
    let mut loaded = Loaded::default();

    let acted = shell
        .execute(&ground.load("deploy"), &mut loaded)
        .expect("the skill is in the catalog");

    assert!(
        acted
            .text
            .contains("Run `scripts/release.sh`, then follow `checklist.md`."),
        "the body is handed back: {}",
        acted.text
    );
    assert!(
        !acted.text.contains("ship the branch") && !acted.text.contains("name: deploy"),
        "the frontmatter is stripped rather than echoed: {}",
        acted.text
    );
    assert!(
        acted.text.contains("`scripts/release.sh`") && acted.text.contains("`checklist.md`"),
        "the files beside the SKILL.md are listed, so a relative reference resolves: {}",
        acted.text
    );
}

#[test]
fn a_second_load_answers_already_loaded_and_carries_no_body() {
    let ground = SkillGround::new();
    let shell = ground.shell();
    let mut loaded = Loaded::default();

    shell
        .execute(&ground.load("deploy"), &mut loaded)
        .expect("the first load");
    let acted = shell
        .execute(&ground.load("deploy"), &mut loaded)
        .expect("the second load is answered, not refused");

    assert!(
        acted.text.contains("already_loaded"),
        "the second load says why it carries nothing: {}",
        acted.text
    );
    assert!(
        !acted.text.contains("Run `scripts/release.sh`"),
        "and carries no body, because a tool result is re-sent every turn: {}",
        acted.text
    );
    assert!(
        !acted.text.contains("checklist.md"),
        "nor the file listing the body was wrapped with: {}",
        acted.text
    );
}

#[test]
fn a_user_only_skill_is_refused_when_the_model_names_it_anyway() {
    let root = tempfile::tempdir().expect("a temp ground");
    let workspace = std::fs::canonicalize(root.path()).expect("the ground resolves");
    let skills = workspace.join(".agents/skills");
    let skill = |name: &str, description: &str, flag: &str| {
        std::fs::create_dir_all(skills.join(name)).expect("the skill directory");
        std::fs::write(
            skills.join(name).join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n{flag}\n---\n\n{name} body.\n"),
        )
        .expect("the skill file");
    };
    skill("deploy", "ship the branch", "");
    skill(
        "handover",
        "the person's own notes",
        "disable-model-invocation: true",
    );
    skill(
        "journal",
        "the person's own diary",
        "disable-model-invocation: true",
    );

    let boundary = Boundary::from_config(&Config::default(), &workspace, Enforcement::Unsafe)
        .expect("the boundary holds");
    let shell = Shell::new(boundary, &workspace, Catalog::discover(&workspace, None));
    let mut loaded = Loaded::default();

    let refused = shell
        .execute(&Call::new("skill", r#"{"name":"handover"}"#), &mut loaded)
        .expect_err("the model may not load a skill its frontmatter withholds");

    assert!(
        refused.to_string().contains("`deploy`"),
        "the refusal offers the names the model could have used: {refused}"
    );
    assert!(
        !refused.to_string().contains("journal"),
        "and not one of the names it may not use, which would be a way around the flag: {refused}"
    );
    assert!(
        !refused.to_string().contains("handover body."),
        "no instructions are handed back: {refused}"
    );
    assert!(
        !loaded.contains("handover"),
        "and a refused load is not marked loaded, so it stays refused"
    );
}

#[test]
fn an_unknown_skill_name_is_refused_by_naming_the_catalog() {
    let ground = SkillGround::new();
    let shell = ground.shell();
    let mut loaded = Loaded::default();

    let refused = shell
        .execute(&ground.load("deployy"), &mut loaded)
        .expect_err("the catalog does not hold that name");

    assert!(
        refused.to_string().contains("deployy"),
        "the refusal names what was asked for: {refused}"
    );
    assert!(
        refused.to_string().contains("deploy"),
        "and the catalog it could have named instead: {refused}"
    );
    assert!(
        !loaded.contains("deployy"),
        "a refused name is not loaded, so a later call is not already_loaded"
    );
}

#[test]
fn two_identical_loads_in_one_turn_are_one_load() {
    let ground = SkillGround::new();
    let (tools, log) = ground.recorded();
    let model = Scripted::answering([
        Reply::asking("loading", [ground.load("deploy"), ground.load("deploy")]),
        Reply::answer("done"),
    ]);
    let mut run = Run::new("deploy it", model, tools);

    run.drive(&mut Transcript::default());

    let answers = answers(&log);
    assert_eq!(answers.len(), 2, "both calls are answered");
    assert!(
        answers[0].contains("Run `scripts/release.sh`"),
        "the first call loads the body: {}",
        answers[0]
    );
    assert!(
        answers[1].contains("already_loaded") && !answers[1].contains("Run `scripts/release.sh`"),
        "the second is one load too, and carries no body: {}",
        answers[1]
    );
    assert_eq!(
        run.skills(),
        ["deploy".to_owned()],
        "the set holds the name once"
    );
}

#[test]
fn a_human_load_makes_the_next_model_load_already_loaded() {
    let ground = SkillGround::new();
    let (tools, log) = ground.recorded();
    let model = Scripted::answering([
        Reply::asking("loading", [ground.load("deploy")]),
        Reply::answer("done"),
    ]);
    let mut run = Run::new("deploy it", model, tools);

    assert_eq!(
        run.load_skill("deploy"),
        Load::Loaded,
        "the human loads it first, with `/skill:deploy`"
    );
    run.drive(&mut Transcript::default());

    let answers = answers(&log);
    assert_eq!(answers.len(), 1);
    assert!(
        answers[0].contains("already_loaded") && !answers[0].contains("Run `scripts/release.sh`"),
        "the model's later load is the same conversation's load, so it carries no body: {}",
        answers[0]
    );
}

#[test]
fn clearing_the_history_allows_a_fresh_load() {
    let ground = SkillGround::new();
    let (tools, log) = ground.recorded();
    let model = Scripted::answering([
        Reply::asking("loading", [ground.load("deploy")]),
        Reply::asking("loading again", [ground.load("deploy")]),
        Reply::answer("done"),
    ]);
    let mut run = Run::new("deploy it", model, tools);

    run.turn(&mut Transcript::default());
    run.clear_history();
    run.drive(&mut Transcript::default());

    let answers = answers(&log);
    assert_eq!(answers.len(), 2, "one load before `/new` and one after it");
    assert!(
        answers[1].contains("Run `scripts/release.sh`"),
        "a cleared history is a new conversation, so the body is handed over again: {}",
        answers[1]
    );
}

#[test]
fn a_builtin_skill_loads_from_the_binary_and_lists_no_files_beside_it() {
    let root = tempfile::tempdir().expect("a temp ground");
    let workspace = std::fs::canonicalize(root.path()).expect("the ground resolves");
    let boundary = Boundary::from_config(&Config::default(), &workspace, Enforcement::Unsafe)
        .expect("the boundary holds");
    let shell = Shell::new(boundary, &workspace, Catalog::discover(&workspace, None));
    let mut loaded = Loaded::default();

    let acted = shell
        .execute(
            &Call::new("skill", r#"{"name":"commit-message"}"#),
            &mut loaded,
        )
        .expect("the binary carries this skill");

    assert!(
        acted
            .text
            .contains("Write from the diff, never from memory"),
        "the body is handed back: {}",
        acted.text
    );
    assert!(
        !acted.text.contains("description:") && !acted.text.contains("name: commit-message"),
        "the frontmatter is stripped rather than echoed: {}",
        acted.text
    );
    assert!(
        acted.text.contains("compiled into the binary"),
        "a skill with no directory says there is nothing beside it: {}",
        acted.text
    );
    assert!(
        !acted.text.contains("Files in this skill's directory"),
        "and does not list a directory it does not have: {}",
        acted.text
    );
}

/// Waits until `path` exists, and says whether it ever did: a command is made to say
/// it has started by writing a file, so no test guesses with a sleep.
fn started_at(path: &Path) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn a_call_the_users_word_stops_is_a_cancelled_exit() {
    let (_root, workspace, boundary) = ground();
    let running = workspace.join("running");
    let cancel = Cancel::new();
    let interrupted = cancel.clone();
    let shell = Shell::new(boundary, &workspace, Catalog::default()).cancel(cancel);
    let line = format!("touch '{}'; sleep 20", running.display());

    let calling = std::thread::spawn(move || {
        shell
            .execute(
                &Call::new("bash", format!(r#"{{"command":"{line}"}}"#)),
                &mut Loaded::default(),
            )
            .expect("the command runs")
    });

    assert!(
        started_at(&running),
        "the command started, so there is something in flight to stop"
    );
    interrupted.now();
    let acted = calling.join().expect("the call returns");

    assert_eq!(
        acted.exit,
        Exit::Cancelled,
        "the row a stopped call opens says the user's word stopped it (§12)"
    );
}

#[test]
fn a_run_cancelled_mid_command_kills_it_and_stops_cancelled() {
    let (_root, workspace, boundary) = ground();
    let running = workspace.join("running");
    let late = workspace.join("late.txt");
    let cancel = Cancel::new();
    let interrupted = cancel.clone();
    let shell = Shell::new(boundary, &workspace, Catalog::default()).cancel(cancel.clone());
    let line = format!(
        "touch '{}'; sleep 2; echo late > '{}'",
        running.display(),
        late.display()
    );
    let model = Scripted::answering([Reply::asking("one command", [command(&line)])]);
    let mut run = Run::new("ask", model, shell).cancel(cancel);

    let driving = std::thread::spawn(move || {
        let mut sink = Transcript::default();
        run.drive(&mut sink)
    });

    assert!(
        started_at(&running),
        "the run is in flight in a command of its own"
    );
    let cancelled_at = std::time::Instant::now();
    interrupted.now();
    let state = driving.join().expect("the run returns");

    assert_eq!(
        state.stop(),
        Some(&Stop::Cancelled),
        "the user's word stops the run the moment the command it was in ends (§4)"
    );
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(10),
        "and it did not wait the command out"
    );
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        !late.exists(),
        "the command the run was in was killed, so it wrote nothing after the word"
    );
}
