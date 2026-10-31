//! The one line a run that left the sandbox says on the way in: stderr, because
//! stdout is the answer or the one object a script parses. The frame says
//! nothing here — its rail chip and its record already carry the posture.

use sindri_core::sandbox::Enforcement;

/// Said by the front ends whose stdout is a contract: batch mode and
/// `sandbox exec`. Silence when the run is sandboxed.
pub(crate) fn unsafe_run(enforcement: Enforcement) {
    if enforcement == Enforcement::YoloUnsafe {
        eprintln!(
            "sindri: --yolo-unsafe: commands run with your own privileges, outside bubblewrap"
        );
    }
}
