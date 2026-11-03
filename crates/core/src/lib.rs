//! The agent core: the sandbox policy, the two derivations from it, and the
//! environment probe. Knows nothing about a terminal.

pub mod agent; // the turn loop and the tools it drives
pub mod cancel; // the one flag a run checks to know it must stop now
pub mod clock; // the wall clock a record is dated by
pub mod config; // a run's settings, and which source wins
pub mod doctor; // the environment probe behind `sindri doctor`
pub mod exec; // the bash executor: one port, two implementations
pub mod instructions; // a repository's own rules, read once at boot
pub mod output; // the machine-readable output contract
pub mod paths; // where Sindri keeps its state
pub mod provider; // the model, behind one seam
pub mod record; // the record every run leaves
pub mod sandbox; // one policy value, and the only two things derived from it
pub mod session; // the value a front end holds to have a run
pub mod skills; // the three scopes folded, the frontmatter read
pub mod status; // what a run tells its terminal it is doing, as bytes
pub mod workspace; // the tree a run stands in
