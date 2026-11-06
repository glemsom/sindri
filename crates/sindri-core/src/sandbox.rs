//! Boot proves the sandbox once, before a turn or a record is made, so that a
//! machine which cannot confine a command is discovered at the start of a run
//! and not in the middle of one. Boot proves it once, and a card names the
//! failing check when the sandbox the boundary describes cannot be made.
//!
//! The proof is a real bubblewrap invocation and not a fact read off the
//! filesystem: the checks are the three `scripts/sandbox-probe.sh` distinguishes,
//! in its order — bubblewrap on `PATH`, bubblewrap that answers for itself, and
//! bubblewrap that creates the sandbox — and the third one runs the argv
//! [`crate::bash::invocation`] builds from the boundary in force, so what boot
//! proves is the sandbox the run's own commands will get and not a rehearsal of
//! something else. A check that fails is named in its own words, with what it
//! said, because a raw argv is a thing only this program's author can read.
//!
//! A refusal is a [`Card`]: the failing check, what it said and the one thing a
//! person can do about it, rendered as plain lines. The terminal UI's frame
//! paints those same lines as a card (§12, #36) and cannot make a different
//! statement about why the run will not start; batch mode prints them on stderr,
//! because stdout is the answer's alone, and the front end exits 3 (§15).
//!
//! There is no fallback here and nowhere else either: a sandbox that cannot be
//! made stops the run. The one route to unconfined execution is `--yolo-unsafe`
//! literally on the command line, and a run that took it has no sandbox to prove,
//! so [`prove`] passes it with an empty [`Proof`] rather than a green one.

use std::error::Error;
use std::fmt;
use std::io::ErrorKind;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::bash::{self, BWRAP};
use crate::policy::{Boundary, Enforcement};

/// §15's sandbox/setup failure, which is the code a card is reported by.
pub const EXIT_CODE: u8 = 3;

/// The command the proof runs inside the sandbox: the smallest thing that proves
/// it starts, so a failure is the sandbox's and not its work's.
const PROBE: &str = "true";

/// One of the checks a boot proof runs, in the order it runs them: the three the
/// shell probe of `scripts/sandbox-probe.sh` distinguishes, so a person reading
/// either one reads the same three names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// There is a bubblewrap to run at all.
    Installed,
    /// And it answers for itself: a binary that cannot report a version is a
    /// broken install, which is a thing someone can fix rather than an absent
    /// capability.
    Runs,
    /// And the kernel lets it create the sandbox: unprivileged user namespaces,
    /// which some kernels forbid and which no retry can supply.
    Namespaces,
}

impl Check {
    /// The check in its own words: what the card names and what a person greps
    /// for, in the shell probe and here alike.
    pub fn name(self) -> &'static str {
        match self {
            Check::Installed => "bubblewrap is on PATH",
            Check::Runs => "bubblewrap runs",
            Check::Namespaces => "bubblewrap creates an unprivileged sandbox",
        }
    }

    /// What a person can do about this check failing: the one thing the card
    /// offers, and the escape hatch named where it is the remaining route.
    pub fn remedy(self) -> &'static str {
        match self {
            Check::Installed => {
                "install bubblewrap, or run with --yolo-unsafe to run unconfined on purpose"
            }
            Check::Runs => {
                "the bwrap on PATH is not a working bubblewrap: reinstall it, or run with \
                 --yolo-unsafe to run unconfined on purpose"
            }
            Check::Namespaces => {
                "this kernel does not allow unprivileged user namespaces: enable them, or run \
                 with --yolo-unsafe to run unconfined on purpose"
            }
        }
    }
}

impl fmt::Display for Check {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// What boot proved: the checks that passed, in the order they ran, so a run can
/// say which sandbox it has rather than that it has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    /// The checks that passed. Empty for a run that asked for no sandbox.
    checks: Vec<Check>,
}

impl Proof {
    /// The checks that passed, in the order they ran.
    pub fn checks(&self) -> &[Check] {
        &self.checks
    }
}

impl fmt::Display for Proof {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.checks.is_empty() {
            return formatter.write_str("sandbox not proved: this run asked for no sandbox");
        }
        let names: Vec<&str> = self.checks.iter().map(|check| check.name()).collect();
        write!(
            formatter,
            "sandbox proved ({} checks): {}",
            self.checks.len(),
            names.join(", ")
        )
    }
}

/// The card a sandbox that cannot be created becomes: the check that failed, in
/// its own words, what it said, and the one thing a person can do about it.
///
/// It renders as plain lines — no colour, no width, no terminal — because the
/// frame (#36) paints these lines and batch mode prints them, and two surfaces
/// must not be able to say two different things about why nothing ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    /// The check that did not pass.
    check: Check,
    /// What that check said, in its own words, on one line.
    said: String,
}

impl Card {
    /// The card for a check that did not pass, carrying what it said.
    fn new(check: Check, said: impl Into<String>) -> Card {
        Card {
            check,
            said: one_line(&said.into()),
        }
    }

    /// The check that did not pass, which is the name the card reports.
    pub fn check(&self) -> Check {
        self.check
    }

    /// What that check said, as one line: empty when it said nothing.
    pub fn said(&self) -> &str {
        &self.said
    }

    /// The exit code a front end reports a card by (§15's setup failure).
    pub fn exit_code(&self) -> u8 {
        EXIT_CODE
    }
}

impl fmt::Display for Card {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot create the sandbox: {}",
            self.check.name()
        )?;
        if !self.said.is_empty() {
            write!(formatter, "\n  said:  {}", self.said)?;
        }
        write!(formatter, "\n  fix:   {}", self.check.remedy())
    }
}

impl Error for Card {}

/// Proves the sandbox `boundary` describes, with the `bwrap` this machine has on
/// `PATH`, and hands back the checks it passed.
///
/// It is called once at boot, before a turn and before a record, so a machine
/// that cannot confine a command is a card at the start of a run rather than a
/// fallback in the middle of one.
pub fn prove(boundary: &Boundary, session_temp: &Path) -> Result<Proof, Card> {
    prove_with(Path::new(BWRAP), boundary, session_temp)
}

/// Proves it with the `bwrap` at `bwrap`: the same three checks, over a program
/// named rather than looked up.
///
/// That name is the seam `scripts/sandbox-probe-test.sh` reaches through a
/// `PATH` of its own: a stand-in is a machine that refuses on demand, so all
/// three refusals are provable without a broken kernel.
pub fn prove_with(bwrap: &Path, boundary: &Boundary, session_temp: &Path) -> Result<Proof, Card> {
    // The escape hatch has no sandbox, so there is nothing to prove and nothing
    // to report green: an empty proof says exactly that.
    if boundary.enforcement() == Enforcement::Unsafe {
        return Ok(Proof { checks: Vec::new() });
    }

    // The first two checks are one spawn: whether there is a bubblewrap, and
    // whether the one there answers for itself. Which of the two failed is what
    // the error kind says, so the card names the check and not the symptom.
    let spoke = match ask(bwrap, &[String::from("--version")]) {
        Ok(spoken) => spoken,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(Card::new(Check::Installed, error.to_string()));
        }
        Err(error) => return Err(Card::new(Check::Runs, error.to_string())),
    };
    let mut checks = vec![Check::Installed];
    if !spoke.status.success() {
        return Err(Card::new(Check::Runs, said_of(&spoke)));
    }
    checks.push(Check::Runs);

    // The third check is the sandbox itself, built by the one function that
    // turns a boundary into an argv: a boundary that cannot even be rendered
    // never reaches the check that would have run it, and the card says why.
    let invocation = bash::invocation(boundary, session_temp, PROBE)
        .map_err(|error| Card::new(Check::Namespaces, error.to_string()))?;
    let made = match ask(bwrap, invocation.args()) {
        Ok(made) => made,
        Err(error) => return Err(Card::new(Check::Namespaces, error.to_string())),
    };
    if !made.status.success() {
        return Err(Card::new(Check::Namespaces, said_of(&made)));
    }
    checks.push(Check::Namespaces);

    Ok(Proof { checks })
}

/// Runs `program` with `args`, capturing what it said: no stdin, because a check
/// has nothing to type at it, and both streams piped, because a refusal's words
/// are the card's.
fn ask(program: &Path, args: &[String]) -> std::io::Result<Output> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
}

/// What a failed check said, as one line: its stderr when it wrote one, its
/// stdout when it did not, and the status when it wrote neither.
fn said_of(output: &Output) -> String {
    let (stderr, stdout) = (
        String::from_utf8_lossy(&output.stderr).into_owned(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    );
    let spoken = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    let words = one_line(&spoken);
    if !words.is_empty() {
        return words;
    }
    match output.status.code() {
        Some(code) => format!("it exited {code} and said nothing"),
        None => String::from("a signal killed it and it said nothing"),
    }
}

/// `words` on one line: trimmed, and every run of whitespace between words
/// collapsed to one space, so a card's second line stays a line.
fn one_line(words: &str) -> String {
    words.split_whitespace().collect::<Vec<&str>>().join(" ")
}

#[cfg(test)]
mod tests {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;
    use crate::config::Config;

    /// A stand-in `bwrap` on disk: the machine a test hands the proof instead of
    /// this one's own, so every refusal is provable where the real bwrap works.
    ///
    /// It answers `--version` as its `version` branch says, writes the argv it
    /// was handed for the sandbox check to `spoken`, and then does what its
    /// `creating` branch says. Shell, because a stand-in is a process too.
    struct Machine {
        /// Held so the script outlives the proof that runs it.
        _root: TempDir,
        /// The stand-in itself, executable.
        bwrap: PathBuf,
        /// Where the sandbox check's argv is kept.
        spoken: PathBuf,
    }

    impl Machine {
        /// A machine whose `bwrap` is `creating` after answering `--version` with
        /// `version`.
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

        /// A machine whose bubblewrap makes the sandbox it was asked for.
        fn working() -> Machine {
            Machine::new("echo 'bubblewrap 0.12.0 (stand-in)'\nexit 0", "exit 0")
        }

        /// A machine whose kernel will not let bubblewrap unshare: the shape a
        /// hosted runner has, with AppArmor forbidding user namespaces.
        fn restricted() -> Machine {
            Machine::new(
                "echo 'bubblewrap 0.12.0 (stand-in)'\nexit 0",
                "echo 'bwrap: setting up uid map: Permission denied' >&2\nexit 1",
            )
        }

        /// A machine whose bwrap cannot report its own version: a broken install
        /// rather than an absent capability.
        fn broken() -> Machine {
            Machine::new(
                "echo 'bwrap: cannot execute: Exec format error' >&2\nexit 1",
                "exit 0",
            )
        }

        /// The argv the stand-in was handed for the sandbox check, word by word.
        fn handed(&self) -> Vec<String> {
            std::fs::read_to_string(&self.spoken)
                .expect("the stand-in kept the argv it was handed")
                .lines()
                .map(String::from)
                .collect()
        }
    }

    /// The ground a boundary stands on: a workspace and a session temp, both real
    /// directories, because the writable set is canonicalised through the
    /// filesystem and never tidied as a string.
    struct Ground {
        /// Held so the directories outlive the boundary that names them.
        _root: TempDir,
        /// The workspace, canonical.
        workspace: PathBuf,
        /// The session temp, canonical.
        temp: PathBuf,
    }

    impl Ground {
        /// An empty workspace and an empty temp.
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

        /// The boundary a run holds on this ground, confined as `enforcement` says.
        fn boundary(&self, enforcement: Enforcement) -> Boundary {
            Boundary::from_config(&Config::default(), &self.workspace, enforcement)
                .expect("the boundary holds")
        }
    }

    #[test]
    fn the_proof_runs_the_three_checks_of_the_shell_probe_in_order() {
        let machine = Machine::working();
        let ground = Ground::new();

        let proof = prove_with(
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

        prove_with(
            &machine.bwrap,
            &ground.boundary(Enforcement::Bubblewrap),
            &ground.temp,
        )
        .expect("a machine that can make a sandbox proves it");

        // The proof is the sandbox the run's own commands will get: every
        // namespace unshared, the root read-only, the workspace bound over
        // itself, the session temp mapped to TMPDIR, and the probe as the
        // command.
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

        let card = prove_with(
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

        let card = prove_with(
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

        let card = prove_with(
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

        let card = prove_with(
            &machine.bwrap,
            &ground.boundary(Enforcement::Bubblewrap),
            &ground.temp,
        )
        .expect_err("this machine cannot make the sandbox");

        // Written out as a reader meets it: the failing check, what it said and
        // what can be done, on three plain lines and no terminal anywhere.
        assert_eq!(
            card.to_string(),
            "cannot create the sandbox: bubblewrap creates an unprivileged sandbox\n  \
             said:  bwrap: setting up uid map: Permission denied\n  \
             fix:   this kernel does not allow unprivileged user namespaces: enable them, or \
             run with --yolo-unsafe to run unconfined on purpose"
        );
        assert_eq!(card.exit_code(), 3, "§15's sandbox/setup failure");
    }

    #[test]
    fn a_check_that_exits_silently_still_gets_a_line_about_what_it_said() {
        let machine = Machine::new("echo 'bubblewrap 0.12.0 (stand-in)'\nexit 0", "exit 7");
        let ground = Ground::new();

        let card = prove_with(
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
        // A bwrap that refuses everything, so a proof that ran anything at all
        // would hand back a card instead of a pass.
        let machine = Machine::restricted();

        let proof = prove_with(
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
}
