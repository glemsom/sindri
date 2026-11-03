//! `sindri doctor`: run the probe, print the description, the rules the run
//! stands in and the skill catalog, and turn a failure into one actionable line
//! on stderr.

use sindri_core::config::Overrides;
use sindri_core::doctor;
use sindri_core::instructions;
use sindri_core::paths;
use sindri_core::skills;
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
    match doctor::probe(env, &config.extras, config.enforcement) {
        Ok(report) => {
            // The catalog is not the sandbox's business, so it is read and said
            // beside the description rather than folded into it. A probe that
            // fails says only its own line, on stderr: a machine with no sandbox
            // can run no skill, and a failure is not the place to list them.
            let catalog = skills::discover(&env.workspace, paths::home().as_deref());
            // Read through the same function the run reads, so the doctor names
            // the file a run would be handed and not a second opinion about it.
            let instructions = instructions::read(&env.workspace);
            print!("{}{}", report.description, catalog.report());
            if let Some(line) = instructions.line() {
                println!("{line}");
            }
            ExitCode::Success.code()
        }
        Err(error) => {
            eprintln!("sindri: doctor: {error}");
            ExitCode::EnvironmentFailure.code()
        }
    }
}
