//! `doctor`'s description is the `bash` tool's, byte for byte: the bytes printed under its heading are cut out
//! of the report and held against what the tool set a run registers renders, both enforcement worlds walked.
//! An eval is judged by `make evals` and never by `make test`, which is why the test here is ignored.

use std::path::PathBuf;
use std::process::{Command, Output};

use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::skills::Catalog;
use sindri_core::tools::{BASH, FrontEnd, ToolSet};
use tempfile::TempDir;

struct Ground {
    /// Held so the directories outlive the paths that name them.
    _root: TempDir,
    workspace: PathBuf,
    config: PathBuf,
    state: PathBuf,
}

impl Ground {
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

    /// The workspace as the child's `getcwd` spells it: the boundary the description is rendered from is built
    /// from that path.
    fn workspace(&self) -> PathBuf {
        std::fs::canonicalize(&self.workspace).expect("the workspace resolves")
    }

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

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn run(command: &mut Command) -> Output {
    command.output().expect("the binary should run")
}

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

/// The `bash` tool's description as a run at `enforcement` registers it, built here from the boundary and
/// the catalog doctor had: the independent half of the byte-for-byte assertion.
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
