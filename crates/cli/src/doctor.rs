//! `sindri doctor`: run the probe, print the description, and turn a failure
//! into one actionable line on stderr.

use sindri_core::config::Overrides;
use sindri_core::doctor;
use sindri_types::ExitCode;

use crate::boot;

pub fn run(overrides: Overrides) -> i32 {
    // The description must be the one a run really gives the model, so the
    // config comes through the diagnostic door: the same sources and the same
    // checks, only the model and the credential not demanded. A value a run
    // would refuse is refused here too, out loud.
    let configured = match boot::configure(overrides) {
        Ok(configured) => configured,
        Err(error) => return error.report(),
    };
    let (env, config) = (&configured.env, &configured.config);
    match doctor::probe(env, &config.extra_writable, config.enforcement) {
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
