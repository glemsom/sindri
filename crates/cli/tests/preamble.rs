//! The preamble at the process seam: a run's request carries the workspace it
//! stands in, derived from the run's own path, and what the sandbox derives
//! stays in the tool description beside it.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::provider::{MockProvider, Reply};
use common::{Env, batch, system_text, tool_text};

#[test]
fn a_batch_run_asks_from_the_workspace_it_stands_in() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("done")]);

    let output = batch(&env, &mock, &["-b", "say done", "--format", "json"], "");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let workspace = env.canonical_workspace();
    let request = &mock.requests()[0];
    let system = system_text(request);
    assert!(
        system.contains(&format!(
            "Current working directory: {}",
            workspace.display()
        )),
        "the request names the directory the run stands in, and the run derived it: {system}"
    );
    // The Environment section names the variable a run writes its own files to;
    // the description names the path that variable holds. Neither states the
    // other's fact, so the temp is a path in exactly one place.
    let session_temp = system
        .split("Current working directory:")
        .next()
        .expect("the section is there");
    assert!(
        session_temp.contains("`$TMPDIR`"),
        "the habit is the preamble's: {system}"
    );
    assert!(
        !system.contains("(TMPDIR)"),
        "the path the variable holds is not: {system}"
    );
    assert!(
        tool_text(request).contains("(TMPDIR)"),
        "it is the `bash` description's, and it is there: {}",
        tool_text(request)
    );
}
