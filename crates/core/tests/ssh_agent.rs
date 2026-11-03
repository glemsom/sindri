//! The ssh agent as a setting: the host's socket is read once, the toggle
//! decides whether the sandbox is handed it, and the policy that carries it is
//! the one both the argv and the description read.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken setting reports itself"
)]

mod support;

use std::path::PathBuf;

use sindri_core::config::{Config, Document, Key, Overrides};
use sindri_core::session::{Env, Session, ssh_agent};
use support::config;
use tempfile::TempDir;

/// A socket path of this test's own. Nothing connects to it: the policy names
/// it and mounts nothing for it.
fn socket() -> PathBuf {
    PathBuf::from("/run/user/1000/sindri-test-agent.socket")
}

/// A host that has an agent, in a workspace of its own, with the setting as
/// named.
fn host(setting: bool) -> (TempDir, Env, sindri_core::config::Config) {
    let root = TempDir::new().expect("host");
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let env = Env {
        workspace: workspace.canonicalize().expect("canonical workspace"),
        search_path: "/usr/bin:/bin".to_string(),
        state: root.path().join("state"),
        ssh_agent: Some(socket()),
    };
    let mut config = config(&[]);
    config.ssh_agent = setting;
    (root, env, config)
}

#[test]
fn the_setting_on_hands_the_sandbox_the_socket_the_host_named() {
    let (_root, env, config) = host(true);

    let session = Session::open(&env, &config).expect("a session opens");
    let policy = session.policy();

    assert_eq!(
        policy.ssh_agent(),
        Some(socket().as_path()),
        "the policy names the one value boot read"
    );
    let argv = policy.to_argv(&["true".to_string()], None);
    let at = argv
        .iter()
        .position(|arg| arg == "SSH_AUTH_SOCK")
        .expect("the variable is set");
    assert_eq!(
        argv[at + 1],
        socket().display().to_string(),
        "and the argv exports exactly that value"
    );
    assert!(
        policy.describe().contains("SSH_AUTH_SOCK"),
        "the description speaks about the value the argv sets: {}",
        policy.describe()
    );
}

#[test]
fn the_setting_off_leaves_the_variable_absent_even_where_the_host_has_one() {
    let (_root, env, config) = host(false);

    let policy = Session::open(&env, &config)
        .expect("a session opens")
        .policy()
        .clone();

    assert_eq!(policy.ssh_agent(), None, "the setting is the last word");
    let argv = policy.to_argv(&["true".to_string()], None);
    assert!(
        !argv.iter().any(|arg| arg == "SSH_AUTH_SOCK"),
        "absent, not empty: {argv:?}"
    );
    assert!(
        !policy.describe().contains("SSH_AUTH_SOCK"),
        "and the description claims nothing the argv does not set: {}",
        policy.describe()
    );
}

#[test]
fn a_host_with_no_agent_names_none_whatever_the_setting_says() {
    let (_root, mut env, config) = host(true);
    env.ssh_agent = None;

    let policy = Session::open(&env, &config)
        .expect("a session opens")
        .policy()
        .clone();

    assert_eq!(policy.ssh_agent(), None);
    assert!(!policy.describe().contains("SSH_AUTH_SOCK"));
}

#[test]
fn the_socket_is_read_from_the_process_and_an_empty_one_is_no_socket() {
    // SAFETY: no other test in this binary reads `SSH_AUTH_SOCK`; the ones
    // above build their `Env` from a literal and never ask the process.
    unsafe { std::env::set_var("SSH_AUTH_SOCK", socket()) };
    assert_eq!(ssh_agent(), Some(socket()), "the host's own, as exported");

    // An exported variable left empty is how a shell un-sets one for a single
    // command, so it names no socket at all.
    // SAFETY: as above.
    unsafe { std::env::set_var("SSH_AUTH_SOCK", "") };
    assert_eq!(ssh_agent(), None);
    // SAFETY: as above, and the variable is left as it was found.
    unsafe { std::env::remove_var("SSH_AUTH_SOCK") };
    assert_eq!(ssh_agent(), None, "and a host with none names none");
}

#[test]
fn the_ssh_agent_is_on_unless_the_file_or_the_environment_says_off() {
    // An assistant that cannot use the keys it was given cannot finish the job,
    // so the socket is handed over unless something says otherwise — and the
    // environment is the last of the two sources to speak.
    let home = TempDir::new().expect("a config home");
    // SAFETY: this file's tests name the variables each one sets, and this one
    // speaks only for the agent.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", home.path()) };
    assert_eq!(
        setting().map(|config| config.ssh_agent),
        Ok(true),
        "a run with nothing configured hands the socket over"
    );

    let mut file =
        Document::load_at(home.path().join("sindri/config.toml")).expect("an empty document loads");
    file.set(Key::SshAgent, "off")
        .expect("the file takes a toggle");
    file.save().expect("written");
    assert_eq!(
        setting().map(|config| config.ssh_agent),
        Ok(false),
        "the file turns it off"
    );

    // SAFETY: as above, and the loop sets only the variable under test.
    unsafe { std::env::set_var("SINDRI_SSH_AGENT", "on") };
    assert_eq!(
        setting().map(|config| config.ssh_agent),
        Ok(true),
        "and SINDRI_SSH_AGENT wins over the file"
    );
}

/// The setting as the run resolves it, from the sources this process is holding.
fn setting() -> Result<Config, String> {
    Config::for_front_end(Overrides::default()).map_err(|error| error.message().to_string())
}
