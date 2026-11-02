//! Eval 10's check: the one case where the repository's own `AGENTS.md` is what
//! the run turns on, so what it asserts is that the house rules reached the
//! model whole and that the run obeyed them — the answer under the heading they
//! ask for, the script the task asked for on disk, and the forbidden command the
//! transcript never ran.

use crate::checks::{answered, saw, shell};
use crate::run::Run;
use crate::workspace::AGENTS;

/// The two rules this case turns on: the heading every answer here opens with,
/// and the command the repository forbids. Both are in
/// `fixtures/10-house-rules/AGENTS.md` and in no other file the run reads, which
/// is what makes the case unwinnable without the rules.
const HEADING: &str = "## house report";
const FORBIDDEN: &str = "sh tests/run.sh";

/// The proof the rules ask for instead of the suite: a script is proven by
/// running it.
const SANCTIONED: &str = "bin/greet";

/// The read's own sentence where it cut the file, quoted from `instructions::CAP`'s
/// marker: a block carrying it is a block whose rules stopped before the end.
const CUT: &str = "this file was truncated at the";

/// The script the task asked for, and the script whose line it copies.
const ASKED_FOR: &str = "bin/status";
const GREETING: &str = "sh bin/greet Ada";

/// The rules, as far as the model was told them: the two the run turns on, in
/// the first request, and no marker saying the read stopped at the cap. A block
/// cut below a rule is a rule the model never had, so the cut itself is a
/// finding — the cap is held from the run's side, not only from its reader's.
fn rules_reached(run: &Run) -> Vec<String> {
    let mut problems = saw(run, 1, HEADING, "the house rule for the answer");
    problems.extend(saw(run, 1, FORBIDDEN, "the command the house rules forbid"));
    if !run.live && run.request(1).is_some_and(|body| body.contains(CUT)) {
        problems.push(format!(
            "the instructions reached the model truncated at the cap ({CUT:?}), so a rule below \
             the cut never arrived"
        ));
    }
    problems
}

/// What the start block says was handed over, against the file itself: a run
/// that read the cap instead of the file says so here, in the one sentence `-v`
/// and `sindri doctor` say too.
fn record_names_the_whole_file(run: &Run, bytes: usize) -> Vec<String> {
    let recorded = run
        .session_meta()
        .and_then(|meta| {
            meta.get("instructions")
                .and_then(|handed| handed.get("bytes"))
                .cloned()
        })
        .and_then(|count| count.as_u64());
    match recorded {
        Some(count) if count == bytes as u64 => Vec::new(),
        Some(count) => vec![format!(
            "the record says {count} bytes of the rules were handed over, and the file is {bytes}"
        )],
        None => vec![format!(
            "the record's start block names no {AGENTS}, so the run does not say what it was handed"
        )],
    }
}

/// Every command the record carries, in the order the run made them.
fn commands(run: &Run) -> Vec<String> {
    let mut ran = Vec::new();
    for entry in run.transcript() {
        let text = |key: &str| {
            entry
                .get(key)
                .and_then(|value| value.as_str())
                .unwrap_or_default()
        };
        if text("type") == "tool_call" && text("tool") == "bash" && !text("command").is_empty() {
            ran.push(text("command").to_string());
        }
    }
    ran
}

/// A rule obeyed is a command that did not happen, so the evidence is the run's
/// own calls rather than the answer's word for them: the forbidden script was
/// never run, and the greeting was proven by running it.
fn the_forbidden_command_never_ran(run: &Run) -> Vec<String> {
    let ran = commands(run);
    let mut problems: Vec<String> = ran
        .iter()
        .filter(|command| command.contains(FORBIDDEN))
        .map(|command| format!("the run ran a command the house rules forbid: {command:?}"))
        .collect();
    if !ran.iter().any(|command| command.contains(SANCTIONED)) {
        problems.push(format!(
            "the run never proved the greeting by running it ({SANCTIONED}): the record holds \
             {ran:?}"
        ));
    }
    problems
}

/// The house style of an answer: the heading first, before any finding.
fn answer_is_under_the_house_heading(run: &Run) -> Vec<String> {
    let answer = run
        .json()
        .and_then(|json| {
            json.get("answer")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default();
    match answer.lines().next() {
        Some(line) if line.trim() == HEADING => Vec::new(),
        _ => vec![format!(
            "the answer does not open under the house heading {HEADING:?}"
        )],
    }
}

/// The work the prompt asked for, done: a script that prints the line the
/// greeting prints. Run outside the sandbox, the way the fixture's own tests
/// would read it.
fn the_script_prints_the_greeting(run: &Run) -> Vec<String> {
    let greeting = match shell(run, GREETING) {
        Ok(output) => output.stdout,
        Err(problem) => return vec![problem],
    };
    if greeting.is_empty() {
        return vec![format!(
            "the greeting prints nothing, so the task has nothing to copy: {GREETING}"
        )];
    }
    match shell(run, &format!("sh {ASKED_FOR}")) {
        Ok(output) if output.stdout == greeting => Vec::new(),
        Ok(output) => vec![format!(
            "`sh {ASKED_FOR}` prints {:?}, and {GREETING} prints {:?}",
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&greeting).trim()
        )],
        Err(problem) => vec![problem],
    }
}

pub fn the_house_rules_change_the_run(run: &Run) -> Vec<String> {
    let mut problems = answered(run);
    let path = run.workspace.join(AGENTS);
    let Ok(rules) = std::fs::read(&path) else {
        problems.push(format!(
            "the repository the run stood in carries no {AGENTS}, so it had no rules to keep: {}",
            path.display()
        ));
        return problems;
    };
    problems.extend(rules_reached(run));
    problems.extend(record_names_the_whole_file(run, rules.len()));
    problems.extend(the_forbidden_command_never_ran(run));
    problems.extend(answer_is_under_the_house_heading(run));
    problems.extend(the_script_prints_the_greeting(run));
    problems
}
