//! The third tool at the process seam: absent from a batch run's schema and
//! answered there when a model names it anyway, and — in the frame, where a human
//! with a browser is — one call that reaches the host's opener with no shell
//! between them and nothing the opener prints on the terminal.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use common::provider::{MockProvider, Reply};
use common::pty::{Pty, expect, spawn, type_line};
use common::{Env, TEST_KEY, batch, stderr, stdout, write_config};
use tempfile::TempDir;

/// The target every test opens: one page, on a machine that is nobody's.
const TARGET: &str = "https://example.com/report.html";

/// A fake opener earlier on `PATH` than the host's own: it records its argv and
/// what its three descriptors point at, then says something on stdout that no
/// frame may show.
fn fake_opener() -> (TempDir, PathBuf) {
    let root = TempDir::new().expect("a directory for the fake opener");
    let program = root.path().join("xdg-open");
    let log = root.path().join("log");
    let script = format!(
        "#!/bin/sh\n\
         streams=\"\"\n\
         for fd in 0 1 2; do streams=\"$streams\\nfd$fd: $(readlink /proc/$$/fd/$fd)\"; done\n\
         {{ echo \"args: $#\"; for arg in \"$@\"; do echo \"arg: $arg\"; done; echo \"$streams\"; \
         }} > {}\n\
         echo the-opener-said-this\n",
        log.display()
    );
    std::fs::write(&program, script).expect("write the fake opener");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
        .expect("make the fake opener executable");
    (root, log)
}

/// `PATH` with the fake opener's directory in front of the host's own, so the
/// host's `xdg-open` is the fake one and every other binary is real.
fn path_with(root: &Path) -> String {
    format!("{}:{}", root.display(), common::host_path())
}

#[test]
fn a_batch_run_is_offered_no_open_tool_and_hears_why_when_it_asks() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::open_call("toolu_1", TARGET, ""),
        Reply::answer("Nothing to open here."),
    ]);

    let output = batch(&env, &mock, &["-v", "-b", "open the report"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Nothing to open here.\n");
    let requests = mock.requests();
    let tools = requests[0].json()["tools"].clone();
    let tools = tools.as_array().expect("the request carries tools");
    assert!(
        tools.iter().all(|tool| tool["name"] != "open"),
        "a script has no window, so its schema has no `open`: {tools:?}"
    );
    assert!(
        requests[1]
            .body
            .contains("there is no `open` tool in this run"),
        "a model that names it is answered with the absence, not silence: {}",
        requests[1].body
    );
    assert!(
        stderr(&output).contains(&format!("sindri: open (turn 1): {TARGET}")),
        "the verbose lines name the target: {}",
        stderr(&output)
    );
}

#[test]
fn the_frame_opens_an_https_url_on_the_host_and_its_opener_says_nothing() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the frame's `open` is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::open_call("toolu_1", TARGET, ""),
        Reply::answer("Put it on your screen."),
    ]);
    let (fake, log) = fake_opener();
    let path = path_with(fake.path());
    let base_url = mock.base_url();
    write_config(
        &env,
        &format!("provider = \"anthropic\"\nmodel = \"model-one\"\nbase_url = \"{base_url}\"\n"),
    );
    let mut child = spawn(
        &env,
        &pty,
        &[
            ("SINDRI_API_KEY", TEST_KEY),
            ("SINDRI_BASE_URL", base_url.as_str()),
            ("PATH", path.as_str()),
        ],
    );

    expect(&pty, &mut child, "cache");
    type_line(&pty, "open the report");
    expect(&pty, &mut child, "Put it on your screen.");

    // The row of the call prints the target, and there is no exit to watch: the
    // whole answer is on the screen by the time the run is over.
    let drawn = pty.screen();
    assert!(
        drawn.contains(TARGET),
        "the frame's call row prints the target: {drawn}"
    );
    assert!(
        drawn.contains("announced"),
        "a call that only hands a target over closes on its announcement: {drawn}"
    );
    assert!(
        !drawn.contains("running"),
        "and nothing is left counting as though the run were still waiting on it: {drawn}"
    );

    // What the host was asked: the argv is the opener and the target, and all
    // three streams are on /dev/null, so nothing it printed reached the frame.
    common::wait_until(&log);
    let logged = std::fs::read_to_string(&log).expect("the fake opener's log");
    assert!(
        logged.contains(&format!("args: 1\narg: {TARGET}")),
        "one argument, and no shell word in it: {logged}"
    );
    for fd in 0..=2 {
        assert!(
            logged.contains(&format!("fd{fd}: /dev/null")),
            "descriptor {fd} is on /dev/null: {logged}"
        );
    }
    assert!(
        !pty.screen().contains("the-opener-said-this"),
        "an opener's own words never land on the frame's terminal: {}",
        pty.screen()
    );

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "the frame leaves on Ctrl-Q");
}
