//! Proves the first run a person actually meets: the binary, no config file at
//! all, and a key supplied by the environment or pasted on stdin — the API-key
//! row on stderr, and the pasted value never in anything the program writes.
//!
//! Every child gets `env_clear`, so nothing in the test process's own environment
//! — least of all a real `$HOME` with a real key in it — can reach the binary.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tempfile::TempDir;

/// A binary invocation a test can add to, with no environment inherited.
fn sindri() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sindri"))
}

/// A config home with nothing in it: the first run of a fresh install.
fn bare_home() -> TempDir {
    tempfile::tempdir().expect("a temp dir")
}

/// A binary invocation pointed at `home` and at nothing else.
fn first_run(home: &TempDir) -> Command {
    let mut command = sindri();
    command
        .env_clear()
        .env("XDG_CONFIG_HOME", home.path())
        .env("HOME", home.path());
    command
}

/// The one file the first run would write.
fn config_file(home: &TempDir) -> PathBuf {
    home.path().join("sindri").join("config.toml")
}

/// The row a first run prints, for the config home it was given: an independent
/// statement of what §11 asks for, not the text the core composes.
fn expected_row(home: &TempDir) -> String {
    format!(
        "api key   from ANTHROPIC_API_KEY, SINDRI_API_KEY or --api-key\n          \
         will be written to {}, mode 0600\n          and is never shown again",
        config_file(home).display()
    )
}

/// A stream as a reader sees it, for asserting on `contains`.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The permission bits a file carries, without the file-type bits beside them.
fn mode(path: &Path) -> u32 {
    std::fs::metadata(path)
        .expect("a file to stat")
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn a_key_in_the_environment_is_enough_with_no_config_file() {
    let home = bare_home();

    let output = first_run(&home)
        .env("SINDRI_PROVIDER", "anthropic")
        .env("SINDRI_MODEL", "claude-sonnet-4-5")
        .env("ANTHROPIC_API_KEY", "sk-ant-the-exported-one")
        .output()
        .expect("the binary should run");

    let err = text(&output.stderr);
    assert!(
        err.contains(&expected_row(&home)),
        "the row names where the key comes from, the exact file and the mode:\n{err}"
    );
    assert!(
        output.stdout.is_empty(),
        "stdout belongs to the answer alone: {}",
        text(&output.stdout)
    );
    assert!(
        !err.contains("sk-ant-the-exported-one"),
        "the key is never echoed back:\n{err}"
    );
    assert!(
        !config_file(&home).exists(),
        "a key from the environment is already supplied, so a first run writes no file"
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "configured, and launched with neither -b nor a terminal: a usage error (§3, §15)"
    );
}

#[test]
fn a_key_on_the_flag_is_a_first_run_too() {
    let home = bare_home();

    let output = first_run(&home)
        .env("SINDRI_PROVIDER", "anthropic")
        .env("SINDRI_MODEL", "claude-sonnet-4-5")
        .args(["--api-key", "sk-ant-from-the-flag"])
        .output()
        .expect("the binary should run");

    let err = text(&output.stderr);
    assert!(err.contains(&expected_row(&home)), "{err}");
    assert!(!err.contains("sk-ant-from-the-flag"), "{err}");
    assert!(
        !config_file(&home).exists(),
        "nothing was pasted, so nothing is written"
    );
}

#[test]
fn a_pasted_key_is_written_owner_only_and_never_printed_back() {
    let home = bare_home();
    let secret = "sk-ant-pasted-9f2c-never-echoed";

    let mut child = first_run(&home)
        .env("SINDRI_PROVIDER", "anthropic")
        .env("SINDRI_MODEL", "claude-sonnet-4-5")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary should start");
    child
        .stdin
        .take()
        .expect("a piped stdin")
        .write_all(format!("  {secret}  \n").as_bytes())
        .expect("the pasted key");
    let output = child.wait_with_output().expect("the binary should finish");

    let out = text(&output.stdout);
    let err = text(&output.stderr);
    assert!(
        err.contains(&expected_row(&home)),
        "the row is printed before the key is asked for, so the path and the mode are known first:\n{err}"
    );
    assert!(
        !out.contains(secret) && !err.contains(secret),
        "the pasted value is never echoed back:\nstdout: {out}\nstderr: {err}"
    );

    let written = std::fs::read_to_string(config_file(&home)).expect("the config the run wrote");
    assert!(
        written.contains(&format!("api_key = \"{secret}\"")),
        "the value really was pasted and read — trimmed of the spaces and the newline a paste carries — rather than simply never seen: {written}"
    );
    assert_eq!(mode(&config_file(&home)), 0o600, "the file is owner-only");
    assert_eq!(
        output.status.code(),
        Some(2),
        "the key was pasted and written, and the launch still names no prompt: a usage error (§3)"
    );
}

#[test]
fn the_key_a_paste_wrote_is_read_back_without_asking_again() {
    let home = bare_home();
    let secret = "sk-ant-pasted-once";

    let mut child = first_run(&home)
        .env("SINDRI_PROVIDER", "anthropic")
        .env("SINDRI_MODEL", "claude-sonnet-4-5")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary should start");
    child
        .stdin
        .take()
        .expect("a piped stdin")
        .write_all(format!("{secret}\n").as_bytes())
        .expect("the pasted key");
    child
        .wait_with_output()
        .expect("the first run should finish");

    let second = first_run(&home)
        .env("SINDRI_PROVIDER", "anthropic")
        .env("SINDRI_MODEL", "claude-sonnet-4-5")
        .stdin(Stdio::null())
        .output()
        .expect("the binary should run");

    let err = text(&second.stderr);
    assert!(
        err.contains("and is never shown again"),
        "the row still says what the file holds and how:\n{err}"
    );
    assert!(
        !err.contains("press Enter"),
        "the second run asks for nothing: the file has the key\n{err}"
    );
    assert!(!err.contains(secret), "{err}");
    assert_eq!(
        second.status.code(),
        Some(2),
        "no prompt and no terminal is a usage error, whatever the key's state (§3)"
    );
}

#[test]
fn a_first_run_with_no_key_and_no_terminal_refuses_and_names_the_key() {
    let home = bare_home();

    let output = first_run(&home)
        .env("SINDRI_PROVIDER", "anthropic")
        .env("SINDRI_MODEL", "claude-sonnet-4-5")
        .stdin(Stdio::null())
        .output()
        .expect("the binary should run");

    let err = text(&output.stderr);
    assert!(
        err.contains(&expected_row(&home)),
        "the row is printed even when the key cannot be asked for:\n{err}"
    );
    assert!(
        err.contains("api_key"),
        "the refusal names the key that is missing:\n{err}"
    );
    assert_eq!(output.status.code(), Some(2), "a usage error");
    assert!(!config_file(&home).exists(), "nothing was written");
}
