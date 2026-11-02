//! Eval 9's check: the one case whose rule lives in a skill rather than in the
//! fixture, so what it asserts is that the catalog triggered — the call in the
//! record, the body in the next turn's prompt, and the rule in the answer and in
//! the script the task asked for.

use crate::checks::{answered, saw};
use crate::run::Run;

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
