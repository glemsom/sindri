//! What boot does when the sandbox cannot be made, measured where a script
//! measures it: the built binary is spawned with a stand-in `bwrap` first on
//! `PATH`, and its stderr, its stdout and its exit code are read back.
//!
//! A stand-in is a machine that refuses on demand — the way
//! `scripts/sandbox-probe-test.sh` fakes one — so all three failures §6 can meet
//! are provable here without a broken kernel: no bubblewrap at all, a bubblewrap
//! that cannot report its version, and a kernel that forbids unprivileged user
//! namespaces. Every child gets `env_clear`, so nothing of this process's
//! environment reaches the binary except the variables a test names.
//!
//! The last test reads the tree instead of running it: it holds the sentence
//! that only the command line can reach `unsafe`, by naming the one function in
//! the front end that may write that variant.

use std::fs::Permissions;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

/// A workspace, a config home and a state home, all of the test's own, and the
/// directory a stand-in `bwrap` is written into.
struct Ground {
    /// Held so the directories outlive the paths that name them.
    _root: TempDir,
    /// The one directory the run stands in.
    workspace: PathBuf,
    /// The config home, which holds no file unless a test writes one.
    config: PathBuf,
    /// The state home the session directory is made under.
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

    /// A machine with no bubblewrap anywhere: a `PATH` of one empty directory.
    fn without_bwrap(&self) -> String {
        let empty = self.config.join("no-bwrap");
        std::fs::create_dir_all(&empty).expect("an empty machine directory");
        empty.display().to_string()
    }

    /// A child pointed at this ground and nowhere else: no inherited environment,
    /// the workspace as its working directory, and everything a run needs to get
    /// as far as boot — a wire nothing answers on, so a run that gets past boot
    /// fails at its first ask rather than hanging.
    fn command(&self, path: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sindri"));
        command
            .env_clear()
            .env("PATH", path)
            .env("XDG_CONFIG_HOME", &self.config)
            .env("XDG_STATE_HOME", &self.state)
            .env("HOME", &self.config)
            .current_dir(&self.workspace)
            .env("SINDRI_PROVIDER", "anthropic")
            .env("SINDRI_MODEL", "claude-sonnet-4-6")
            .env("SINDRI_BASE_URL", closed_wire())
            .env("ANTHROPIC_API_KEY", "sk-ant-the-stand-in-one");
        command
    }

    /// Whether any session under the state home holds a record: a run that boot
    /// refused wrote none, and the session directory — the ground its temp is
    /// mapped from, which boot needs before it can prove anything — may stand.
    fn recorded(&self) -> bool {
        let sessions = self.state.join("sindri").join("sessions");
        let Ok(entries) = std::fs::read_dir(sessions) else {
            return false;
        };
        entries
            .filter_map(Result::ok)
            .any(|entry| entry.path().join("session.json").exists())
    }

    /// The one file a config home holds, written as a test words it.
    fn write_config(&self, text: &str) {
        let sindri = self.config.join("sindri");
        std::fs::create_dir_all(&sindri).expect("a config directory");
        std::fs::write(sindri.join("config.toml"), text).expect("a config file");
    }
}

/// A URL nothing answers on: a loopback port that was bound and let go, so a run
/// that reaches the wire is refused at once instead of waiting out a timeout.
fn closed_wire() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// A bwrap that reports a version and whose kernel will not let it unshare: the
/// machine a hosted runner is.
const RESTRICTED: &str = "if [ \"${1:-}\" = --version ]; then\n  \
                          echo 'bubblewrap 0.12.0 (stand-in)'\n  exit 0\nfi\n\
                          echo 'bwrap: setting up uid map: Permission denied' >&2\nexit 1";

/// A bwrap that cannot report its own version: a broken install.
const BROKEN: &str = "echo 'bwrap: cannot execute: Exec format error' >&2\nexit 1";

/// A stream as a reader sees it.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Runs `command` to its end.
fn run(command: &mut Command) -> Output {
    command.output().expect("the binary should run")
}

#[test]
fn a_sandbox_that_cannot_be_created_is_a_card_on_stderr_and_exit_three() {
    let ground = Ground::new();
    let path = ground.machine(RESTRICTED);

    let output = run(ground.command(&path).args(["-b", "triage this"]));

    let err = text(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(3),
        "§15's sandbox/setup failure: {err}"
    );
    assert!(
        err.contains("cannot create the sandbox: bubblewrap creates an unprivileged sandbox"),
        "the card names the failing check: {err}"
    );
    assert!(
        err.contains("setting up uid map: Permission denied"),
        "and carries what the check said: {err}"
    );
    assert!(
        err.contains("run with --yolo-unsafe"),
        "and the one thing that would run anyway: {err}"
    );
    assert!(
        output.stdout.is_empty(),
        "stdout is the answer's alone, and a run that never started has none: {}",
        text(&output.stdout)
    );
    assert!(
        !ground.recorded(),
        "boot refuses before a run is recorded: the card is the only thing left of it"
    );
}

#[test]
fn a_machine_with_no_bubblewrap_gets_a_card_naming_the_missing_check() {
    let ground = Ground::new();
    let path = ground.without_bwrap();

    let output = run(ground.command(&path).args(["-b", "triage this"]));

    let err = text(&output.stderr);
    assert_eq!(output.status.code(), Some(3), "{err}");
    assert!(
        err.contains("cannot create the sandbox: bubblewrap is on PATH"),
        "the card names the check that found nothing: {err}"
    );
    assert!(output.stdout.is_empty(), "{}", text(&output.stdout));
}

#[test]
fn a_broken_bwrap_install_is_named_rather_than_excused() {
    let ground = Ground::new();
    let path = ground.machine(BROKEN);

    let output = run(ground.command(&path).args(["-b", "triage this"]));

    let err = text(&output.stderr);
    assert_eq!(output.status.code(), Some(3), "{err}");
    assert!(
        err.contains("cannot create the sandbox: bubblewrap runs"),
        "a bwrap on PATH that cannot answer is a failure, not an excuse: {err}"
    );
    assert!(output.stdout.is_empty(), "{}", text(&output.stdout));
}

#[test]
fn yolo_unsafe_is_the_one_route_past_a_sandbox_that_cannot_be_created() {
    let ground = Ground::new();
    let path = ground.machine(RESTRICTED);

    let output = run(ground
        .command(&path)
        .args(["-b", "triage this", "--yolo-unsafe", "-v"]));

    let err = text(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "the escape hatch gets past boot to the wire, which is what fails here: {err}"
    );
    assert!(
        !err.contains("cannot create the sandbox"),
        "nothing is printed that did not happen: {err}"
    );
    assert!(
        err.contains("enforcement unsafe"),
        "and the run the boundary was built for is the unsafe one: {err}"
    );
    assert!(
        output.stdout.is_empty(),
        "an unsafe run that failed still answers nothing: {}",
        text(&output.stdout)
    );
}

#[test]
fn an_environment_variable_naming_the_escape_hatch_changes_nothing() {
    let ground = Ground::new();
    let path = ground.machine(RESTRICTED);

    let output = run(ground
        .command(&path)
        .env("SINDRI_YOLO_UNSAFE", "true")
        .env("YOLO_UNSAFE", "true")
        .args(["-b", "triage this"]));

    let err = text(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(3),
        "the escape hatch has no environment variable (§6): {err}"
    );
    assert!(
        err.contains("cannot create the sandbox"),
        "so the card is printed, as it is for any run that asked for nothing: {err}"
    );
}

#[test]
fn a_yolo_key_in_the_config_is_refused_rather_than_honoured() {
    let ground = Ground::new();
    ground.write_config("yolo_unsafe = true\n");
    let path = ground.without_bwrap();

    let output = run(ground.command(&path).args(["-b", "triage this"]));

    let err = text(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "a persisted escape hatch is a usage error, not a setting: {err}"
    );
    assert!(
        err.contains("yolo_unsafe"),
        "the refusal names the key that cannot be written: {err}"
    );
}

#[test]
fn a_machine_that_can_make_a_sandbox_proves_it_before_the_first_turn() {
    let ground = Ground::new();
    // This machine's own PATH, so the real bwrap is the one boot proves with.
    let path = std::env::var("PATH").expect("this machine has a PATH");

    let output = run(ground.command(&path).args(["-b", "triage this", "-v"]));

    let err = text(&output.stderr);
    assert!(
        output.stdout.is_empty(),
        "no wire answers here, so there is no answer: {}",
        text(&output.stdout)
    );
    match output.status.code() {
        // The proof passed and the run went on to a wire that is not there.
        Some(1) => assert!(
            err.contains("sandbox proved (3 checks)"),
            "boot proved the sandbox the run then used: {err}"
        ),
        // The proof ran and the kernel refused it: a host without unprivileged
        // user namespaces is a card that says so, never a run that pretends.
        Some(3) => assert!(
            err.contains("cannot create the sandbox: bubblewrap creates an unprivileged sandbox"),
            "a machine that forbids the sandbox is named as the one that did: {err}"
        ),
        code => panic!("boot ended with {code:?}, which is neither a proof nor its card: {err}"),
    }
}

/// `Enforcement::Unsafe` in the two crates' sources, as the file it is written in
/// and the function that writes it.
fn unsafe_sites() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found: Vec<(String, String)> = Vec::new();
    for crate_dir in ["crates/sindri-core/src", "crates/sindri/src"] {
        let mut queue = vec![root.join(crate_dir)];
        while let Some(directory) = queue.pop() {
            for entry in std::fs::read_dir(&directory).expect("a source directory") {
                let path = entry.expect("a source file").path();
                if path.is_dir() {
                    queue.push(path);
                    continue;
                }
                if path.extension().and_then(|kind| kind.to_str()) != Some("rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("a source file");
                let lines: Vec<&str> = text.lines().collect();
                let file = path
                    .strip_prefix(&root)
                    .expect("a file under the repository root")
                    .display()
                    .to_string();
                for (index, line) in lines.iter().enumerate() {
                    if line.contains("Enforcement::Unsafe") {
                        found.push((file.clone(), enclosing(&lines, index)));
                    }
                }
            }
        }
    }
    found
}

/// The name of the `fn` a line sits in, found by walking back to the nearest
/// declaration above it.
fn enclosing(lines: &[&str], at: usize) -> String {
    lines[..=at]
        .iter()
        .rev()
        .find_map(|line| {
            let (_, rest) = line.split_once("fn ")?;
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
        .unwrap_or_default()
}

#[test]
fn enforcement_becomes_unsafe_only_where_the_command_line_flag_is_read() {
    let sites = unsafe_sites();
    assert!(
        sites
            .iter()
            .any(|(file, _)| file == "crates/sindri/src/cli.rs"),
        "the scan reads the tree it means to: {sites:?}"
    );

    // In the front end, one function may write the variant, and it is the one
    // that reads `--yolo-unsafe` off this invocation: every other route to it
    // would be a fallback nobody asked for.
    let front_end: Vec<&(String, String)> = sites
        .iter()
        .filter(|(file, _)| file.starts_with("crates/sindri/src"))
        .collect();
    assert_eq!(
        front_end.len(),
        1,
        "one place in the front end reaches unsafe: {front_end:?}"
    );
    assert_eq!(
        (front_end[0].0.as_str(), front_end[0].1.as_str()),
        ("crates/sindri/src/cli.rs", "enforcement"),
        "and it is the flag's own constructor"
    );

    // In the core, the variant is read in the three modules that hold the fact —
    // the one that defines it, the one that enforces it and the one that proves
    // it — and in no module that could arrive at it from a file or a variable.
    let core: Vec<&(String, String)> = sites
        .iter()
        .filter(|(file, _)| file.starts_with("crates/sindri-core/src"))
        .collect();
    for (file, _) in &core {
        assert!(
            matches!(
                file.as_str(),
                "crates/sindri-core/src/policy.rs"
                    | "crates/sindri-core/src/bash.rs"
                    | "crates/sindri-core/src/sandbox.rs"
            ),
            "the core reads the fact only where it defines, enforces and proves it, \
             never in {file} where a config could reach it"
        );
    }
    assert!(
        core.iter()
            .any(|(file, _)| file == "crates/sindri-core/src/policy.rs"),
        "the variant is defined in the policy it describes: {sites:?}"
    );
}
