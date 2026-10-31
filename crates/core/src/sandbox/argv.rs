//! The enforced argv, derived from the policy. No hand-written bwrap invocation
//! exists anywhere else in the tree.

use std::os::fd::RawFd;

use super::SandboxPolicy;

/// The locale inside the sandbox is fixed, not inherited: a UTF-8 locale makes
/// tools behave predictably, and inheriting one would make the argv depend on
/// the host.
pub const LOCALE: &str = "C.UTF-8";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    Sandboxed(Vec<String>),
    Unsandboxed(Vec<String>),
}

impl Invocation {
    pub fn argv(&self) -> &[String] {
        match self {
            Invocation::Sandboxed(argv) | Invocation::Unsandboxed(argv) => argv,
        }
    }
}

impl SandboxPolicy {
    /// The argv that enforces this policy. Mount order is load-bearing: the
    /// read-only root first, or the writable binds are shadowed.
    pub fn to_argv(&self, command: &[String], status_fd: Option<RawFd>) -> Invocation {
        if self.is_unsafe() {
            return Invocation::Unsandboxed(command.to_vec());
        }
        let mut argv: Vec<String> = Vec::with_capacity(40 + command.len());
        let temp = self.session_temp().display().to_string();
        push(&mut argv, &["bwrap"]);
        push(&mut argv, &["--ro-bind", "/", "/"]);
        for path in self.writable().paths() {
            let path = path.display().to_string();
            push(&mut argv, &["--bind", &path, &path]);
        }
        push(&mut argv, &["--bind", &temp, &temp]);
        push(&mut argv, &["--dev", "/dev"]);
        // A fresh procfs, or the pid namespace is a lie: `/proc` would still
        // list the host's processes, which this sandbox cannot reach.
        push(&mut argv, &["--proc", "/proc"]);

        push(
            &mut argv,
            &[
                "--unshare-pid",
                "--unshare-ipc",
                "--unshare-uts",
                "--unshare-cgroup-try",
                "--share-net",
                "--die-with-parent",
                "--new-session",
            ],
        );

        if let Some(fd) = status_fd {
            push(&mut argv, &["--json-status-fd", &fd.to_string()]);
        }

        push(&mut argv, &["--clearenv"]);
        for (key, value) in self.environment() {
            push(&mut argv, &["--setenv", key, &value]);
        }

        push(&mut argv, &["--"]);
        argv.extend(command.iter().cloned());
        Invocation::Sandboxed(argv)
    }

    /// The variables the sandbox sets, in the order `--setenv` writes them. The
    /// one list: the argv enforces it and the description names it, so the
    /// boundary the model is told is the boundary it runs in.
    pub(super) fn environment(&self) -> Vec<(&'static str, String)> {
        let temp = self.session_temp().display().to_string();
        let home = self.session_temp().join("home").display().to_string();
        vec![
            ("HOME", home),
            ("TMPDIR", temp),
            ("PATH", self.search_path().to_string()),
            ("LANG", LOCALE.to_string()),
            ("LC_ALL", LOCALE.to_string()),
        ]
    }
}

fn push(argv: &mut Vec<String>, items: &[&str]) {
    argv.extend(items.iter().map(|item| item.to_string()));
}
