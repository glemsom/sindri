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
mod tests;
