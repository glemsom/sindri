//! What each eval asserts: exit codes, JSON fields, what the model was told and
//! what the run left on disk. Never the model's prose, which drifts — a
//! recorded answer is a fixture, and an assertion on it would only prove the
//! fixture.

use std::path::Path;

use crate::run::{Run, first_line};
use crate::workspace::{SENTINEL, SENTINEL_TEXT};

/// The paths eval 7 tries to write: parents that exist and are covered by the
/// read-only root. Never `rm -rf /`, which root preservation blocks with no
/// sandbox at all.
const PROTECTED: &[&str] = &["/etc/sindri-eval-probe", "/usr/lib/sindri-eval-probe"];

pub fn answered(run: &Run) -> Vec<String> {
    let mut problems = Vec::new();
    if run.exit_code != 0 {
        problems.push(format!(
            "exit code {} instead of 0 (stderr: {})",
            run.exit_code,
            first_line(&run.stderr)
        ));
    }
    if !run.stdout.ends_with('\n') || run.stdout.ends_with("\n\n") {
        problems.push("stdout is not the answer object plus exactly one newline".to_string());
    }
    if !run.stderr.is_empty() {
        problems.push(format!("stderr is not silent: {}", first_line(&run.stderr)));
    }
    match run.json() {
        None => problems.push(format!(
            "stdout is not one JSON object: {}",
            first_line(&run.stdout)
        )),
        Some(json) => {
            let stopped = json.get("stopped").and_then(|value| value.as_str());
            if stopped != Some("final") {
                problems.push(format!("stopped is {stopped:?}, not final"));
            }
            if json
                .get("answer")
                .and_then(|value| value.as_str())
                .is_none_or(|answer| answer.trim().is_empty())
            {
                problems.push("the answer is absent or empty".to_string());
            }
            if json
                .get("turns")
                .and_then(|value| value.as_u64())
                .is_none_or(|t| t == 0)
            {
                problems.push("turns is absent or zero".to_string());
            }
        }
    }
    problems.extend(record(run));
    problems
}

fn record(run: &Run) -> Vec<String> {
    let mut problems = Vec::new();
    let sessions = run.session_dirs().len();
    if sessions != 1 {
        problems.push(format!("the run left {sessions} session records, not one"));
    }
    let Some(meta) = run.session_meta() else {
        problems.push("the run left no readable session.json".to_string());
        return problems;
    };
    match meta.get("enforcement").and_then(|value| value.as_str()) {
        Some("bubblewrap") => {}
        other => problems.push(format!(
            "the record says enforcement {other:?}, not bubblewrap"
        )),
    }
    if run.transcript().is_empty() {
        problems.push("the run left no transcript".to_string());
    }
    problems
}

fn saw(run: &Run, turn: usize, needle: &str, what: &str) -> Vec<String> {
    if run.live {
        return Vec::new();
    }
    match run.request(turn) {
        Some(body) if body.contains(needle) => Vec::new(),
        Some(_) => vec![format!(
            "turn {turn} did not show the model {what} ({needle:?})"
        )],
        None => vec![format!(
            "the run made no turn {turn} to show {what} ({needle:?})"
        )],
    }
}

fn tests_pass(run: &Run) -> Vec<String> {
    match shell(run, "sh tests/run.sh") {
        Ok(output) if output.status.success() => Vec::new(),
        Ok(output) => vec![format!(
            "the project's tests still fail: {} {}",
            first_line(&String::from_utf8_lossy(&output.stdout)),
            first_line(&String::from_utf8_lossy(&output.stderr))
        )],
        Err(problem) => vec![problem],
    }
}

/// Runs a command in the eval's workspace, outside the sandbox: the check reads
/// the fixture the way its own tests would.
fn shell(run: &Run, script: &str) -> Result<std::process::Output, String> {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(script)
        .current_dir(&run.workspace)
        .output()
        .map_err(|error| format!("run `{script}`: {error}"))
}

pub fn summarise_a_diff(run: &Run) -> Vec<String> {
    let mut problems = answered(run);
    // The piped diff is context appended after the prompt, so a summary without
    // it in the request is a summary of nothing.
    problems.extend(saw(run, 1, "say_hello", "the diff piped in on stdin"));
    if let Some(json) = run.json() {
        let answer = json
            .get("answer")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if answer.chars().count() < 40 {
            problems.push(format!("the answer is a stub: {answer:?}"));
        }
    }
    problems
}

pub fn explain_a_file(run: &Run) -> Vec<String> {
    let mut problems = answered(run);
    problems.extend(saw(run, 2, "say_hello()", "the file it read"));
    problems.extend(saw(run, 2, "src/greet.sh", "which file it read"));
    problems
}

pub fn find_a_symbol(run: &Run) -> Vec<String> {
    let mut problems = answered(run);
    problems.extend(saw(run, 2, "src/greet.sh:", "the definition it found"));
    problems.extend(saw(run, 2, "bin/greet:", "the call site it found"));
    problems
}

pub fn rename_across_files(run: &Run) -> Vec<String> {
    let mut problems = answered(run);
    let left = crate::workspace::files_containing(&run.workspace, "say_hello");
    if !left.is_empty() {
        problems.push(format!("the old name survives in {}", left.join(", ")));
    }
    let renamed = crate::workspace::files_containing(&run.workspace, "greet_name");
    if renamed.len() < 3 {
        problems.push(format!(
            "the new name reached {} files, not the three that used the old one: {renamed:?}",
            renamed.len()
        ));
    }
    // The fixture project is red on purpose (that is eval 5's bug), so a rename
    // is proven by the front end still running rather than by the suite.
    match shell(run, "sh bin/greet Ada") {
        Ok(output) if output.status.success() => {}
        Ok(output) => problems.push(format!(
            "the renamed front end no longer runs: {}",
            first_line(&String::from_utf8_lossy(&output.stderr))
        )),
        Err(problem) => problems.push(problem),
    }
    problems
}

pub fn fix_the_failing_tests(run: &Run) -> Vec<String> {
    let mut problems = answered(run);
    problems.extend(saw(
        run,
        2,
        "FAIL: greet Ada printed Hello!",
        "the failing test it was asked to fix",
    ));
    problems.extend(tests_pass(run));
    problems
}

pub fn add_a_flag_and_a_test(run: &Run) -> Vec<String> {
    let mut problems = answered(run);
    match shell(run, "sh bin/greet --upper Ada") {
        Ok(output) => {
            let printed = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if printed != "HELLO, ADA!" {
                problems.push(format!("`greet --upper Ada` printed {printed:?}"));
            }
        }
        Err(problem) => problems.push(problem),
    }
    let tests = std::fs::read_to_string(run.workspace.join("tests/run.sh")).unwrap_or_default();
    if !tests.contains("--upper") {
        problems.push("no test for the new flag was added".to_string());
    }
    problems.extend(tests_pass(run));
    problems
}

pub fn sandbox_refuses_an_outside_write(run: &Run) -> Vec<String> {
    let mut problems = answered(run);
    // The kernel's own words, in the tool result the model read: a refusal by
    // something else would not say this, and a bypassed sandbox would not either.
    problems.extend(saw(
        run,
        2,
        "Read-only file system",
        "the kernel's refusal of the write",
    ));
    for path in PROTECTED {
        if crate::run::exists(Path::new(path)) {
            problems.push(format!("a probe escaped the sandbox: {path} exists"));
        }
    }
    // A refusal every write earns would prove nothing: the writable set must
    // still be writable.
    match std::fs::read_to_string(run.workspace.join("proof.txt")) {
        Ok(text) if text.trim() == "inside" => {}
        Ok(text) => problems.push(format!("the in-sandbox write landed as {text:?}")),
        Err(error) => problems.push(format!("the in-sandbox write did not land: {error}")),
    }
    match std::fs::read_to_string(run.workspace.join(SENTINEL)) {
        Ok(text) if text == SENTINEL_TEXT => {}
        Ok(_) => problems.push(format!("the run's own {SENTINEL} was changed")),
        Err(error) => problems.push(format!("the run's own {SENTINEL} is gone: {error}")),
    }
    if !run.live && run.requests.len() != 2 {
        problems.push(format!(
            "the run made {} requests, not the 2 the conversation has",
            run.requests.len()
        ));
    }
    problems
}

pub fn turn_cap_stops_a_loop(run: &Run, cap: u32) -> Vec<String> {
    let mut problems = Vec::new();
    if run.exit_code != 100 {
        problems.push(format!(
            "exit code {} instead of 100 (stderr: {})",
            run.exit_code,
            first_line(&run.stderr)
        ));
    }
    match run.json() {
        None => problems.push(format!(
            "stdout is not one JSON object: {}",
            first_line(&run.stdout)
        )),
        Some(json) => {
            let stopped = json.get("stopped").and_then(|value| value.as_str());
            if stopped != Some("max_turns") {
                problems.push(format!("stopped is {stopped:?}, not max_turns"));
            }
            let turns = json.get("turns").and_then(|value| value.as_u64());
            if turns != Some(u64::from(cap)) {
                problems.push(format!("turns is {turns:?}, not the cap of {cap}"));
            }
        }
    }
    if !run.live && run.requests.len() != cap as usize {
        problems.push(format!(
            "the loop asked {} times past a cap of {cap}",
            run.requests.len()
        ));
    }
    problems
}

/// The one skill the fixture repository ships, and the rule its body states.
/// The rule is in that body and in no other file of the fixture, which is what
/// makes the task unwinnable without the call.
const HOUSE_SKILL: &str = "house-style";
const HOUSE_RULE: &str = "# house: spindle-4411";

/// Every `skill` call the transcript recorded, with the word its result opened
/// with: `Some("loaded")` is a body the conversation now carries, and `None` is
/// a call the loop answered without ever reaching the tool.
fn skills_loaded(run: &Run) -> Vec<(String, Option<String>)> {
    let mut calls: Vec<(String, String, Option<String>)> = Vec::new();
    for entry in run.transcript() {
        let text = |key: &str| {
            entry
                .get(key)
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string()
        };
        match entry.get("type").and_then(|value| value.as_str()) {
            Some("tool_call") if text("tool") == "skill" => {
                calls.push((text("call_id"), text("name"), None))
            }
            Some("tool_result") => {
                let (id, outcome) = (
                    text("call_id"),
                    entry.get("outcome").and_then(|v| v.as_str()),
                );
                if let Some(call) = calls.iter_mut().find(|call| call.0 == id) {
                    call.2 = outcome.map(str::to_string);
                }
            }
            _ => {}
        }
    }
    calls
        .into_iter()
        .map(|(_, name, outcome)| (name, outcome))
        .collect()
}

/// The other half of `saw`: a needle the turn must not carry, because a rule
/// already in the prompt would let an answer be right without the call that is
/// the point of the eval.
fn never_shown(run: &Run, turn: usize, needle: &str, what: &str) -> Vec<String> {
    if run.live {
        return Vec::new();
    }
    match run.request(turn) {
        Some(body) if body.contains(needle) => vec![format!(
            "turn {turn} already carried {what} ({needle:?}), so the answer need not load it"
        )],
        _ => Vec::new(),
    }
}

/// The rule in the answer itself: the issue asks for a case that fails when the
/// answer arrives without it. Recorded only — a live model's phrasing is its
/// own, and the file it wrote is the assertion a live run can keep.
fn answer_states(run: &Run, needle: &str) -> Vec<String> {
    if run.live {
        return Vec::new();
    }
    let answer = run
        .json()
        .and_then(|json| {
            json.get("answer")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default();
    if answer.contains(needle) {
        Vec::new()
    } else {
        vec![format!("the answer never states the house rule {needle:?}")]
    }
}

/// The script the task asked for, carrying the rule where the skill puts it: a
/// body in the prompt is not a rule applied, and the second line is the one the
/// release script reads.
fn script_carries_the_rule(run: &Run) -> Vec<String> {
    let path = run.workspace.join("src/uptime.sh");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return vec![format!(
            "the script the task asked for is not there: {}",
            path.display()
        )];
    };
    let mut lines = text.lines();
    let mut problems = Vec::new();
    if lines.next() != Some("#!/bin/sh") {
        problems.push("src/uptime.sh does not open with the house shebang".to_string());
    }
    if lines.next() != Some(HOUSE_RULE) {
        problems.push(format!(
            "src/uptime.sh does not carry {HOUSE_RULE:?} on its second line"
        ));
    }
    problems
}

pub fn the_catalog_triggers(run: &Run) -> Vec<String> {
    let mut problems = answered(run);
    let loaded = skills_loaded(run);
    let claimed = loaded
        .iter()
        .any(|(name, outcome)| name == HOUSE_SKILL && outcome.as_deref() == Some("loaded"));
    if !claimed {
        problems.push(format!(
            "the run never loaded `{HOUSE_SKILL}`: the transcript records {loaded:?}"
        ));
    }
    problems.extend(never_shown(run, 1, HOUSE_RULE, "the rule the skill states"));
    // The wrapper, not the wrapper's attributes: the result is one string
    // inside the request body, so its own quotes arrive escaped.
    problems.extend(saw(
        run,
        2,
        "<skill_content",
        "the body of the skill it loaded",
    ));
    problems.extend(saw(run, 2, HOUSE_RULE, "the rule the skill states"));
    problems.extend(answer_states(run, HOUSE_RULE));
    problems.extend(script_carries_the_rule(run));
    problems
}
