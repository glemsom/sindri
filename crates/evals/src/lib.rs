//! The eval suite's parts, so its own reporting and its checks can be tested
//! without a process: the binary is the table, and this is everything under it.

pub mod case; // the ten evals: a prompt, a recorded run, and the checks
pub mod catalog; // eval 9's check: the one case whose rule lives in a skill
pub mod checks; // what each eval asserts
pub mod guard; // the suite refuses to run when the sandbox is not enforcing
pub mod house; // eval 10's check, where the repository's own rules reached the model whole
pub mod report; // the pass/fail table: one line per eval
pub mod run; // one eval's run, as an eval checks it
pub mod workspace; // one eval's isolated host
