//! The enforced argv, derived from the policy. No hand-written bwrap invocation
//! exists anywhere else in the tree (ADR-0002).
//!
//! `to_argv` is a pure function of the policy: it reads nothing from the
//! ambient environment, so enforcement cannot change behind the description's
//! back.

use super::SandboxPolicy;

/// The locale inside the sandbox is fixed, not inherited: a UTF-8 locale makes
/// tools behave predictably, and inheriting one would make the argv depend on
/// the host.
pub const LOCALE: &str = "C.UTF-8";

/// What actually runs: bubblewrap wrapping the command, or the command alone
/// under `--yolo-unsafe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// The command inside bwrap.
    Sandboxed(Vec<String>),
    /// The command with the user's own privileges.
    Unsandboxed(Vec<String>),
}

impl Invocation {
    /// The exact argv to execute.
    pub fn argv(&self) -> &[String] {
        match self {
            Invocation::Sandboxed(argv) | Invocation::Unsandboxed(argv) => argv,
        }
    }
}

impl SandboxPolicy {
    /// The argv that enforces this policy. Mount order is load-bearing: the
    /// read-only root first, or the writable binds are shadowed.
    pub fn to_argv(&self, command: &[String]) -> Invocation {
        if self.is_unsafe() {
            return Invocation::Unsandboxed(command.to_vec());
        }
        let temp = self.session_temp().display().to_string();
        let home = self.session_temp().join("home").display().to_string();

        let mut argv: Vec<String> = Vec::with_capacity(40 + command.len());
        push(&mut argv, &["bwrap"]);
        push(&mut argv, &["--ro-bind", "/", "/"]);
        for path in self.writable().paths() {
            let path = path.display().to_string();
            push(&mut argv, &["--bind", &path, &path]);
        }
        push(&mut argv, &["--bind", &temp, &temp]);
        push(&mut argv, &["--dev", "/dev"]);

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

        push(&mut argv, &["--clearenv"]);
        for (key, value) in [
            ("HOME", home.as_str()),
            ("TMPDIR", temp.as_str()),
            ("TEMP", temp.as_str()),
            ("TMP", temp.as_str()),
            ("PATH", self.search_path()),
            ("LANG", LOCALE),
            ("LC_ALL", LOCALE),
        ] {
            push(&mut argv, &["--setenv", key, value]);
        }

        push(&mut argv, &["--"]);
        argv.extend(command.iter().cloned());
        Invocation::Sandboxed(argv)
    }
}

fn push(argv: &mut Vec<String>, items: &[&str]) {
    argv.extend(items.iter().map(|item| item.to_string()));
}
