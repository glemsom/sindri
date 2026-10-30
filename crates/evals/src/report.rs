//! The pass/fail table: one line per eval, and a non-zero status when any of
//! them failed. A suite whose failures are not loud is not a suite.

#[derive(Debug, Clone)]
pub struct Verdict {
    pub name: &'static str,
    pub problems: Vec<String>,
}

impl Verdict {
    pub fn passed(&self) -> bool {
        self.problems.is_empty()
    }
}

pub fn line(verdict: &Verdict) -> String {
    let state = if verdict.passed() { "PASS" } else { "FAIL" };
    match verdict.problems.first() {
        Some(first) => format!("{:<58} {state}  {first}", verdict.name),
        None => format!("{:<58} {state}", verdict.name),
    }
}

pub fn exit_code(verdicts: &[Verdict]) -> i32 {
    if verdicts.iter().all(Verdict::passed) {
        0
    } else {
        1
    }
}
