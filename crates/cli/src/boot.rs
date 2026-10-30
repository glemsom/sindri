//! Boot: the one module that turns the flags and the process's own facts into a
//! run ready to start (ADR-0015). A front end calls one door and keeps only what
//! is its own — the frame proves the sandbox and draws the card, batch mode
//! prints the answer, `sandbox exec` prints the outcome. Boot probes nothing,
//! owns no cancel flag, and says exactly one thing on the way in: a repository
//! config it refused (ADR-0005).

use sindri_core::config::{Config, ConfigError, Overrides};
use sindri_core::session::{Env, Session};

use crate::failure::Failure;

/// A run ready to start: the environment it was opened in, the config it is
/// bounded by, and the session recording it.
pub struct Boot {
    /// The environment the session was opened in. It comes back out so `/new`
    /// opens the next session in the same workspace, `PATH` and state root
    /// without reading the process a second time (ADR-0015 §6).
    pub env: Env,
    pub config: Config,
    pub session: Session,
}

/// What boot has read before anything exists on disk: the environment and the
/// config. A front end with something to prove first — the frame proves the
/// sandbox before it starts anything (ADR-0011 §15) — starts the session itself
/// from here.
pub struct Configured {
    pub env: Env,
    pub config: Config,
}

impl Configured {
    /// Opens the session: the one place a run is started, in the one order
    /// (ADR-0015 §1). Nothing is read from the process here.
    pub fn start(self) -> Result<Boot, Failure> {
        let session = Session::open(&self.env, &self.config).map_err(Failure::Start)?;
        Ok(Boot {
            env: self.env,
            config: self.config,
            session,
        })
    }
}

/// Starts a run that asks a model: the read a run needs, and its session.
pub fn open(overrides: Overrides) -> Result<Boot, Failure> {
    prepare(overrides)?.start()
}

/// The read a run that asks a model needs — a model and a credential. The frame
/// proves the sandbox on this before anything is created on disk.
pub fn prepare(overrides: Overrides) -> Result<Configured, Failure> {
    read(overrides, Config::resolve)
}

/// The diagnostic door: the same read, without a model or a credential, so
/// `sandbox exec` and `doctor` probe and describe the sandbox a run would get
/// without being configured as one (ADR-0015 §7).
pub fn configure(overrides: Overrides) -> Result<Configured, Failure> {
    read(overrides, Config::for_command)
}

/// The read both doors do: the process's own facts once, and the settings one
/// of the two resolutions answers. The environment is read first, so a machine
/// whose state root cannot be found is the environment's failure before a config
/// value is blamed for it. A config that was refused is said here, once, because
/// this is where every cold path resolves one.
fn read(
    overrides: Overrides,
    resolve: fn(Overrides) -> Result<Config, ConfigError>,
) -> Result<Configured, Failure> {
    let env = Env::ambient().map_err(Failure::Environment)?;
    let config = resolve(overrides).map_err(Failure::Config)?;
    announce(&config);
    Ok(Configured { env, config })
}

/// The one note a refused repository config earns, said the same way by every
/// front end that resolves a config before it starts: a directory that silently
/// ignores the file it was given would leave a user editing a config nobody
/// reads (ADR-0005). The frame's mid-session save is deliberately not a caller —
/// it runs while the frame owns the screen, where a line on stderr is corruption
/// (ADR-0011 §15). It keeps a config failure as a value and says that in the
/// transcript, and the refusal it re-reads is not said again: the launch said it
/// once, and a save changes nothing about a file nobody reads.
fn announce(config: &Config) {
    if let Some(path) = &config.refused_repo_config {
        eprintln!(
            "sindri: ignoring {}: a config committed in a repository is never honoured (ADR-0005)",
            path.display()
        );
    }
}

/// `/new`: the next session in the same run environment, from the values the
/// launch already holds — the process is not read again (ADR-0015 §6).
pub fn renew(boot: &Boot) -> Result<Session, Failure> {
    Session::open(&boot.env, &boot.config).map_err(Failure::Start)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        reason = "a panicking test is how a broken seam is reported"
    )]

    use std::time::Duration;

    use sindri_core::config::Provider;
    use tempfile::TempDir;

    use super::*;

    /// A config with every value written out: no file and no variable of the
    /// host may reach a test's session.
    fn config() -> Config {
        Config {
            provider: Provider::Anthropic,
            model: "model-one".to_string(),
            api_key: String::new(),
            base_url: None,
            max_turns: 3,
            timeout: Duration::from_secs(5),
            thinking: false,
            extra_writable: Vec::new(),
            extra_headers: Vec::new(),
            refused_repo_config: None,
        }
    }

    /// A run environment under temporary directories: a workspace to work in,
    /// and a state root that is emphatically not the process's own.
    fn env(root: &TempDir) -> Env {
        let workspace = root.path().join("workspace");
        let state = root.path().join("state");
        for dir in [&workspace, &state] {
            std::fs::create_dir_all(dir).expect("a temporary directory");
        }
        Env {
            workspace: workspace.canonicalize().expect("canonical workspace"),
            search_path: "/usr/bin:/bin".to_string(),
            state,
        }
    }

    #[test]
    fn the_session_a_start_opens_is_the_one_the_read_named() {
        let root = TempDir::new().expect("temp root");
        let configured = Configured {
            env: env(&root),
            config: config(),
        };
        let state = configured.env.state.clone();
        let workspace = configured.env.workspace.clone();

        let boot = configured
            .start()
            .expect("a session in the read environment");

        assert!(
            boot.session.dir().root().starts_with(&state),
            "the session is created in the state root boot read: {}",
            boot.session.dir().root().display()
        );
        assert!(
            boot.session
                .policy()
                .writable()
                .paths()
                .contains(&workspace),
            "and the policy is derived from the workspace boot read: {:?}",
            boot.session.policy().writable().paths()
        );
        assert_eq!(
            boot.env.workspace, workspace,
            "the environment comes back out of boot, unchanged"
        );
    }

    #[test]
    fn renew_opens_the_next_session_in_the_same_environment_and_never_reads_the_process() {
        let root = TempDir::new().expect("temp root");
        let env = env(&root);
        let session = Session::open(&env, &config()).expect("the launch's session");
        let boot = Boot {
            env: env.clone(),
            config: config(),
            session,
        };

        let next = renew(&boot).expect("the next session");

        // If `/new` read the process again it would answer the process's own
        // state root and the process's own workspace — neither of which is the
        // temporary root this launch was booted in.
        assert!(
            next.dir().root().starts_with(&env.state),
            "`/new` opens in the state root the launch was booted in: {}",
            next.dir().root().display()
        );
        assert!(
            next.policy().writable().paths().contains(&env.workspace),
            "and in its workspace, not the process's: {:?}",
            next.policy().writable().paths()
        );
    }
}
