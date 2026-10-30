//! Ctrl-C. The handler raises the run's cancel flag — one atomic store, the one
//! async-signal-safe thing to do — and every part of the run checks it: the
//! loop stops between turns, and an executor kills the command's process group
//! mid-flight, so a cancelled run stops dead and leaves no orphan (ADR-0004).

use std::sync::OnceLock;

use sindri_core::cancel::Cancel;

/// The flag the handler raises. It is set before the handler is installed, so
/// the handler never runs while it is still empty.
static RAISED: OnceLock<Cancel> = OnceLock::new();

pub fn install(cancel: Cancel) {
    if RAISED.set(cancel).is_err() {
        return;
    }
    // SAFETY: `on_sigint` only reads a `OnceLock` that was filled before the
    // handler was installed and stores into an atomic; `sigaction` and
    // `sigemptyset` touch nothing else, and the zeroed mask is valid for the
    // struct `sigemptyset` immediately initialises.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_sigint as *const () as libc::sighandler_t;
        libc::sigemptyset(&mut action.sa_mask);
        // No `SA_RESTART`: a blocking read returns early, so the run notices the
        // cancellation instead of waiting out a request that no longer matters.
        action.sa_flags = 0;
        if libc::sigaction(libc::SIGINT, &action, std::ptr::null_mut()) != 0 {
            eprintln!("sindri: could not install the SIGINT handler; Ctrl-C will kill the run");
        }
    }
}

extern "C" fn on_sigint(_signal: libc::c_int) {
    if let Some(cancel) = RAISED.get() {
        cancel.trigger();
    }
}
