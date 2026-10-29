//! `sindri doctor`: run the probe, print the description, and turn a failure
//! into one actionable line on stderr.

use sindri_core::doctor;
use sindri_types::ExitCode;

pub fn run() -> i32 {
    match doctor::probe() {
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
