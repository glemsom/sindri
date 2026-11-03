//! The sandbox: one policy value, and the only two things derived from it —
//! the bwrap argv that is enforced and the description that is published.

mod argv;
mod describe;
mod policy;

pub use argv::LOCALE;
pub use policy::{Enforcement, FALLBACK_PATH, PolicyError, SandboxPolicy, WritableSet};
