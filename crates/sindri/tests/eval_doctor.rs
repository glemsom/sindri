//! `doctor`'s description is the `bash` tool's, byte for byte (§2, §5, §16): the
//! built binary is spawned in a workspace, a config home and a state home of the
//! test's own, the bytes it prints under the description's heading are cut out,
//! and they are held against the bytes the tool set a run registers renders.
//!
//! The two sides are built independently — doctor by running it, the expected
//! bytes by asking `ToolSet` — so a doctor that re-rendered the boundary in its own
//! words fails rather than passing by agreeing with itself. Both enforcement worlds
//! are walked, because the description is derived from the boundary in force and a
//! stale one is exactly the drift §5 forbids: a run that is sandboxed is told so,
//! and a run that is not is told that instead.
//!
//! Nothing here reads the network, and nothing here needs a terminal: the report is
//! stdout, and a host that will not let bubblewrap make a sandbox still prints it
//! whole and exits 3 over a card (§15).
//!
//! An eval is judged by `make evals` and never by `make test`, which is why the
//! test here is ignored: the stage that owns a failure is the stage that prints it.

use std::path::PathBuf;
use std::process::{Command, Output};

use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::skills::Catalog;
use sindri_core::tools::{BASH, FrontEnd, ToolSet};
use tempfile::TempDir;

/// A workspace, a config home and a state home, all the test's own.
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

    /// A child pointed at this ground and nowhere else: no inherited environment,
    /// the workspace as its working directory, and no key, because a diagnostic
    /// answers without one.
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sindri"));
        command
            .env_clear()
            .env(
                "PATH",
                std::env::var("PATH").expect("this machine has a PATH"),
            )
            .env("XDG_CONFIG_HOME", &self.config)
            .env("XDG_STATE_HOME", &self.state)
            .env("HOME", &self.config)
            .current_dir(&self.workspace);
        command
    }
}

/// A child's stream as a reader sees it.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Runs `command` to its end.
fn run(command: &mut Command) -> Output {
    command.output().expect("the binary should run")
}

/// The description doctor printed, cut out by the heading it is printed under and
/// the heading that follows it.
fn printed_description(stdout: &str) -> &str {
    let heading = format!("the description the model is given for `{BASH}`\n\n");
    let rest = stdout
        .split_once(&heading)
        .unwrap_or_else(|| panic!("doctor never printed the description:\n{stdout}"))
        .1;
    rest.split_once("\n\nskills (")
        .unwrap_or_else(|| panic!("doctor never listed the skills:\n{stdout}"))
        .0
}

/// The `bash` tool's description as a run at `enforcement` registers it, built here
/// from the boundary and the catalog doctor had: the independent half of the
/// byte-for-byte assertion, so doctor is not asked to agree with itself.
fn registered(ground: &Ground, enforcement: Enforcement) -> String {
    let boundary = Boundary::from_config(&Config::default(), &ground.workspace(), enforcement)
        .expect("a boundary over the temp workspace");
    let catalog = Catalog::discover(&ground.workspace(), Some(&ground.config));
    let tools = ToolSet::new(&boundary, &catalog, FrontEnd::Batch);
    tools
        .get(BASH)
        .expect("bash is always registered")
        .description()
        .to_owned()
}

/// The first line two descriptions disagree on, with its number and both sides, because
/// a drift a reader cannot find is a drift nobody fixes.
fn diff(printed: &str, registered: &str) -> String {
    for (at, (left, right)) in printed
        .lines()
        .zip(registered.lines())
        .enumerate()
        .map(|(at, pair)| (at + 1, pair))
    {
        if left != right {
            return format!("line {at} differs:\n  doctor:   {left:?}\n  tool set: {right:?}");
        }
    }
    format!(
        "doctor printed {} lines and the tool set renders {}",
        printed.lines().count(),
        registered.lines().count()
    )
}

#[test]
#[ignore = "an eval is judged by `make evals`, not by `make test`"]
fn doctor_prints_the_bash_description_the_model_is_given_byte_for_byte() {
    let ground = Ground::new();

    for (flags, enforcement) in [
        (Vec::<&str>::new(), Enforcement::Bubblewrap),
        (vec!["--yolo-unsafe"], Enforcement::Unsafe),
    ] {
        let output = run(ground.command().args(&flags).arg("doctor"));
        let out = text(&output.stdout);
        assert!(
            matches!(output.status.code(), Some(0) | Some(3)),
            "doctor ended {:?} instead of reporting, under {flags:?}:\n{out}",
            output.status.code()
        );
        assert!(
            output.stderr.is_empty(),
            "a diagnostic that answered wrote no chatter on stderr: {}",
            text(&output.stderr)
        );

        let printed = printed_description(&out);
        let expected = registered(&ground, enforcement);
        assert!(
            printed == expected,
            "§5: the boundary the model is told about is the boundary enforced, under \
             {flags:?}: {}\nThe description is the registered tool's own bytes and never a \
             second rendering of them.",
            diff(printed, &expected)
        );
    }

    println!(
        "\ncheck: eval doctor-description ... ok ({} bytes, both worlds)",
        registered(&ground, Enforcement::Bubblewrap).len()
    );
}
