//! The pass/fail table: one line per eval, and a non-zero status when any of
//! them failed. A suite whose failures are not loud is not a suite.

/// What one eval found: no findings means the eval passed.
#[derive(Debug, Clone)]
pub struct Verdict {
    /// The eval's number and sentence, as the table prints it.
    pub name: &'static str,
    /// Every way the run broke its promise; empty is a pass.
    pub problems: Vec<String>,
}

impl Verdict {
    /// The one line the table prints for this eval.
    pub fn passed(&self) -> bool {
        self.problems.is_empty()
    }
}

/// One table line: name, verdict, and one reason when the verdict is a failure.
pub fn line(verdict: &Verdict) -> String {
    let state = if verdict.passed() { "PASS" } else { "FAIL" };
    match verdict.problems.first() {
        Some(first) => format!("{:<58} {state}  {first}", verdict.name),
        None => format!("{:<58} {state}", verdict.name),
    }
}

/// The shell's status: 0 only when every eval passed.
pub fn exit_code(verdicts: &[Verdict]) -> i32 {
    if verdicts.iter().all(Verdict::passed) {
        0
    } else {
        1
    }
}
