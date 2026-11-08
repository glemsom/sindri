//! Proves a session's directory: where it lives under the state home, the GUID
//! that names it, the temp mapped to `TMPDIR`, and the spill beside it.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use sindri_core::bash;
use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::run::Cancel;
use sindri_core::session::{Session, sessions_root};
use tempfile::TempDir;

/// An environment a test can hold as data, instead of mutating the process's.
fn environment(vars: &[(&str, &str)]) -> BTreeMap<String, String> {
    vars.iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

/// A state home of the test's own, so no real `XDG_STATE_HOME` is read or written.
fn state_home() -> TempDir {
    tempfile::tempdir().expect("a temp state home")
}

/// Real directories for the writable set, because a boundary canonicalises what
/// it is given through the filesystem.
struct Ground {
    /// Held so the directories outlive the ground that names them.
    _root: TempDir,
    /// The same directory, as the filesystem spells it.
    path: PathBuf,
}

impl Ground {
    /// A ground of its own, canonical.
    fn new() -> Ground {
        let root = tempfile::tempdir().expect("a temp ground");
        let path = std::fs::canonicalize(root.path()).expect("the ground resolves");
        Ground { _root: root, path }
    }

    /// A directory under the ground, created and returned canonical.
    fn directory(&self, name: &str) -> PathBuf {
        let directory = self.path.join(name);
        std::fs::create_dir_all(&directory).expect("the directory");
        std::fs::canonicalize(&directory).expect("the directory resolves")
    }

    /// The boundary a run holds, over the ground's workspace.
    fn boundary(&self) -> Boundary {
        let workspace = self.directory("workspace");
        Boundary::from_config(&Config::default(), &workspace, Enforcement::Bubblewrap)
            .expect("the boundary holds")
    }
}

/// What a failed bwrap said, or its status when it said nothing.
fn said(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    match stderr.is_empty() {
        true => output.status.to_string(),
        false => stderr.to_owned(),
    }
}

/// Whether this machine can confine a command at all, said loudly when it
/// cannot: a skip must be visible on a green run, so it is written to the
/// descriptor rather than through a captured `eprintln!`.
fn sandbox() -> Option<()> {
    let problem = match Command::new(bash::BWRAP).arg("--version").output() {
        Err(error) => Some(format!("bwrap not on PATH: {error}")),
        Ok(version) if !version.status.success() => {
            Some(format!("bwrap --version failed: {}", said(&version)))
        }
        Ok(_) => match Command::new(bash::BWRAP)
            .args(["--unshare-all", "--ro-bind", "/", "/", "--dev", "/dev"])
            .args(["--proc", "/proc", "--", "true"])
            .output()
        {
            Err(error) => Some(format!("bwrap could not be run: {error}")),
            Ok(probe) if !probe.status.success() => Some(format!(
                "bwrap cannot create an unprivileged sandbox: {}",
                said(&probe)
            )),
            Ok(_) => None,
        },
    };

    let Some(reason) = problem else {
        return Some(());
    };
    let line = format!("sandbox tier: skipped (bwrap unavailable: {reason})\n");
    let _ = std::io::stderr().write_all(line.as_bytes());
    None
}

#[test]
fn the_sessions_root_follows_xdg_state_home_and_falls_back_to_home() {
    let xdg = sessions_root(&environment(&[
        ("XDG_STATE_HOME", "/xdg/state"),
        ("HOME", "/home/someone"),
    ]))
    .expect("a state home is given");
    assert_eq!(xdg, PathBuf::from("/xdg/state/sindri/sessions"));

    let home = sessions_root(&environment(&[("HOME", "/home/someone")])).expect("a home is enough");
    assert_eq!(
        home,
        PathBuf::from("/home/someone/.local/state/sindri/sessions")
    );

    let none = sessions_root(&environment(&[("XDG_STATE_HOME", "")]))
        .expect_err("neither variable is set, so there is nowhere to write");
    assert_eq!(
        none.exit_code(),
        2,
        "nowhere to record a session is a usage error"
    );
    assert!(
        none.to_string().contains("XDG_STATE_HOME"),
        "the refusal names the variable to set: {none}"
    );
}

#[test]
fn a_session_owns_one_directory_named_by_its_guid() {
    let state = state_home();
    let session = Session::create(state.path()).expect("a session");

    let guid = session.guid().to_owned();

    // 8-4-4-4-12 lowercase hex, version 4, RFC 4122 variant: a GUID and not a
    // string that merely looks distinctive.
    assert_eq!(guid.len(), 36, "a GUID is 36 characters: {guid}");
    let shapes: Vec<usize> = guid.split('-').map(str::len).collect();
    assert_eq!(shapes, vec![8, 4, 4, 4, 12], "{guid}");
    assert!(
        guid.chars().all(|c| c == '-' || c.is_ascii_hexdigit()),
        "lowercase hex and dashes only: {guid}"
    );
    assert_eq!(
        guid.chars().nth(14),
        Some('4'),
        "the version nibble says version 4: {guid}"
    );
    assert!(
        matches!(guid.chars().nth(19), Some('8' | '9' | 'a' | 'b')),
        "the variant nibble is the RFC 4122 one: {guid}"
    );

    let root = std::fs::canonicalize(state.path()).expect("the state home resolves");
    assert_eq!(
        session.directory(),
        root.join("sindri").join("sessions").join(&guid)
    );
    assert_eq!(session.temp(), session.directory().join("tmp"));
    assert_eq!(session.spill_dir(), session.directory().join("spill"));
    assert!(
        session.temp().is_dir(),
        "the temp is a directory the moment the session exists: {}",
        session.temp().display()
    );
    assert!(
        session.spill_dir().is_dir(),
        "spill/ is beside the temp, created with it: {}",
        session.spill_dir().display()
    );
}

#[test]
fn output_a_budget_could_not_hold_is_spilled_and_referenced() {
    let state = state_home();
    let mut session = Session::create(state.path()).expect("a session");
    let output: &[u8] = b"line one\nline two\n\xff\x00 a tail that is not utf-8\n";

    let first = session.spill(output).expect("the output spills");

    assert!(
        first.path().starts_with(session.spill_dir()),
        "a spill lands under spill/, beside the temp: {}",
        first.path().display()
    );
    assert_eq!(
        first.bytes(),
        output.len() as u64,
        "the reference counts the bytes it holds"
    );
    assert_eq!(
        std::fs::read(first.path()).expect("the spilled file"),
        output,
        "the bytes are kept, not dropped and not lossily written"
    );
    assert_eq!(
        first.to_string(),
        first.path().display().to_string(),
        "the reference is a line the caller can print back to the model"
    );

    let second = session.spill(b"more output").expect("a second spill");

    assert_ne!(
        first.path(),
        second.path(),
        "two spills are two files, so a later one never overwrites an earlier"
    );
    assert_eq!(
        std::fs::read(first.path()).expect("the first spilled file"),
        output,
        "the first spill still holds its own bytes"
    );
    assert_eq!(
        std::fs::read_dir(session.spill_dir())
            .expect("the spill directory")
            .count(),
        2,
        "spill/ holds the spills and nothing else"
    );
}

#[test]
fn ending_a_session_empties_the_temp_and_keeps_the_directory_and_the_spill() {
    let state = state_home();
    let mut session = Session::create(state.path()).expect("a session");
    std::fs::write(session.temp().join("scratch.txt"), "scratch\n").expect("a scratch file");
    std::fs::create_dir_all(session.temp().join("nested").join("deeper")).expect("a nested dir");
    std::fs::write(session.temp().join("nested/deeper/file"), "x").expect("a nested file");
    let spilled = session.spill(b"kept\n").expect("a spill");

    session.finish().expect("the session ends");

    assert!(
        session.temp().is_dir(),
        "the temp directory itself is kept: {}",
        session.temp().display()
    );
    assert_eq!(
        std::fs::read_dir(session.temp())
            .expect("the temp directory")
            .count(),
        0,
        "the temp is emptied of everything a call left in it, nested directories included"
    );
    assert!(
        session.directory().is_dir(),
        "the session directory is kept"
    );
    assert!(session.spill_dir().is_dir(), "spill/ is kept");
    assert_eq!(
        std::fs::read(spilled.path()).expect("the spilled file"),
        b"kept\n",
        "a spilled file outlives the temp, which is the whole reason it is beside it"
    );
    assert_eq!(
        session
            .directory()
            .file_name()
            .and_then(|name| name.to_str()),
        Some(session.guid()),
        "the directory still answers to the GUID the record will name"
    );

    session
        .finish()
        .expect("ending a session twice is not a failure");
}

#[test]
fn two_sessions_are_two_directories() {
    let state = state_home();
    let one = Session::create(state.path()).expect("a session");
    let other = Session::create(state.path()).expect("another session");

    assert_ne!(one.guid(), other.guid(), "a GUID names one session");
    assert_ne!(one.directory(), other.directory());
    assert_ne!(one.temp(), other.temp());
    assert!(
        !other.directory().starts_with(one.directory())
            && !one.directory().starts_with(other.directory()),
        "neither session sits inside the other"
    );

    // The GUID is stable for the life of the session, so the record's `session`
    // field and the last line of `-v` can both be written from it.
    let guid = one.guid().to_owned();
    assert_eq!(guid, one.guid());
    assert_eq!(
        one.directory().file_name().and_then(|name| name.to_str()),
        Some(guid.as_str())
    );
}

#[test]
fn a_later_call_reads_what_an_earlier_call_wrote_in_the_temp() {
    if sandbox().is_none() {
        return;
    }
    let ground = Ground::new();
    let boundary = ground.boundary();
    let state = state_home();
    let mut session = Session::create(state.path()).expect("a session");

    let first = bash::run(
        &boundary,
        session.temp(),
        "mkdir -p \"$TMPDIR/nested\" \
         && echo kept > \"$TMPDIR/nested/one.txt\" \
         && printf '%s' \"$TMPDIR\" > \"$TMPDIR/where.txt\"",
        &Cancel::new(),
    )
    .expect("the command runs");
    assert_eq!(first.code, Some(0), "stderr: {}", first.stderr);

    let second = bash::run(
        &boundary,
        session.temp(),
        "cat \"$TMPDIR/where.txt\"; echo; cat \"$TMPDIR/nested/one.txt\"",
        &Cancel::new(),
    )
    .expect("the command runs");

    assert_eq!(second.code, Some(0), "stderr: {}", second.stderr);
    let mut lines = second.stdout.lines();
    assert_eq!(
        lines.next(),
        Some(session.temp().display().to_string().as_str()),
        "TMPDIR inside the sandbox is the session temp, so a call writes where the next one looks"
    );
    assert_eq!(
        lines.next(),
        Some("kept"),
        "the file the first call wrote is readable by the second"
    );

    session.finish().expect("the session ends");
}

#[test]
fn no_session_sees_another_sessions_temp() {
    if sandbox().is_none() {
        return;
    }
    let ground = Ground::new();
    let boundary = ground.boundary();
    let state = state_home();
    let mut one = Session::create(state.path()).expect("a session");
    let mut other = Session::create(state.path()).expect("another session");

    let wrote = bash::run(
        &boundary,
        one.temp(),
        "echo private > \"$TMPDIR/only-in-one.txt\" && ls \"$TMPDIR\"",
        &Cancel::new(),
    )
    .expect("the command runs");
    assert_eq!(wrote.code, Some(0), "stderr: {}", wrote.stderr);
    assert_eq!(wrote.stdout.trim(), "only-in-one.txt");

    let listing = bash::run(&boundary, other.temp(), "ls -A \"$TMPDIR\"", &Cancel::new())
        .expect("the command runs");
    assert_eq!(listing.code, Some(0), "stderr: {}", listing.stderr);
    assert_eq!(
        listing.stdout, "",
        "the other session's TMPDIR holds nothing of the first's"
    );

    let looking = bash::run(
        &boundary,
        other.temp(),
        "cat \"$TMPDIR/only-in-one.txt\"",
        &Cancel::new(),
    )
    .expect("the command runs");
    assert_ne!(
        looking.code,
        Some(0),
        "the other session cannot read a file this one wrote"
    );
    assert!(
        !looking.stdout.contains("private") && !looking.stderr.contains("private"),
        "not one byte of it crossed: {}{}",
        looking.stdout,
        looking.stderr
    );
    assert!(
        looking.stderr.contains("No such file or directory"),
        "the file is absent, not merely unreadable: {}",
        looking.stderr
    );
    assert_eq!(
        std::fs::read_dir(other.temp())
            .expect("the other session's temp")
            .count(),
        0,
        "the other session's temp is still empty"
    );

    one.finish().expect("the session ends");
    other.finish().expect("the other session ends");
    assert!(
        !one.temp().exists() || std::fs::read_dir(one.temp()).expect("its temp").count() == 0,
        "the temp is emptied at the end"
    );
    assert!(
        one.directory().is_dir() && one.spill_dir().is_dir(),
        "and the rest is kept"
    );
}

#[test]
fn output_past_the_budget_is_spilled_as_the_bytes_the_command_wrote() {
    if sandbox().is_none() {
        return;
    }
    let ground = Ground::new();
    let boundary = ground.boundary();
    let state = state_home();
    let mut session = Session::create(state.path()).expect("a session");

    let outcome = bash::run(
        &boundary,
        session.temp(),
        "i=0; while [ $i -lt 2000 ]; do echo \"line $i of a command that says too much\"; i=$((i+1)); done",        &Cancel::new(),
     )
    .expect("the command runs");
    assert_eq!(outcome.code, Some(0), "stderr: {}", outcome.stderr);

    // This is the caller's decision and not this module's: the output is past
    // what the run will feed back whole, so it is spilled and only a reference
    // is printed.
    let budget = 1024usize;
    assert!(
        outcome.stdout.len() > budget,
        "the command said {} bytes, past the {} the caller will hold",
        outcome.stdout.len(),
        budget
    );
    let spilled = session
        .spill(outcome.stdout.as_bytes())
        .expect("the output spills");

    assert_eq!(
        std::fs::read(spilled.path()).expect("the spilled file"),
        outcome.stdout.as_bytes(),
        "every byte of it is kept, not dropped"
    );
    assert_eq!(spilled.bytes(), outcome.stdout.len() as u64);
    assert!(spilled.path().starts_with(session.spill_dir()));
    assert!(
        spilled.to_string().ends_with(".out"),
        "the reference is a path a reader can open: {spilled}"
    );

    session.finish().expect("the session ends");
    assert_eq!(
        std::fs::read(spilled.path()).expect("the spilled file after the end"),
        outcome.stdout.as_bytes(),
        "the spilled output outlives the temp"
    );
}
