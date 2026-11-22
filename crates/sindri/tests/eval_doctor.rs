use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::skills::Catalog;
use sindri_core::tools::{BASH, FrontEnd, ToolSet};
use tempfile::TempDir;

const PROVES: &str = "if [ \"${1:-}\" = --version ]; then\n  \
                      echo 'bubblewrap 0.12.0 (stand-in)'\n  exit 0\nfi\n\
                      exit 0";

struct Ground {
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
        ground.naming_a_run();
        ground
    }

    fn naming_a_run(&self) {
        let directory = self.config.join("sindri");
        std::fs::create_dir_all(&directory).expect("the config directory");
        std::fs::write(
            directory.join("config.toml"),
            "provider = \"anthropic\"\nmodel = \"claude-sonnet-4-5\"\n",
        )
        .expect("a config naming a run");
    }

    fn workspace(&self) -> PathBuf {
        std::fs::canonicalize(&self.workspace).expect("the workspace resolves")
    }

    fn machine(&self) -> String {
        let bin = self.config.join("machine");
        std::fs::create_dir_all(&bin).expect("a machine directory");
        let bwrap = bin.join("bwrap");
        std::fs::write(&bwrap, format!("#!/bin/sh\n{PROVES}\n")).expect("a stand-in bwrap");
        std::fs::set_permissions(&bwrap, Permissions::from_mode(0o755))
            .expect("the stand-in is executable");
        format!("{}:/usr/bin:/bin", bin.display())
    }

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
    let path = ground.machine();

    for (flags, enforcement) in [
        (Vec::<&str>::new(), Enforcement::Bubblewrap),
        (vec!["--yolo-unsafe"], Enforcement::Unsafe),
    ] {
        let output = run(ground.command(&path).args(&flags).arg("doctor"));
        let out = text(&output.stdout);
        assert_eq!(
            output.status.code(),
            Some(0),
            "every world a stand-in sandbox is handed reports whole, under {flags:?}:\n{out}",
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
            "the boundary the model is told about is the boundary enforced, under \
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
