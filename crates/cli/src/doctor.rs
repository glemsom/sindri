//! `sindri doctor`: run the probe, print the description, and turn a failure
//! into one actionable line on stderr.

use sindri_core::config::{Config, Overrides};
use sindri_core::doctor;
use sindri_types::ExitCode;

use crate::failure;

pub fn run(overrides: Overrides) -> i32 {
    // The description must be the one a run really gives the model, so the
    // configured extras are read here too — and a config that cannot be read is
    // said out loud rather than quietly ignored.
    let extras = match Config::configured_extras(overrides) {
        Ok(extras) => extras,
        Err(error) => return failure::config_error(error),
    };
    match doctor::probe(extras.as_slice()) {
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
