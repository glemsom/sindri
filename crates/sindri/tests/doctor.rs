//! What `sindri doctor` reports to a person at a prompt, measured where a
//! script measures it: the built binary is spawned in a workspace, a config home
//! and a state home of the test's own, and its stdout, its stderr and its exit
//! code are read back as bytes.
//!
//! The description's bytes are an eval and not a test (§16): `eval_doctor` spawns
//! this binary, cuts the description out of its stdout, and holds it to the bytes
//! the `bash` tool a run registers renders, so a doctor that re-rendered the
//! boundary in its own words fails there rather than passing by agreeing with
//! itself. What is left here is what the report says and what it costs. Every child
//! gets `env_clear`, so nothing of this process's environment reaches the binary.
//!
//! A failing sandbox check is reachable without a broken kernel by putting a
//! stand-in `bwrap` first on `PATH`, the way `scripts/sandbox-probe-test.sh`
//! fakes a machine: one that refuses to unshare, and one that records the argv it
//! was handed so the proof can be shown to be a real invocation.
//!
//! The tests that read the report off this machine's own `PATH` do not require it
//! to be able to make a sandbox: `doctor` prints the whole report either way and
//! ends `0` over a sandbox it proved and `3` over one the kernel refused (§15, A1),
//! and that a host may forbid the second is a fact about the host rather than a
//! verdict on the report. What is not relaxed is the description's bytes — nor the
//! card, which a faked machine proves where this one may not.

use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::skills::Catalog;
use sindri_core::tools::{BASH, FrontEnd, ToolSet};
use tempfile::TempDir;

/// A workspace, a config home and a state home, all of the test's own.
struct Ground {
    /// Held so the directories outlive the paths that name them.
    _root: TempDir,
    /// The one directory the run stands in.
    workspace: PathBuf,
    /// The config home, which holds no file unless a test writes one.
    config: PathBuf,
    /// The state home, which doctor writes nothing into.
    state: PathBuf,
}

impl Ground {
    /// A ground with an empty workspace and empty homes.
    fn new() -> Ground {
        let root = tempfile::tempdir().expect("a temp ground");
        let ground = Ground {
            workspace: root.path().join("workspace"),
            config: root.path().join("config"),
            state: root.path().join("state"),
            _root: root,
        };
        for path in [&ground.workspace, &ground.config, &ground.state] {
            std::fs::create_dir_all(path).expect("a directory");
        }
        ground
    }

    /// The workspace as the child's `getcwd` spells it: canonical, because the
    /// boundary the description is rendered from is built from that path.
    fn workspace(&self) -> PathBuf {
        std::fs::canonicalize(&self.workspace).expect("the workspace resolves")
    }

    /// A skill directory `SKILL.md` under `scope`, written with the frontmatter
    /// the core reads.
    fn skill(&self, scope: &Path, name: &str, description: &str) {
        let directory = scope.join(name);
        std::fs::create_dir_all(&directory).expect("a skill directory");
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\nBody of {name}.\n"),
        )
        .expect("a SKILL.md");
    }

    /// A skill in the `repo` scope: the `.agents/skills` of the workspace, which
    /// is the repository root because the temp tree has no `.git`.
    fn repo_skill(&self, name: &str, description: &str) {
        let scope = self.workspace.join(".agents").join("skills");
        std::fs::create_dir_all(&scope).expect("a repo scope");
        self.skill(&scope, name, description);
    }

    /// A `SKILL.md` under the repo scope that will not parse: no frontmatter at
    /// all, which is refused at its first line.
    fn broken_repo_skill(&self, name: &str) -> PathBuf {
        let directory = self.workspace.join(".agents").join("skills").join(name);
        std::fs::create_dir_all(&directory).expect("a skill directory");
        let file = directory.join("SKILL.md");
        std::fs::write(&file, "Instructions, and nothing that names them.\n").expect("a SKILL.md");
        file
    }

    /// A skill in the `user` scope: `$HOME/.agents/skills`, and `HOME` is the
    /// config home here.
    fn user_skill(&self, name: &str, description: &str) {
        let scope = self.config.join(".agents").join("skills");
        std::fs::create_dir_all(&scope).expect("a user scope");
        self.skill(&scope, name, description);
    }

    /// The one directory a scope's skill lies in, as a stand-in bwrap's world.
    fn user_skill_path(&self, name: &str) -> PathBuf {
        self.config.join(".agents").join("skills").join(name)
    }

    /// A machine whose `bwrap` is the script `body`, first on `PATH`, with the
    /// real `/usr/bin` and `/bin` behind it so only bubblewrap is faked.
    fn machine(&self, body: &str) -> String {
        let bin = self.config.join("machine");
        std::fs::create_dir_all(&bin).expect("a machine directory");
        let bwrap = bin.join("bwrap");
        std::fs::write(&bwrap, format!("#!/bin/sh\n{body}\n")).expect("a stand-in bwrap");
        std::fs::set_permissions(&bwrap, Permissions::from_mode(0o755))
            .expect("the stand-in is executable");
        format!("{}:/usr/bin:/bin", bin.display())
    }

    /// A child pointed at this ground and nowhere else: no inherited environment,
    /// the workspace as its working directory, and no key, because a diagnostic
    /// answers without one.
    fn command(&self, path: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sindri"));
        command
            .env_clear()
            .env("PATH", path)
            .env("XDG_CONFIG_HOME", &self.config)
            .env("XDG_STATE_HOME", &self.state)
            .env("HOME", &self.config)
            .current_dir(&self.workspace);
        command
    }
}

/// The session directories the state home holds, which a diagnostic keeps none of.
fn kept_sessions(ground: &Ground) -> Vec<String> {
    let root = ground.state.join("sindri").join("sessions");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

/// A bwrap that reports a version and whose kernel will not let it unshare.
const RESTRICTED: &str = "if [ \"${1:-}\" = --version ]; then\n  \
                          echo 'bubblewrap 0.12.0 (stand-in)'\n  exit 0\nfi\n\
                          echo 'bwrap: setting up uid map: Permission denied' >&2\nexit 1";

/// A stream as a reader sees it.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Runs `command` to its end.
fn run(command: &mut Command) -> Output {
    command.output().expect("the binary should run")
}

/// Asserts that `doctor` answered with a report rather than a refusal of its own:
/// `0` over a sandbox this machine proved, and `3` over one its kernel would not
/// let it make — the report is whole in both, with the card printed under a
/// `sandbox failed` row (§15, A1). Which of the two this host is, is the host's
/// fact; the card itself is read in
/// `a_sandbox_check_that_fails_is_named_in_a_card_and_exits_three`, from a machine
/// that refuses on demand.
fn a_report_was_made(output: &Output) {
    assert!(
        matches!(output.status.code(), Some(0) | Some(3)),
        "doctor ended {:?} instead of reporting: stdout:\n{}\nstderr:\n{}",
        output.status.code(),
        text(&output.stdout),
        text(&output.stderr)
    );
}

/// The one line of the report `name` is listed on, so a row is read whole rather
/// than by the substring that happened to be asserted.
fn row<'a>(stdout: &'a str, name: &str) -> &'a str {
    stdout
        .lines()
        .find(|line| line.split_whitespace().any(|word| word == name))
        .unwrap_or_else(|| panic!("doctor never listed `{name}`:\n{stdout}"))
}

#[test]
fn every_skill_found_is_listed_with_the_scope_it_came_from() {
    let ground = Ground::new();
    ground.user_skill("greet", "say hello to whoever is there");
    ground.repo_skill("review", "read the diff before you commit it");
    let path = std::env::var("PATH").expect("this machine has a PATH");

    let output = run(ground.command(&path).args(["doctor"]));

    let out = text(&output.stdout);
    a_report_was_made(&output);

    // One builtin is compiled in, so three skills are found: one per scope.
    assert!(
        out.contains("skills (3 found)"),
        "the catalog is counted, and the builtin is one of them:\n{out}"
    );
    let builtin = row(&out, "commit-message");
    assert!(
        builtin.trim_start().starts_with("builtin"),
        "a compiled-in skill says so, having no path to show: {builtin}"
    );
    let user = row(&out, "greet");
    assert!(
        user.trim_start().starts_with("user"),
        "a skill under ~/.agents/skills is the user's own: {user}"
    );
    assert!(
        user.contains("say hello to whoever is there"),
        "and the one line beside it is the skill's own: {user}"
    );
    let repo = row(&out, "review");
    assert!(
        repo.trim_start().starts_with("repo"),
        "a skill under the workspace is the repository's: {repo}"
    );
}

/// The one line of the report that says which skill took a name, and from whom.
fn shadow<'a>(stdout: &'a str, name: &str) -> &'a str {
    stdout
        .lines()
        .find(|line| line.trim_start().starts_with("shadows") && line.contains(name))
        .unwrap_or_else(|| {
            panic!("doctor never reported `{name}` as shadowing anything:\n{stdout}")
        })
}

#[test]
fn a_shadowed_name_reports_both_sides_and_the_nearer_scope_wins() {
    let ground = Ground::new();
    ground.user_skill("greet", "the user's own hello");
    ground.repo_skill("greet", "the repository's hello");
    let path = std::env::var("PATH").expect("this machine has a PATH");

    let output = run(ground.command(&path).args(["doctor"]));

    let out = text(&output.stdout);
    a_report_was_made(&output);

    // One name, one row: the nearer scope's skill is the one in force.
    let winner = row(&out, "greet");
    assert!(
        winner.trim_start().starts_with("repo"),
        "the repository's skill stands nearest, so it wins: {winner}"
    );
    assert!(
        winner.contains("the repository's hello"),
        "and it is described in its own words: {winner}"
    );
    assert!(
        !out.contains("the user's own hello"),
        "the skill that never loads is not described as if it had:\n{out}"
    );

    // Both sides of the shadowing, and the loser by the path that never loads.
    let lost = ground
        .user_skill_path("greet")
        .canonicalize()
        .expect("the loser is a directory");
    let reported = shadow(&out, "greet");
    assert!(
        reported.contains("repo") && reported.contains("user"),
        "the line names the scope that won and the scope that lost: {reported}"
    );
    assert!(
        reported.contains(&lost.display().to_string()),
        "and the losing skill by the path a person can go and read: {reported}"
    );
}

#[test]
fn a_sandbox_check_that_fails_is_named_in_a_card_and_exits_three() {
    let ground = Ground::new();
    let path = ground.machine(RESTRICTED);

    let output = run(ground.command(&path).args(["doctor"]));

    let out = text(&output.stdout);
    let err = text(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(3),
        "§15's sandbox/setup failure, which is A1's lost clause: stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        out.contains("cannot create the sandbox: bubblewrap creates an unprivileged sandbox"),
        "the report names the check that failed, in the check's own words:\n{out}"
    );
    assert!(
        out.contains("setting up uid map: Permission denied"),
        "and carries what the check said:\n{out}"
    );
    assert!(
        out.contains("run with --yolo-unsafe"),
        "and the one thing a person can do about it:\n{out}"
    );
    assert!(
        err.is_empty(),
        "the card is the answer doctor was asked for, so it is on stdout: {err}"
    );
    assert!(
        kept_sessions(&ground).is_empty(),
        "a diagnostic that proved nothing still leaves no session behind: {:?}",
        kept_sessions(&ground)
    );
}

#[test]
fn the_proof_is_a_real_bubblewrap_invocation_rather_than_a_version_probe() {
    let ground = Ground::new();
    // A stand-in that records every argv it is handed, so what the proof ran is
    // read back rather than assumed.
    let log = ground.config.join("argv.log");
    let body = format!(
        "echo \"$@\" >> {}\n\
         if [ \"${{1:-}}\" = --version ]; then\n  \
           echo 'bubblewrap 0.12.0 (stand-in)'\n  exit 0\nfi\n\
         exit 0",
        log.display()
    );
    let path = ground.machine(&body);

    let output = run(ground.command(&path).args(["doctor"]));

    let out = text(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "stdout:\n{out}");
    assert!(
        out.contains("sandbox proved (3 checks)"),
        "the report says what was proved, check by check:\n{out}"
    );

    let seen = std::fs::read_to_string(&log).expect("the stand-in was handed an argv");
    let invocations: Vec<&str> = seen.lines().collect();
    assert!(
        invocations.contains(&"--version"),
        "one call asks the version, which is the check that it runs: {invocations:?}"
    );
    let sandbox: Vec<&&str> = invocations
        .iter()
        .filter(|args| !args.contains("--version") && args.contains("--ro-bind"))
        .collect();
    assert_eq!(
        sandbox.len(),
        1,
        "and one builds the sandbox the boundary describes: {invocations:?}"
    );
    assert!(
        sandbox[0].contains("--die-with-parent") && sandbox[0].contains("--bind"),
        "the argv is the one a command is run with, writable set and all: {}",
        sandbox[0]
    );
    assert!(
        sandbox[0].ends_with("true"),
        "and the command inside it is the smallest thing that proves it starts: {}",
        sandbox[0]
    );
    assert!(
        kept_sessions(&ground).is_empty(),
        "the temp the proof borrowed went with the session it borrowed it from: {:?}",
        kept_sessions(&ground)
    );
}

#[test]
fn a_skill_file_that_will_not_parse_is_reported_where_it_lies() {
    let ground = Ground::new();
    let broken = ground.broken_repo_skill("nameless");
    ground.repo_skill("review", "read the diff before you commit it");
    let path = std::env::var("PATH").expect("this machine has a PATH");

    let output = run(ground.command(&path).args(["doctor"]));

    let out = text(&output.stdout);
    a_report_was_made(&output);
    assert!(
        out.contains(&format!(
            "{}:1",
            broken.canonicalize().unwrap_or(broken).display()
        )),
        "the file it is in, and the line to open it at:\n{out}"
    );
    assert!(
        out.contains("no frontmatter"),
        "and what is wrong with it, in the words a person would use:\n{out}"
    );
    assert!(
        row(&out, "review").trim_start().starts_with("repo"),
        "while the skill beside it still loads, which is the point of a diagnostic:\n{out}"
    );
}

#[test]
fn a_builtin_shadowed_by_a_nearer_scope_is_named_by_the_scope_alone() {
    let ground = Ground::new();
    ground.repo_skill("commit-message", "the repository's own way with a subject");
    let path = std::env::var("PATH").expect("this machine has a PATH");

    let output = run(ground.command(&path).args(["doctor"]));

    let out = text(&output.stdout);
    a_report_was_made(&output);
    assert!(
        row(&out, "commit-message").trim_start().starts_with("repo"),
        "a repository skill may take a compiled-in name:\n{out}"
    );
    let reported = shadow(&out, "commit-message");
    assert!(
        reported.trim_end().ends_with("repo wins over builtin"),
        "a builtin has no path to name, so its scope word is the whole of it: {reported}"
    );
}

#[test]
fn an_unsafe_run_is_told_so_and_has_no_sandbox_to_prove() {
    let ground = Ground::new();
    // A machine with no bubblewrap at all: an unsafe run has no sandbox to make,
    // so finding none changes nothing about what it says.
    let empty = ground.config.join("no-bwrap");
    std::fs::create_dir_all(&empty).expect("an empty machine directory");

    let output = run(ground
        .command(&empty.display().to_string())
        .args(["--yolo-unsafe", "doctor"]));

    let out = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "there is nothing to prove and nothing to fail: stdout:\n{out}"
    );
    assert!(
        out.contains("enforcement   unsafe"),
        "the report says how the run would be confined, in §6's two-valued words:\n{out}"
    );
    assert!(
        out.contains("sandbox not proved: this run asked for no sandbox"),
        "and claims no proof it did not make:\n{out}"
    );
}
