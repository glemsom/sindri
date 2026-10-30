//! The eight evals: a prompt, the conversation a recorded run replays, and the
//! checks that make the eval fail when the product regresses.

use crate::checks;
use crate::run::Run;

pub struct Case {
    pub name: &'static str,
    pub prompt: &'static str,
    pub stdin_file: Option<&'static str>,
    pub args: &'static [&'static str],
    pub recorded: &'static str,
    pub check: fn(&Run) -> Vec<String>,
}

pub fn all() -> Vec<Case> {
    vec![
        Case {
            name: "1  summarise a diff",
            prompt: "Summarise this diff in two or three sentences: what changed, and why it matters.",
            stdin_file: Some("diff.patch"),
            args: &[],
            recorded: "1-summarise-diff",
            check: checks::summarise_a_diff,
        },
        Case {
            name: "2  explain a file",
            prompt: "Explain what src/greet.sh does and who calls it.",
            stdin_file: None,
            args: &[],
            recorded: "2-explain-file",
            check: checks::explain_a_file,
        },
        Case {
            name: "3  find and explain a symbol",
            prompt: "Find where say_hello is defined and where it is used, then explain it.",
            stdin_file: None,
            args: &[],
            recorded: "3-find-a-symbol",
            check: checks::find_a_symbol,
        },
        Case {
            name: "4  rename a symbol across files",
            prompt: "Rename say_hello to greet_name everywhere it appears, and check that bin/greet still runs.",
            stdin_file: None,
            args: &[],
            recorded: "4-rename-a-symbol",
            check: checks::rename_across_files,
        },
        Case {
            name: "5  run the tests and fix the failure",
            prompt: "Run the project's tests, find out why they fail, and fix it.",
            stdin_file: None,
            args: &[],
            recorded: "5-fix-the-tests",
            check: checks::fix_the_failing_tests,
        },
        Case {
            name: "6  add a flag and a test",
            prompt: "Add an --upper flag to bin/greet and a test for it in tests/run.sh.",
            stdin_file: None,
            args: &[],
            recorded: "6-add-a-flag",
            check: checks::add_a_flag_and_a_test,
        },
        Case {
            name: "7  sandbox refuses a write outside the writable set",
            prompt: "Try to write a probe to /etc/sindri-eval-probe and to /usr/lib/sindri-eval-probe, then write proof.txt inside the workspace. Report what each attempt did.",
            stdin_file: None,
            args: &[],
            recorded: "7-sandbox",
            check: checks::sandbox_refuses_an_outside_write,
        },
        Case {
            // The cap is put below the conversation's own length on purpose: a
            // run that ignored it would keep asking, and the mock would answer.
            name: "8  turn cap stops a loopy prompt",
            prompt: "Keep probing the tree until you are completely sure you have seen everything.",
            stdin_file: None,
            args: &["--max-turns", "3"],
            recorded: "8-turn-cap",
            check: |run| checks::turn_cap_stops_a_loop(run, 3),
        },
    ]
}
