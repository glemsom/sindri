//! The workspace and its ground, the home skills are read from, the served name, the record, the
//! state, the rail's facts, the marker and the focus, gathered as one value and handed to the frame
//! whole, beside the screen those facts are drawn as.

use std::path::{Path, PathBuf};

use sindri_core::open::Ground;
use sindri_core::provider::served::Listing;
use sindri_core::records::SessionDocument;

use crate::ledger::{self, Boot};
use crate::pulse::Pulse;

/// Handed whole, never a fact at a time: the screen a frame is drawn as follows from it, so a frame
/// handed none is the splash screen and one handed a run's is that run's frame. No fact that lands
/// ends the splash screen, because the facts a launch opens on are the splash screen's own.
#[derive(Clone, Debug)]
pub struct Facts {
    rail: ledger::Facts,
    workspace: Option<PathBuf>,
    ground: Option<Ground>,
    home: Option<PathBuf>,
    served: Option<Listing>,
    record: Option<SessionDocument>,
    pulse: Option<Pulse>,
    live: bool,
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
            live: false,
            focus: Focus::Focused,
            splash: true,
        }
    }
}

impl Facts {
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

    /// The same value as the frame a prompt has been asked for: the splash screen is over.
    pub fn asked(mut self) -> Facts {
        self.splash = false;
        self
    }

    /// The same value with the run that prompt starts.
    pub fn running(mut self) -> Facts {
        self.live = true;
        self.asked()
    }

    /// The same value with the run that prompt started ended.
    pub fn idle(mut self) -> Facts {
        self.live = false;
        self
    }

    pub fn railed(mut self, rail: ledger::Facts) -> Facts {
        self.rail = rail;
        self
    }

    pub fn recorded(mut self, document: &SessionDocument) -> Facts {
        self.record = Some(document.clone());
        self
    }

    pub fn working(mut self, workspace: PathBuf, home: Option<PathBuf>) -> Facts {
        self.workspace = Some(workspace);
        self.home = home;
        self
    }

    pub fn grounded(mut self, ground: Option<Ground>) -> Facts {
        self.ground = ground;
        self
    }

    pub fn served_by(mut self, listing: Listing) -> Facts {
        self.served = Some(listing);
        self
    }

    /// The same value with the names a wire served dropped: a screen that asks again holds none until
    /// that answer lands.
    pub fn unserved(mut self) -> Facts {
        self.served = None;
        self
    }

    pub fn pulsed(mut self, pulse: Option<Pulse>) -> Facts {
        self.pulse = pulse;
        self
    }

    pub fn focused(mut self, focused: bool) -> Facts {
        self.focus = match focused {
            true => Focus::Focused,
            false => Focus::Away,
        };
        self
    }

    pub fn splash(&self) -> bool {
        self.splash
    }

    /// Whether a run is live: the draft band's two words, where the rail says the run's state whole,
    /// the stop it ended on included.
    pub fn live(&self) -> bool {
        self.live
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
    fn a_run_a_prompt_has_been_asked_for_is_no_longer_the_splash_screen() {
        let facts = Facts::default().running();
        assert!(
            !facts.splash(),
            "a run asked for is that run's frame, and never the splash screen"
        );
        assert!(
            facts.live(),
            "and the run it was asked for is the one that is live"
        );
        assert!(!facts.idle().live(), "and the run a stop ended is not");
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
        assert_eq!(recorded.live(), landed.live(), "and no state moved with it");

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
