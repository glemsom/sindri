use std::error::Error;
use std::fmt;
use std::io::ErrorKind;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::bash;
use crate::policy::{Boundary, Enforcement};

pub const EXIT_CODE: u8 = 3;

const PROBE: &str = "true";

/// The three checks `scripts/sandbox-probe.sh` distinguishes, in its order, so a person reading
/// either one reads the same names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    Installed,
    Runs,
    /// Unprivileged user namespaces, which some kernels forbid and no retry can supply.
    Namespaces,
}

impl Check {
    pub fn name(self) -> &'static str {
        match self {
            Check::Installed => "bubblewrap is on PATH",
            Check::Runs => "bubblewrap runs",
            Check::Namespaces => "bubblewrap creates an unprivileged sandbox",
        }
    }

    pub fn remedy(self) -> &'static str {
        match self {
            Check::Installed => {
                "install bubblewrap, or run with --yolo-unsafe to run unconfined on purpose"
            }
            Check::Runs => {
                "the bwrap on PATH is not a working bubblewrap: reinstall it, or run with \
                 --yolo-unsafe to run unconfined on purpose"
            }
            Check::Namespaces => {
                "this kernel does not allow unprivileged user namespaces: enable them, or run \
                 with --yolo-unsafe to run unconfined on purpose"
            }
        }
    }
}

impl fmt::Display for Check {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    checks: Vec<Check>,
}

impl Proof {
    pub fn checks(&self) -> &[Check] {
        &self.checks
    }
}

impl fmt::Display for Proof {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.checks.is_empty() {
            return formatter.write_str("sandbox not proved: this run asked for no sandbox");
        }
        let names: Vec<&str> = self.checks.iter().map(|check| check.name()).collect();
        write!(
            formatter,
            "sandbox proved ({} checks): {}",
            self.checks.len(),
            names.join(", ")
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    check: Check,
    said: String,
}

impl Card {
    fn new(check: Check, said: impl Into<String>) -> Card {
        Card {
            check,
            said: one_line(&said.into()),
        }
    }

    pub fn check(&self) -> Check {
        self.check
    }

    pub fn said(&self) -> &str {
        &self.said
    }

    pub fn exit_code(&self) -> u8 {
        EXIT_CODE
    }
}

impl fmt::Display for Card {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot create the sandbox: {}",
            self.check.name()
        )?;
        if !self.said.is_empty() {
            write!(formatter, "\n  said:  {}", self.said)?;
        }
        write!(formatter, "\n  fix:   {}", self.check.remedy())
    }
}

impl Error for Card {}

pub fn prove_with(bwrap: &Path, boundary: &Boundary, session_temp: &Path) -> Result<Proof, Card> {
    if boundary.enforcement() == Enforcement::Unsafe {
        return Ok(Proof { checks: Vec::new() });
    }

    let spoke = match ask(bwrap, &[String::from("--version")]) {
        Ok(spoken) => spoken,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(Card::new(Check::Installed, error.to_string()));
        }
        Err(error) => return Err(Card::new(Check::Runs, error.to_string())),
    };
    let mut checks = vec![Check::Installed];
    if !spoke.status.success() {
        return Err(Card::new(Check::Runs, said_of(&spoke)));
    }
    checks.push(Check::Runs);

    let invocation = bash::invocation(boundary, session_temp, PROBE)
        .map_err(|error| Card::new(Check::Namespaces, error.to_string()))?;
    let made = match ask(bwrap, invocation.args()) {
        Ok(made) => made,
        Err(error) => return Err(Card::new(Check::Namespaces, error.to_string())),
    };
    if !made.status.success() {
        return Err(Card::new(Check::Namespaces, said_of(&made)));
    }
    checks.push(Check::Namespaces);

    Ok(Proof { checks })
}

/// No stdin, because a check has nothing to type at it, and both streams piped, because a
/// refusal's words are the card's.
fn ask(program: &Path, args: &[String]) -> std::io::Result<Output> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
}

fn said_of(output: &Output) -> String {
    let (stderr, stdout) = (
        String::from_utf8_lossy(&output.stderr).into_owned(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    );
    let spoken = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    let words = one_line(&spoken);
    if !words.is_empty() {
        return words;
    }
    match output.status.code() {
        Some(code) => format!("it exited {code} and said nothing"),
        None => String::from("a signal killed it and it said nothing"),
    }
}

fn one_line(words: &str) -> String {
    words.split_whitespace().collect::<Vec<&str>>().join(" ")
}

#[cfg(test)]
mod tests;
