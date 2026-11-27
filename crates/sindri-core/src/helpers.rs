use std::path::Path;
use std::process::{Command, Stdio};

use crate::bash;
use crate::policy::{Boundary, Enforcement};

pub const RECOMMENDED: &[&str] = &["git", "python3", "jq", "yq", "awk", "sed", "rg", "grep"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Helpers {
    available: Vec<String>,
    missing: Vec<String>,
    failure: Option<String>,
}

impl Helpers {
    pub fn probe(boundary: &Boundary, temp: &Path, bwrap: &Path) -> Helpers {
        let command = format!(
            "for helper in {}; do if command -v \"$helper\" >/dev/null 2>&1; then printf 'available:%s\\n' \"$helper\"; else printf 'missing:%s\\n' \"$helper\"; fi; done",
            RECOMMENDED.join(" ")
        );
        let result = (|| {
            let invocation =
                bash::invocation(boundary, temp, &command).map_err(|error| error.to_string())?;
            let program = if boundary.enforcement() == Enforcement::Unsafe {
                Path::new(invocation.program())
            } else {
                bwrap
            };
            let output = Command::new(program)
                .args(invocation.args())
                .stdin(Stdio::null())
                .output()
                .map_err(|error| error.to_string())?;
            if !output.status.success() {
                return Err("helper probe did not succeed".to_owned());
            }
            Self::parse(&String::from_utf8_lossy(&output.stdout))
        })();
        result.unwrap_or_else(|failure| Helpers {
            available: Vec::new(),
            missing: Vec::new(),
            failure: Some(failure),
        })
    }

    fn parse(output: &str) -> Result<Helpers, String> {
        let mut helpers = Helpers {
            available: Vec::new(),
            missing: Vec::new(),
            failure: None,
        };
        let lines: Vec<_> = output.lines().collect();
        if lines.len() != RECOMMENDED.len() {
            return Err("helper probe returned an incomplete inventory".to_owned());
        }
        for (line, name) in lines.into_iter().zip(RECOMMENDED) {
            if line == format!("available:{name}") {
                helpers.available.push((*name).to_owned());
            } else if line == format!("missing:{name}") {
                helpers.missing.push((*name).to_owned());
            } else {
                return Err("helper probe returned an invalid inventory".to_owned());
            }
        }
        Ok(helpers)
    }

    pub fn render(&self) -> String {
        if let Some(failure) = &self.failure {
            return format!("- Helper availability could not be checked: {failure}.");
        }
        let available = if self.available.is_empty() {
            "none".to_owned()
        } else {
            self.available.join(", ")
        };
        let mut lines = vec![format!(
            "- Available recommended helpers on the execution PATH: {available}."
        )];
        if !self.missing.is_empty() {
            lines.push(format!(
                "- Warning: recommended helpers checked but not found: {}.",
                self.missing.join(", ")
            ));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests;
