//! The sandbox: one policy value, and the only two things derived from it —
//! the bwrap argv that is enforced and the description that is published.

mod argv;
mod describe;
mod extras;
mod policy;

pub use argv::LOCALE;
pub use extras::{Extras, FLOOR};
pub use policy::{Enforcement, FALLBACK_PATH, PolicyError, SandboxPolicy, WritableSet};
