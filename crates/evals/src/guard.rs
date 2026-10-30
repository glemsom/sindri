//! The suite refuses to run when the sandbox is not actually enforcing (issue
//! #10): eval 7 deliberately writes outside the writable set, so a pre-flight
//! proves bubblewrap runs and the kernel refuses the canary write.

use std::path::Path;
use std::process::Command;

use crate::workspace::Workspace;

const CANARY: &str = "guard-canary";

pub fn sandbox_is_safe(bin: &Path, ws: &Workspace) -> Result<(), String> {
    let doctor = Command::new(bin)
        .current_dir(&ws.repo)
        .arg("doctor")
        .env("XDG_STATE_HOME", &ws.state)
        .env("XDG_CONFIG_HOME", &ws.config)
        .output()
        .map_err(|error| format!("run `sindri doctor`: {error}"))?;
    let description = String::from_utf8_lossy(&doctor.stdout).to_string();
    if !doctor.status.success() {
        return Err(format!(
            "`sindri doctor` failed: {}",
            String::from_utf8_lossy(&doctor.stderr).trim()
        ));
    }
    if !description.contains("enforcement: bubblewrap") {
        return Err(format!(
            "the sandbox policy in force is not bubblewrap:\n{description}"
        ));
    }

    let outside = ws.root().join(CANARY);
    let canary = format!("echo canary > {}", outside.display());
    let probe = Command::new(bin)
        .current_dir(&ws.repo)
        .args(["sandbox", "exec", "--", "bash", "-c", &canary])
        .env("XDG_STATE_HOME", &ws.state)
        .env("XDG_CONFIG_HOME", &ws.config)
        .output()
        .map_err(|error| format!("run `sindri sandbox exec`: {error}"))?;
    if crate::run::exists(&outside) {
        let _ = std::fs::remove_file(&outside);
        return Err(format!(
            "a write outside the writable set landed at {}: the sandbox is not enforcing",
            outside.display()
        ));
    }
    let outcome = String::from_utf8_lossy(&probe.stdout).to_string();
    if !outcome.contains("Read-only file system") {
        return Err(format!(
            "the canary write was not refused by the sandbox: {outcome}{}",
            String::from_utf8_lossy(&probe.stderr)
        ));
    }
    Ok(())
}
