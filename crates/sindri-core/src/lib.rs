//! The core of Sindri: the run loop, the policy in force, the wire dialects, the
//! records, the skills and the config. Every decision a run makes lives here,
//! and nothing that knows what a terminal is does.

pub mod bash; // the only way a run acts
pub mod config; // read and written whole and patched in place
pub mod policy; // the boundary in force, and every tool description derived from it
pub mod provider; // an ask goes out in Sindri's own words and a reply comes back
pub mod records; // the session directory, and the entries written into it
pub mod redact; // every string that leaves the core is a `Redacted`, whose
pub mod run; // the agent loop, its turns and the stop reason that ended it
pub mod skills; // reported rather than silently resolved
pub mod wire; // a provider dialect and its endpoint
