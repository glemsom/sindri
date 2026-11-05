//! The session temp's two promises, proven through a `Session`: the scratch a
//! run leaves goes when the session ends, so no run reads what another wrote,
//! and the `spill/` a record names stays byte for byte, so a reader who follows
//! that path finds the output the record promised. The emptying is the session's
//! end and never the seam between two calls, which is proven here too.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a session that cannot start reports itself"
)]

mod support;

use std::path::Path;
use std::time::Duration;

use sindri_core::cancel::Cancel;
use sindri_core::exec::{BashExecutor, Outcome, ToolResult};
use sindri_core::sandbox::Enforcement;
use sindri_core::session::Session;
use support::{config, host};

/// The names a directory holds, sorted, so an assertion reads as the list.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("the directory reads")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// One `bash` call of `session`, run for real: the enforcement is the config's,
/// and the test that uses this asks for the unsafe one because the temp's
/// emptying is the session's business and not bubblewrap's, so a host without
/// `bwrap` proves it as well as one with it.
fn call(session: &Session, script: &str) -> ToolResult {
    session
        .executor(Duration::from_secs(30), Cancel::new())
        .execute(script)
        .expect("the run was not cancelled")
}

#[test]
fn a_session_that_ends_leaves_only_its_spill_in_the_temp() {
    // A command's output is bytes, so the spill is read back as bytes: a file
    // that survived as text would still be a file the record misdescribes.
    const SPILLED: &[u8] = b"the full output a truncated result names by path\n\xff\x00";
    let (_root, env) = host();
    let session = Session::open(&env, &config(&[])).expect("a session opens");
    let temp = session.dir().temp();
    let spill = session.dir().spill();
    std::fs::create_dir_all(&spill).expect("the spill directory");

    // What a run leaves behind: a file and a directory of scratch in the temp,
    // and the output kept for the record that outlives it.
    std::fs::write(temp.join("scratch.txt"), "one call's working\n").expect("scratch");
    std::fs::create_dir_all(temp.join("scratch-dir")).expect("a directory of scratch");
    std::fs::write(temp.join("scratch-dir/note"), "more scratch\n").expect("scratch");
    std::fs::write(spill.join("1-stdout.txt"), SPILLED).expect("a spill file");

    drop(session);

    assert_eq!(
        names(&temp),
        vec!["spill".to_string()],
        "the session ended, so the run's scratch went and the spill stayed"
    );
    assert_eq!(
        std::fs::read(spill.join("1-stdout.txt")).expect("the spill reads back"),
        SPILLED,
        "a record names this path, so a reader who follows it finds what was written"
    );
}

#[test]
fn a_file_one_call_writes_in_the_temp_is_read_by_the_next_call() {
    let (_root, env) = host();
    let mut unsafe_config = config(&[]);
    unsafe_config.enforcement = Enforcement::YoloUnsafe;
    let session = Session::open(&env, &unsafe_config).expect("a session opens");
    let at = session.dir().temp().join("kept.txt");

    let wrote = call(&session, &format!("printf 'kept' > '{}'", at.display()));
    assert_eq!(
        wrote.outcome,
        Outcome::Exited { exit_code: 0 },
        "the first call writes: {}",
        wrote.stderr
    );
    let read = call(&session, &format!("cat '{}'", at.display()));
    assert_eq!(
        read.stdout, "kept",
        "the next call of one session reads what the call before it wrote"
    );
}
