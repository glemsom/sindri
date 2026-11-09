//! The sign-in command's edges without a socket: one wire to sign in to, the two that are asked
//! with a key refused, and a launch with nowhere to keep a session refused before GitHub is reached.
//! Every child gets `env_clear`, so no real `$HOME` and no session already kept in one reaches it.

use std::process::{Command, Output};

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn sindri(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sindri"))
        .env_clear()
        .args(args)
        .output()
        .expect("the binary runs")
}

#[test]
fn the_sign_in_command_offers_only_the_wire_that_signs_in() {
    let refused = sindri(&["auth", "login", "anthropic"]);

    assert_eq!(
        refused.status.code(),
        Some(2),
        "a wire that holds a key instead of signing in is a usage error (§15)"
    );
    let said = text(&refused.stderr);
    assert!(
        said.contains("copilot"),
        "the refusal names the one word the command accepts: {said}"
    );
}

#[test]
fn a_sign_in_with_nowhere_to_keep_a_session_refuses_before_it_sends() {
    let refused = sindri(&["auth", "login", "copilot"]);

    assert_eq!(
        refused.status.code(),
        Some(2),
        "with neither XDG_CONFIG_HOME nor HOME there is no file to keep it in"
    );
    let said = text(&refused.stderr);
    assert!(
        said.contains("XDG_CONFIG_HOME"),
        "the refusal names what is missing: {said}"
    );
    assert!(
        refused.stdout.is_empty(),
        "and nothing at all reaches stdout (§15): {:?}",
        text(&refused.stdout)
    );
}
