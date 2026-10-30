//! Cancellation: the one flag a run checks to know it must stop now. A signal
//! handler is allowed to raise it — the flag is an atomic and nothing else, so
//! raising it from a handler is the one async-signal-safe thing to do — and the
//! loop and the executor both read it, which is what lets a cancel reach a
//! command that is already running.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A request to stop, raised by a key or a signal and put down between runs.
/// Cheap to clone, so every part of a run shares the same flag: raise it once
/// and the model call, the command and the loop all see it.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Cancel::default()
    }

    /// Asks the run to stop. One atomic store, so this may be called from a
    /// signal handler.
    pub fn trigger(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Puts the flag down, for a front end that runs more than one prompt in a
    /// launch: one flag per process, cleared between prompts, so a cancel ends
    /// one run and never the next.
    pub fn clear(&self) {
        self.0.store(false, Ordering::SeqCst);
    }

    pub fn is_triggered(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::Cancel;

    #[test]
    fn a_cleared_flag_is_down_for_the_next_run() {
        let cancel = Cancel::new();
        let shared = cancel.clone();
        cancel.trigger();
        assert!(shared.is_triggered(), "a raised flag stops the run");
        shared.clear();
        assert!(
            !cancel.is_triggered(),
            "the front end clears it between prompts, so the next run starts fresh"
        );
    }

    #[test]
    fn a_clone_raises_the_same_flag() {
        let cancel = Cancel::new();
        let shared = cancel.clone();
        assert!(!shared.is_triggered(), "a fresh flag is down");
        cancel.trigger();
        assert!(shared.is_triggered(), "one flag, wherever it is read");
    }
}
