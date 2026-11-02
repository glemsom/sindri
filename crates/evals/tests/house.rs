//! Eval 10's check, which must fail when a repository's rules stop reaching the
//! run or stop being obeyed: the case exists to prove that an `AGENTS.md`
//! changes what a run does, so a check that passed with the rules gone, with
//! them cut at the cap, or with the forbidden command in the record would prove
//! nothing at all.

#![allow(
    clippy::expect_used,
    reason = "a test harness reports a setup failure by panicking"
)]

use std::path::Path;

use sindri_evals::run::Run;
use sindri_harness::Request;

/// The rules the case's repository carries, read from the fixture itself: a copy
/// written here could drift from the file the run is handed.
fn rules() -> String {
    let path = Path::new(sindri_evals::workspace::FIXTURES).join("10-house-rules/AGENTS.md");
    std::fs::read_to_string(&path).expect("the case's AGENTS.md")
}

/// The answer a run that kept the house style gave, and the JSON the batch front
/// end prints it in. Escaped here because the check reads the answer out of it.
const ANSWER: &str =
    "## house report\\n\\n`bin/status` prints `Hello!`, the line `sh bin/greet Ada` prints.";

/// The one record a run leaves, with the instructions its start block names.
fn record(repo: &Path, state: &Path, bytes: usize) {
    let session = state.join("sindri/sessions/one");
    std::fs::create_dir_all(&session).expect("the session directory");
    std::fs::write(
        session.join("session.json"),
        format!(
            "{{\"enforcement\":\"bubblewrap\",\"instructions\":{{\"path\":\"{}\",\"bytes\":{bytes}}}}}",
            repo.join("AGENTS.md").display()
        ),
    )
    .expect("the session record");
    std::fs::write(
        session.join("transcript.jsonl"),
        "{\"type\":\"tool_call\",\"turn\":1,\"call_id\":\"c1\",\"tool\":\"bash\",\"command\":\"sh bin/greet Ada\"}\n\
         {\"type\":\"tool_result\",\"turn\":1,\"call_id\":\"c1\",\"content\":\"Hello!\",\"is_error\":false,\"outcome\":\"exit 0\"}\n\
         {\"type\":\"tool_call\",\"turn\":2,\"call_id\":\"c2\",\"tool\":\"bash\",\"command\":\"sh bin/status\"}\n",
    )
    .expect("the transcript");
}

/// A run that obeyed every rule, so a test can take one thing away and watch the
/// check name it: the rules on disk, the rules in the first request, the
/// record's count of them, the script the task asked for, and the calls made.
fn obeyed() -> (tempfile::TempDir, Run) {
    let root = tempfile::TempDir::new().expect("temp dir");
    let repo = root.path().join("repo");
    std::fs::create_dir_all(repo.join("bin")).expect("the fixture's bin");
    let text = rules();
    std::fs::write(repo.join("AGENTS.md"), &text).expect("the repository's rules");
    std::fs::write(
        repo.join("bin/greet"),
        "#!/bin/sh\n. \"$(dirname \"$0\")/../src/greet.sh\"\nsay_hello \"${1:-world}\"\n",
    )
    .expect("the greeting");
    std::fs::create_dir_all(repo.join("src")).expect("the fixture's src");
    std::fs::write(
        repo.join("src/greet.sh"),
        "say_hello() {\n    echo \"Hello!\"\n}\n",
    )
    .expect("the greeting itself");
    std::fs::write(
        repo.join("bin/status"),
        "#!/bin/sh\nhere=$(dirname \"$0\")\nexec sh \"$here/greet\" Ada\n",
    )
    .expect("the script the task asked for");
    let state = root.path().join("state");
    record(&repo, &state, text.len());
    let run = Run {
        exit_code: 0,
        stdout: format!(
            "{{\"answer\":\"{ANSWER}\",\"turns\":3,\"stopped\":\"final\",\"tool_calls\":2}}\n"
        ),
        stderr: String::new(),
        requests: vec![Request::literal(
            "POST",
            "/v1/messages",
            &format!(
                "{}\n\n{text}",
                "You are Sindri, answering a developer at a Unix terminal."
            ),
        )],
        workspace: repo,
        state,
        live: false,
    };
    (root, run)
}

#[test]
fn a_run_that_kept_every_rule_passes() {
    let (_root, run) = obeyed();
    assert_eq!(
        sindri_evals::house::the_house_rules_change_the_run(&run),
        Vec::<String>::new(),
        "the baseline must be green, or every finding below proves nothing"
    );
}

#[test]
fn a_repository_that_carries_no_rules_is_reported() {
    // The case's `AGENTS.md` is gone from the repository the run stands in — the
    // fixture's own copy of it — and a run without instructions must not pass
    // the check that exists to prove they matter.
    let (_root, run) = obeyed();
    std::fs::remove_file(run.workspace.join("AGENTS.md")).expect("remove the rules");
    let problems = sindri_evals::house::the_house_rules_change_the_run(&run);
    assert!(
        problems
            .iter()
            .any(|line| line.contains("carries no AGENTS.md")),
        "a repository with no rules must be a finding: {problems:?}"
    );
}

#[test]
fn instructions_cut_at_the_cap_below_a_rule_are_reported() {
    // What the model was handed stopped short of the rule it needed, and said
    // so in the read's own marker: the cap is held from the run's side too.
    let (_root, mut run) = obeyed();
    let rules = rules();
    let head = rules
        .split("## Commands")
        .next()
        .expect("the rules keep their commands last");
    run.requests[0] = Request::literal(
        "POST",
        "/v1/messages",
        &format!(
            "{head}\n\n[sindri: this file was truncated at the 8192-byte cap; 120 bytes of it were not read]"
        ),
    );
    let problems = sindri_evals::house::the_house_rules_change_the_run(&run);
    assert!(
        problems.iter().any(|line| line.contains("truncated")),
        "a block cut at the cap must be a finding: {problems:?}"
    );
    assert!(
        problems.iter().any(|line| line.contains("sh tests/run.sh")),
        "the rule below the cut was never shown, and must be named: {problems:?}"
    );
}

#[test]
fn a_record_that_handed_over_less_than_the_file_is_reported() {
    // The same promise from the run's own side: a start block whose byte count
    // is the cap rather than the file says the rules were cut before they were
    // sent, whatever the request happens to carry.
    let (_root, run) = obeyed();
    let (repo, state) = (run.workspace.clone(), run.state.clone());
    record(&repo, &state, 8192);
    let problems = sindri_evals::house::the_house_rules_change_the_run(&run);
    assert!(
        problems.iter().any(|line| line.contains("8192")),
        "a record that handed over the cap says the read was cut: {problems:?}"
    );
}

#[test]
fn a_forbidden_command_in_the_record_is_reported() {
    // The run ran what the repository forbids. The answer may still be right;
    // the record is what the rule is kept by.
    let (root, run) = obeyed();
    let session = root.path().join("state/sindri/sessions/one");
    std::fs::write(
        session.join("transcript.jsonl"),
        "{\"type\":\"tool_call\",\"turn\":1,\"call_id\":\"c1\",\"tool\":\"bash\",\"command\":\"sh tests/run.sh\"}\n",
    )
    .expect("a transcript that ran the suite");
    let problems = sindri_evals::house::the_house_rules_change_the_run(&run);
    assert!(
        problems
            .iter()
            .any(|line| line.contains("forbid") && line.contains("tests/run.sh")),
        "a forbidden command must be a finding: {problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|line| line.contains("never proved the greeting")),
        "and the greeting it should have run instead: {problems:?}"
    );
}

#[test]
fn an_answer_outside_the_house_style_is_reported() {
    let (_root, mut run) = obeyed();
    run.stdout = "{\"answer\":\"Done: bin/status prints Hello!.\",\"turns\":3,\"stopped\":\"final\",\"tool_calls\":2}\n"
        .to_string();
    let problems = sindri_evals::house::the_house_rules_change_the_run(&run);
    assert!(
        problems.iter().any(|line| line.contains("## house report")),
        "the answer's heading is the rule the run kept: {problems:?}"
    );
}

#[test]
fn a_task_left_undone_is_reported() {
    let (_root, run) = obeyed();
    std::fs::remove_file(run.workspace.join("bin/status")).expect("remove the script");
    let problems = sindri_evals::house::the_house_rules_change_the_run(&run);
    assert!(
        problems.iter().any(|line| line.contains("bin/status")),
        "the script the task asked for is on disk or the task is not done: {problems:?}"
    );
}

/// A case the table does not list never runs, and its check's tests would go on
/// passing without it: the case is in the suite or this proves nothing.
#[test]
fn the_case_that_carries_rules_is_in_the_suite() {
    let names: Vec<&str> = sindri_evals::case::all()
        .iter()
        .map(|case| case.name)
        .collect();
    assert!(
        names.iter().any(|name| name.contains("house rules")),
        "the suite lists the case that proves a repository's rules: {names:?}"
    );
}

#[test]
fn the_house_heading_lives_in_the_rules_alone() {
    // The heading is a rule the repository states and nothing else does: were it
    // in the fixture's own README, a run could keep the style without ever being
    // handed the instructions, and the case would quietly stop proving anything.
    let repo = Path::new(sindri_evals::workspace::FIXTURES).join("repo");
    assert_eq!(
        sindri_evals::workspace::files_containing(&repo, "## house report"),
        Vec::<String>::new(),
        "the rule the eval turns on has one home"
    );
}
