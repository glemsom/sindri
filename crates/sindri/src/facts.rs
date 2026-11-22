use std::path::{Path, PathBuf};
use std::sync::Arc;

use sindri_core::open::Ground;
use sindri_core::provider::served::Listing;
use sindri_core::records::{Entry, SessionDocument};
use sindri_core::run::Loaded;

use crate::ledger::{self, Boot};
use crate::pulse::Pulse;

#[derive(Clone, Debug)]
pub struct Facts {
    rail: ledger::Facts,
    workspace: Option<PathBuf>,
    ground: Option<Ground>,
    home: Option<PathBuf>,
    served: Option<Listing>,
    record: Option<SessionDocument>,
    transcript: Option<Arc<[Entry]>>,
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
    fn default() -> Facts {
        Facts {
            rail: ledger::Facts::default(),
            workspace: None,
            ground: None,
            home: None,
            served: None,
            record: None,
            transcript: None,
            pulse: None,
            live: false,
            focus: Focus::Focused,
            splash: true,
        }
    }
}

impl Facts {
    pub fn boot(boot: Boot, home: Option<PathBuf>) -> Facts {
        let workspace = boot.workspace.clone();
        Facts {
            rail: ledger::Facts::boot(boot),
            workspace: Some(workspace),
            home,
            ..Facts::default()
        }
    }

    pub fn asked(mut self) -> Facts {
        self.splash = false;
        self
    }

    pub fn running(mut self) -> Facts {
        self.live = true;
        self.asked()
    }

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

    pub fn transcribed(mut self, entries: Vec<Entry>) -> Facts {
        self.transcript = Some(Arc::from(entries));
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

    pub fn live(&self) -> bool {
        self.live
    }

    pub fn rail(&self) -> &ledger::Facts {
        &self.rail
    }

    pub fn loaded(&self) -> &Loaded {
        &self.rail.loaded
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

    pub fn transcript(&self) -> Option<&[Entry]> {
        self.transcript.as_deref()
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
