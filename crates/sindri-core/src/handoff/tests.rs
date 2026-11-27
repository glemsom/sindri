use std::collections::BTreeMap;
use std::path::PathBuf;

use tempfile::TempDir;

use super::{ChildLaunch, Handoff};
use crate::config::Config;
use crate::policy::{Boundary, Enforcement};

const SOCKET: &str = "/run/user/1000/gcr/ssh";

struct Ground {
    workspace: TempDir,
    temp: TempDir,
}

impl Ground {
    fn new() -> Ground {
        Ground {
            workspace: tempfile::tempdir().expect("a workspace"),
            temp: tempfile::tempdir().expect("a session temp"),
        }
    }

    fn command(&self, config: &Config, agent: Option<&str>) -> String {
        let mut environment = BTreeMap::new();
        environment.insert(String::from("HOME"), String::from("/home/tester"));
        if let Some(socket) = agent {
            environment.insert(String::from("SSH_AUTH_SOCK"), String::from(socket));
        }
        let boundary =
            Boundary::from_config(config, self.workspace.path(), Enforcement::Bubblewrap)
                .expect("the workspace is a boundary");
        let transport = Handoff::write(
            config,
            &boundary,
            self.workspace.path(),
            self.temp.path(),
            &environment,
        )
        .expect("a handoff is written");
        ChildLaunch {
            executable: PathBuf::from("/usr/bin/sindri"),
            transport,
        }
        .command(self.temp.path(), "true")
    }
}

#[test]
fn a_launch_nobody_asked_for_an_agent_names_none() {
    let ground = Ground::new();
    let untouched = Config::default();
    let refused = Config {
        ssh_agent: Some(false),
        ..Config::default()
    };
    for config in [untouched, refused] {
        let command = ground.command(&config, Some(SOCKET));
        assert!(
            !command.contains("SSH_AUTH_SOCK"),
            "a command keeps no agent it was not given: {command}"
        );
    }
}

#[test]
fn a_launch_that_asks_for_an_agent_names_the_socket_that_is_running() {
    let ground = Ground::new();
    let config = Config {
        ssh_agent: Some(true),
        ..Config::default()
    };
    let command = ground.command(&config, Some(SOCKET));
    assert!(
        command.contains("SSH_AUTH_SOCK='/run/user/1000/gcr/ssh'"),
        "a command joins the conversation with the socket outside: {command}"
    );
}

#[test]
fn a_launch_that_asks_for_an_agent_that_is_not_running_names_none() {
    let ground = Ground::new();
    let config = Config {
        ssh_agent: Some(true),
        ..Config::default()
    };
    let command = ground.command(&config, None);
    assert!(
        !command.contains("SSH_AUTH_SOCK"),
        "there is no socket to name, so the command is handed no empty one: {command}"
    );
}
