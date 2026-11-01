//! The surface `--help` prints: the flag that leaves the sandbox is on it,
//! carrying the warning, and no spelling a script already uses moved.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::Env;

fn help(env: &Env) -> String {
    let output = env.cmd().arg("--help").output().expect("run --help");
    assert_eq!(output.status.code(), Some(0), "`--help` is not a failure");
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(!text.is_empty(), "the surface is printed on stdout");
    text
}

#[test]
fn help_lists_the_flag_that_leaves_the_sandbox_with_its_warning() {
    let env = Env::new();
    let text = help(&env);

    assert!(text.contains("--yolo-unsafe"), "{text}");
    assert!(
        text.contains("your own privileges"),
        "the flag's doc comment is the warning: {text}"
    );
    assert!(
        text.contains("outside bubblewrap"),
        "and it names what it costs: {text}"
    );
}

#[test]
fn every_flag_already_spelled_here_is_still_spelled_the_same_way() {
    let env = Env::new();
    let text = help(&env);

    for flag in [
        "-b, --batch",
        "--provider",
        "--model",
        "--base-url",
        "--max-turns",
        "--writable",
        "--header",
        "--timeout",
        "--thinking",
        "--no-thinking",
        "--format",
        "-v, --verbose",
    ] {
        assert!(text.contains(flag), "`{flag}` is missing from:\n{text}");
    }
    assert!(
        !text.contains("--no-yolo-unsafe"),
        "there is no shorthand and no negation: {text}"
    );
}

/// Clap wraps a doc comment to the terminal's width, so a phrase is matched on
/// the words rather than on where the line ends.
fn mentions(text: &str, phrase: &str) -> bool {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .contains(phrase)
}

#[test]
fn every_flag_that_printed_bare_says_what_it_is_for() {
    let env = Env::new();
    let text = help(&env);

    for phrase in [
        "Which wire to speak",
        "the turn cap",
        "one command may run inside the sandbox",
    ] {
        assert!(
            mentions(&text, phrase),
            "`{phrase}` is missing from:\n{text}"
        );
    }
}
