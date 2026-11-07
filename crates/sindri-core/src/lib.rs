//! The core of Sindri: the run loop, the policy in force, the wire dialects, the
//! records, the skills and the config. Every decision a run makes lives here,
//! and nothing that knows what a terminal is does.

pub mod asking; // the one adapter between the run loop and the provider seam
pub mod bash; // the only way a run acts
pub mod config; // read and written whole and patched in place
pub mod first_run; // no file at all
pub mod open; // every refusal lives in the call
pub mod outcome; // what a batch run hands whatever scripted it
pub mod policy; // the boundary in force, and every tool description derived from it
pub mod preamble; // the persona, the principles, the environment the run stands in, and the project's own instructions
pub mod provider; // an ask goes out in Sindri's own words and a reply comes back
pub mod records; // the session directory, and the entries written into it
pub mod redact; // every string that leaves the core is a `Redacted`, whose
pub mod run; // the agent loop, its turns and the stop reason that ended it
pub mod sandbox; // boot proves it once, and a card names the failing check
pub mod session; // the temp a run's calls share and the spill a budget emptied
pub mod skills; // reported rather than silently resolved
pub mod tools; // the tools a run registers, and the ones it drops when there is nothing to name
pub mod wire; // a provider dialect and its endpoint
