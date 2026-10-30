//! `sindri doctor`: run the probe, print the description, and turn a failure
//! into one actionable line on stderr.

use sindri_core::config::Overrides;
use sindri_core::doctor;
use sindri_types::ExitCode;

use crate::boot;

pub fn run(overrides: Overrides) -> i32 {
    // The description must be the one a run really gives the model, so it is
    // read through the diagnostic door: the same config, from the same sources,
    // with the same checks — only the model and the credential are not required
    // of a diagnostic. The extras described here are then, by construction, the
    // extras a run would be granted, and a value a run would refuse is refused
    // here too, out loud (ADR-0005).
    let configured = match boot::configure(overrides) {
        Ok(configured) => configured,
        Err(error) => return error.report(),
    };
    match doctor::probe(&configured.env, &configured.config.extra_writable) {
        Ok(report) => {
            print!("{}", report.description);
            ExitCode::Success.code()
        }
        Err(error) => {
            eprintln!("sindri: doctor: {error}");
            ExitCode::EnvironmentFailure.code()
        }
    }
}
