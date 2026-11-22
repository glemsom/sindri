use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use tempfile::TempDir;

use super::*;
use crate::config::Config;
use std::time::{Duration, Instant};

/// `ETXTBSY` is what the kernel answers when a file is open for writing anywhere at the
/// moment it is executed, and every stand-in here is a file this process wrote a moment
/// earlier: so a busy one is asked again, and one that is really broken still fails.
fn proved(bwrap: &Path, boundary: &Boundary, session_temp: &Path) -> Result<Proof, Card> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let outcome = prove_with(bwrap, boundary, session_temp);
        let busy = outcome
            .as_ref()
            .err()
            .is_some_and(|card| card.said().contains("Text file busy"));
        if !busy || Instant::now() > deadline {
            return outcome;
        }
    }
}

struct Machine {
    _root: TempDir,
    bwrap: PathBuf,
    spoken: PathBuf,
}

impl Machine {
    fn new(version: &str, creating: &str) -> Machine {
        let root = tempfile::tempdir().expect("a temp machine");
        let bwrap = root.path().join("bwrap");
        let spoken = root.path().join("spoken-argv");
        let script = format!(
            "#!/bin/sh\nif [ \"${{1:-}}\" = --version ]; then\n{version}\nfi\n\
                 printf '%s\\n' \"$@\" > '{spoken}'\n{creating}\n",
            spoken = spoken.display()
        );
        std::fs::write(&bwrap, script).expect("a stand-in bwrap");
        std::fs::set_permissions(&bwrap, Permissions::from_mode(0o755))
            .expect("the stand-in is executable");
        Machine {
            _root: root,
            bwrap,
            spoken,
        }
    }

    fn working() -> Machine {
        Machine::new("echo 'bubblewrap 0.12.0 (stand-in)'\nexit 0", "exit 0")
    }

    fn restricted() -> Machine {
        Machine::new(
            "echo 'bubblewrap 0.12.0 (stand-in)'\nexit 0",
            "echo 'bwrap: setting up uid map: Permission denied' >&2\nexit 1",
        )
    }

    fn broken() -> Machine {
        Machine::new(
            "echo 'bwrap: cannot execute: Exec format error' >&2\nexit 1",
            "exit 0",
        )
    }

    fn handed(&self) -> Vec<String> {
        std::fs::read_to_string(&self.spoken)
            .expect("the stand-in kept the argv it was handed")
            .lines()
            .map(String::from)
            .collect()
    }
}

struct Ground {
    _root: TempDir,
    workspace: PathBuf,
    temp: PathBuf,
}

impl Ground {
    fn new() -> Ground {
        let root = tempfile::tempdir().expect("a temp ground");
        let workspace = root.path().join("workspace");
        let temp = root.path().join("tmp");
        for path in [&workspace, &temp] {
            std::fs::create_dir_all(path).expect("a directory");
        }
        Ground {
            workspace: std::fs::canonicalize(workspace).expect("the workspace resolves"),
            temp: std::fs::canonicalize(temp).expect("the temp resolves"),
            _root: root,
        }
    }

    fn boundary(&self, enforcement: Enforcement) -> Boundary {
        Boundary::from_config(&Config::default(), &self.workspace, enforcement)
            .expect("the boundary holds")
    }
}

#[test]
fn the_proof_runs_the_three_checks_of_the_shell_probe_in_order() {
    let machine = Machine::working();
    let ground = Ground::new();

    let proof = proved(
        &machine.bwrap,
        &ground.boundary(Enforcement::Bubblewrap),
        &ground.temp,
    )
    .expect("a machine that can make a sandbox proves it");

    assert_eq!(
        proof.checks(),
        &[Check::Installed, Check::Runs, Check::Namespaces],
        "the checks are the shell probe's three, in its order"
    );
    assert_eq!(
        proof.to_string(),
        "sandbox proved (3 checks): bubblewrap is on PATH, bubblewrap runs, \
             bubblewrap creates an unprivileged sandbox"
    );
}

#[test]
fn the_sandbox_check_runs_the_argv_the_boundary_describes() {
    let machine = Machine::working();
    let ground = Ground::new();
    let workspace = ground.workspace.display().to_string();
    let temp = ground.temp.display().to_string();

    proved(
        &machine.bwrap,
        &ground.boundary(Enforcement::Bubblewrap),
        &ground.temp,
    )
    .expect("a machine that can make a sandbox proves it");

    let handed = machine.handed();
    let flags: Vec<&str> = handed.iter().map(String::as_str).collect();
    for flag in [
        "--unshare-all",
        "--share-net",
        "--die-with-parent",
        "--ro-bind",
    ] {
        assert!(
            flags.contains(&flag),
            "the proof runs the boundary's own argv, and {flag} is missing from {flags:?}"
        );
    }
    assert!(
        flags
            .windows(3)
            .any(|three| three == ["--bind", &workspace, &workspace]),
        "the workspace is bound over itself where the model was told it is: {flags:?}"
    );
    assert!(
        flags.windows(2).any(|two| two == ["--bind", &temp]),
        "the session temp is mapped so a command has scratch: {flags:?}"
    );
    assert_eq!(
        flags.last().copied(),
        Some(PROBE),
        "the check's command is the smallest one that proves the sandbox starts"
    );
}

#[test]
fn a_machine_with_no_bubblewrap_names_the_check_that_is_missing() {
    let ground = Ground::new();
    let absent = ground.workspace.join("bin").join("bwrap");

    let card = proved(
        &absent,
        &ground.boundary(Enforcement::Bubblewrap),
        &ground.temp,
    )
    .expect_err("there is no bubblewrap to prove anything with");

    assert_eq!(card.check(), Check::Installed);
    assert!(
        card.to_string().contains("bubblewrap is on PATH"),
        "the card names the missing check in its own words: {card}"
    );
}

#[test]
fn a_bubblewrap_that_cannot_report_a_version_is_a_broken_install() {
    let machine = Machine::broken();
    let ground = Ground::new();

    let card = proved(
        &machine.bwrap,
        &ground.boundary(Enforcement::Bubblewrap),
        &ground.temp,
    )
    .expect_err("a bwrap that cannot report a version is not usable");

    assert_eq!(card.check(), Check::Runs);
    assert!(
        card.to_string().contains("Exec format error"),
        "the card carries what the check said: {card}"
    );
}

#[test]
fn a_kernel_that_forbids_user_namespaces_names_the_check_it_forbade() {
    let machine = Machine::restricted();
    let ground = Ground::new();

    let card = proved(
        &machine.bwrap,
        &ground.boundary(Enforcement::Bubblewrap),
        &ground.temp,
    )
    .expect_err("a kernel that forbids unprivileged user namespaces makes no sandbox");

    assert_eq!(card.check(), Check::Namespaces);
    assert!(
        card.said()
            .contains("setting up uid map: Permission denied"),
        "the card carries bwrap's own words: {card}"
    );
}

#[test]
fn the_card_is_plain_lines_a_frame_will_paint() {
    let machine = Machine::restricted();
    let ground = Ground::new();

    let card = proved(
        &machine.bwrap,
        &ground.boundary(Enforcement::Bubblewrap),
        &ground.temp,
    )
    .expect_err("this machine cannot make the sandbox");

    assert_eq!(
        card.to_string(),
        "cannot create the sandbox: bubblewrap creates an unprivileged sandbox\n  \
             said:  bwrap: setting up uid map: Permission denied\n  \
             fix:   this kernel does not allow unprivileged user namespaces: enable them, or \
             run with --yolo-unsafe to run unconfined on purpose"
    );
    assert_eq!(card.exit_code(), 3, "the sandbox/setup failure");
}

#[test]
fn a_check_that_exits_silently_still_gets_a_line_about_what_it_said() {
    let machine = Machine::new("echo 'bubblewrap 0.12.0 (stand-in)'\nexit 0", "exit 7");
    let ground = Ground::new();

    let card = proved(
        &machine.bwrap,
        &ground.boundary(Enforcement::Bubblewrap),
        &ground.temp,
    )
    .expect_err("a sandbox that exits 7 is not a sandbox");

    assert_eq!(card.check(), Check::Namespaces);
    assert_eq!(card.said(), "it exited 7 and said nothing");
}

#[test]
fn an_unsafe_run_has_no_sandbox_to_prove() {
    let ground = Ground::new();
    let machine = Machine::restricted();

    let proof = proved(
        &machine.bwrap,
        &ground.boundary(Enforcement::Unsafe),
        &ground.temp,
    )
    .expect("an unsafe run has no sandbox to prove");

    assert!(
        proof.checks().is_empty(),
        "and nothing is reported green that was not proved: {proof}"
    );
    assert!(
        proof.to_string().contains("asked for no sandbox"),
        "the proof says so rather than claiming one: {proof}"
    );
}
