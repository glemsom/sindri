//! The facts a frame draws, gathered as one value: what a launch knows before a run, and what the
//! run and its session add as they land. A frame handed no facts is the splash screen, and only a
//! launch's opening facts are the splash screen's.

use std::path::{Path, PathBuf};

use sindri_core::open::Ground;
use sindri_core::provider::served::Listing;
use sindri_core::records::SessionDocument;

use crate::composer::State;
use crate::ledger::{self, Boot};
use crate::pulse::Pulse;

/// The facts a frame draws, as one value: what a launch knows before a run, and what the run and its
/// session add as they land. A frame handed no facts is the splash screen, and only a launch's
/// opening facts are the splash screen's.
#[derive(Clone, Debug)]
pub struct Facts {
    rail: ledger::Facts,
    workspace: Option<PathBuf>,
    ground: Option<Ground>,
    home: Option<PathBuf>,
    served: Option<Listing>,
    record: Option<SessionDocument>,
    pulse: Option<Pulse>,
    state: State,
    focus: Focus,
    splash: bool,
}

/// Whether the terminal reports this window as the one being read: a terminal that never reports is
/// being read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Focused,
    Away,
}

impl Default for Facts {
    /// The splash screen: the frame a launch opens on, before a fact of a run's has landed.
    fn default() -> Facts {
        Facts {
            rail: ledger::Facts::default(),
            workspace: None,
            ground: None,
            home: None,
            served: None,
            record: None,
            pulse: None,
            state: State::Idle,
            focus: Focus::Focused,
            splash: true,
        }
    }
}

impl Facts {
    /// The facts of a run a launch has told nothing of yet: what a frame drawn by no launch, and a
    /// screen test's frame, is built from. Not the splash screen.
    pub fn run() -> Facts {
        Facts {
            splash: false,
            ..Facts::default()
        }
    }

    /// The facts a launch opens on, beside the home its skills are read from: the splash screen over
    /// the wire, the model, the workspace, the branch and the enforcement it already knows.
    pub fn boot(boot: Boot, home: Option<PathBuf>) -> Facts {
        let workspace = boot.workspace.clone();
        Facts {
            rail: ledger::Facts::boot(boot),
            workspace: Some(workspace),
            home,
            ..Facts::default()
        }
    }

    /// The same value with the facts the run's rail lays, as the launch's report carries them.
    pub fn railed(mut self, rail: ledger::Facts) -> Facts {
        self.rail = rail;
        self
    }

    /// The same value with the record a run's launch has written so far.
    pub fn recorded(mut self, document: &SessionDocument) -> Facts {
        self.record = Some(document.clone());
        self
    }

    /// The same value with the directory the run writes in and the home its skills are read from.
    pub fn working(mut self, workspace: PathBuf, home: Option<PathBuf>) -> Facts {
        self.workspace = Some(workspace);
        self.home = home;
        self
    }

    /// The same value with the place a run judges a target from, or nothing where its temp will not
    /// resolve.
    pub fn grounded(mut self, ground: Option<Ground>) -> Facts {
        self.ground = ground;
        self
    }

    /// The same value with the names a wire served.
    pub fn served_by(mut self, listing: Listing) -> Facts {
        self.served = Some(listing);
        self
    }

    /// The same value with the marker at the head of the newest answer.
    pub fn pulsed(mut self, pulse: Option<Pulse>) -> Facts {
        self.pulse = pulse;
        self
    }

    /// The same value with the terminal's report of who is reading.
    pub fn focused(mut self, focused: bool) -> Facts {
        self.focus = match focused {
            true => Focus::Focused,
            false => Focus::Away,
        };
        self
    }

    /// The same value as the frame a prompt has been asked for: the splash screen is over.
    pub fn asked(mut self) -> Facts {
        self.splash = false;
        self
    }

    /// The same value with the run a prompt has just started.
    pub fn running(mut self) -> Facts {
        self.state = State::Running;
        self.splash = false;
        self
    }

    /// The same value with the run a stop has ended.
    pub fn idle(mut self) -> Facts {
        self.state = State::Idle;
        self
    }

    pub fn splash(&self) -> bool {
        self.splash
    }

    pub fn rail(&self) -> &ledger::Facts {
        &self.rail
    }

    pub fn workspace(&self) -> Option<&Path> {
        self.workspace.as_deref()
    }

    pub fn ground(&self) -> Option<&Ground> {
        self.ground.as_ref()
    }

    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    pub fn served(&self) -> Option<&Listing> {
        self.served.as_ref()
    }

    pub fn record(&self) -> Option<&SessionDocument> {
        self.record.as_ref()
    }

    pub fn pulse(&self) -> Option<&Pulse> {
        self.pulse.as_ref()
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sindri_core::policy::{Enforcement, PolicyRecord};
    use sindri_core::provider::Route;
    use sindri_core::records::{Dials, SessionDocument};
    use sindri_core::run::{State as RunState, Stop};
    use sindri_core::turn::Usage;
    use sindri_core::wire::Wire;

    use super::*;

    /// The launch a boot frame is built from: a config's wire and model, the workspace the launch
    /// stands in and its branch, and the enforcement its policy holds.
    fn boot() -> Boot {
        Boot {
            wire: Some(Wire::Anthropic),
            model: Some(String::from("claude-sonnet-4-5")),
            workspace: PathBuf::from("/srv/workspace"),
            branch: Some(String::from("main")),
            enforcement: Enforcement::Bubblewrap,
        }
    }

    fn rail() -> ledger::Facts {
        ledger::Facts {
            wire: Some(String::from("anthropic")),
            model: Some(String::from("claude-sonnet-4-5-20250929")),
            workspace: Some(PathBuf::from("/srv/workspace")),
            branch: Some(String::from("main")),
            sandbox: Some(Enforcement::Bubblewrap),
            ..ledger::Facts::default()
        }
    }

    fn document() -> SessionDocument {
        SessionDocument {
            session: String::from("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"),
            wire: Wire::Anthropic,
            model: String::from("claude-sonnet-4-5-20250929"),
            enforcement: Enforcement::Bubblewrap,
            policy: PolicyRecord {
                enforcement: Enforcement::Bubblewrap,
                writable: vec![String::from("/srv/workspace")],
                timeout_seconds: 120,
                command_budget_ms: 3_600_000,
                output_budget_bytes: 8 * 1024 * 1024,
                description: String::from("the boundary the model was told about"),
            },
            workspace: PathBuf::from("/srv/workspace"),
            branch: Some(String::from("main")),
            commit: Some(String::from("16becb5")),
            dials: Dials {
                max_turns: 250,
                command_budget_ms: 3_600_000,
                output_budget_bytes: 8 * 1024 * 1024,
            },
            start: String::from("2026-01-01T00:00:00Z"),
            end: Some(String::from("2026-01-01T00:04:00Z")),
            turns: 3,
            usage: Usage {
                input: Some(48_200),
                output: Some(3_100),
                total: Some(51_300),
                cached_input: Some(12_000),
                reasoning: Some(900),
            },
            stopped: RunState::Stopped(Stop::Final),
            route: Some(Route::Chat),
            thinking: true,
        }
    }

    #[test]
    fn the_facts_a_frame_is_built_with_are_the_splash_screen() {
        assert!(
            Facts::default().splash(),
            "a frame handed no facts is the splash screen a launch opens on"
        );
    }

    #[test]
    fn a_run_a_launch_has_told_the_frame_of_is_no_longer_the_splash_screen() {
        let facts = Facts::run().railed(rail()).running();
        assert!(
            !facts.splash(),
            "a run whose rail the launch has laid is that run's frame, and never the splash screen"
        );
        assert_eq!(
            facts.state(),
            State::Running,
            "and the run it was told of is the one that is running"
        );
    }

    #[test]
    fn the_facts_a_launch_opens_on_are_the_splash_screen_over_what_the_launch_knows() {
        let facts = Facts::boot(boot(), None);
        assert!(
            facts.splash(),
            "a launch opens on the splash screen, and a fact of a run's has yet to land"
        );
        let rail = facts.rail();
        assert_eq!(
            rail.wire.as_deref(),
            Some("anthropic"),
            "the rail reads the wire the launch named, in the wire's own word"
        );
        assert_eq!(
            rail.model.as_deref(),
            Some("claude-sonnet-4-5"),
            "the model it named, whole here and cut to its head wherever the rail draws it"
        );
        assert_eq!(
            rail.workspace.as_deref(),
            Some(Path::new("/srv/workspace")),
            "the workspace the launch stands in"
        );
        assert_eq!(
            rail.branch.as_deref(),
            Some("main"),
            "its branch, as the repository answered before the first paint"
        );
        assert_eq!(
            rail.sandbox,
            Some(Enforcement::Bubblewrap),
            "and the enforcement its policy holds"
        );
        assert_eq!(
            rail.session.as_deref(),
            Some("—"),
            "a session no run has opened yet is an em dash, and not an absence"
        );
        assert_eq!(
            rail.temp.as_deref(),
            Some(Path::new("—")),
            "and so is the temp a run has yet to fill"
        );
        assert_eq!(
            facts.workspace(),
            Some(Path::new("/srv/workspace")),
            "the directory the run writes in is the one the launch stands in"
        );
    }

    #[test]
    fn a_fact_that_lands_leaves_every_other_fact_where_it_was() {
        let landed = Facts::boot(boot(), None)
            .railed(rail())
            .pulsed(Some(Pulse::live(0)))
            .running();

        let recorded = landed.clone().recorded(&document());
        assert_eq!(
            recorded.record(),
            Some(&document()),
            "the record a run's launch wrote is the fact that landed"
        );
        assert_eq!(
            recorded.rail().model,
            landed.rail().model,
            "and no rail moved with it"
        );
        assert_eq!(
            recorded.rail().workspace,
            landed.rail().workspace,
            "no fact the rail lays moved with it either"
        );
        assert_eq!(
            recorded.pulse(),
            landed.pulse(),
            "no marker at the head of the answer moved with it"
        );
        assert_eq!(
            recorded.state(),
            landed.state(),
            "and no state moved with it"
        );

        let focused = landed.clone().focused(false);
        assert_eq!(
            focused.focus(),
            Focus::Away,
            "the terminal's report of who is reading is the fact that landed"
        );
        assert_eq!(
            focused.record(),
            None,
            "and the record stands where it was: no report touches it"
        );
    }
}
