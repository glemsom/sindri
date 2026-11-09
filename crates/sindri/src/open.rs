//! The opener is this host's, because the sandbox has no display: the core has
//! already judged the target, and it arrives as one argv word and never through a
//! shell. The child outlives the frame, so it is reaped rather than waited for.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::ErrorKind;
use std::process::{Command, Stdio};
use std::thread;

use sindri_core::open::Opening;

/// The program a Linux host opens a target with. A front end on another machine
/// builds its own [`Opener`] with that machine's opener instead.
pub const HOST_OPENER: &str = "xdg-open";

/// This machine's opener, and the one argv word it is spawned with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opener {
    /// The program to spawn, as the host names it.
    program: OsString,
}

impl Opener {
    /// An opener running `program`, which is what a test hands a recording
    /// script and what a front end hands its platform's opener.
    pub fn new(program: impl Into<OsString>) -> Opener {
        Opener {
            program: program.into(),
        }
    }

    /// This machine's opener: the one that knows which browser the user picked.
    pub fn host() -> Opener {
        Opener::new(HOST_OPENER)
    }

    /// The program this opener spawns.
    pub fn program(&self) -> &OsStr {
        &self.program
    }

    /// Hand a judged target to the opener — one argv word, both streams closed,
    /// and a thread that reaps the child so nothing is left behind.
    pub fn open(&self, opening: &Opening) -> Result<(), OpenError> {
        let mut child = Command::new(&self.program)
            .arg(opening.word())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| OpenError {
                program: self.program.clone(),
                kind: error.kind(),
                reason: error.to_string(),
            })?;

        // A browser outlives the frame, so the wait is a thread's: that is what reaps the
        // child instead of leaving a zombie a long session never collects.
        thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

/// Why the opener could not be spawned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenError {
    /// The program that would not run.
    program: OsString,
    /// What kind of failure it was, so a front end can tell a missing opener from
    /// one that is there and would not start.
    kind: ErrorKind,
    /// Why it would not run, in the operating system's own words.
    reason: String,
}

impl OpenError {
    /// The program that would not run.
    pub fn program(&self) -> &OsStr {
        &self.program
    }

    /// What kind of failure it was.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot run {}: {}",
            self.program.to_string_lossy(),
            self.reason
        )
    }
}

impl std::error::Error for OpenError {}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use sindri_core::config::Config;
    use sindri_core::open::Ground;
    use sindri_core::policy::{Boundary, Enforcement};
    use tempfile::TempDir;

    use super::*;

    /// How long a recording script is given to write its argv, which is generous
    /// because the spawner does not wait for it.
    const PATIENCE: Duration = Duration::from_secs(5);

    /// A workspace, a session temp beside it, and a place to keep what a fake
    /// opener records.
    struct Tree {
        /// Held so the directories outlive the ground that names them.
        root: TempDir,
    }

    impl Tree {
        /// A tree of the three directories a judgement and a spawn need.
        fn new() -> Tree {
            let root = tempfile::tempdir().expect("a temp tree");
            for name in ["workspace", "tmp"] {
                std::fs::create_dir_all(root.path().join(name)).expect("the directory");
            }
            Tree { root }
        }

        /// A directory of the tree, canonical.
        fn directory(&self, name: &str) -> PathBuf {
            std::fs::canonicalize(self.root.path().join(name)).expect("the directory resolves")
        }

        /// A file in the workspace holding `body`.
        fn in_workspace(&self, name: &str) -> PathBuf {
            let path = self.directory("workspace").join(name);
            std::fs::write(&path, "body").expect("the file");
            path
        }

        /// The ground a run standing in the workspace judges from.
        fn ground(&self) -> Ground {
            let boundary = Boundary::from_config(
                &Config::default(),
                &self.directory("workspace"),
                Enforcement::Bubblewrap,
            )
            .expect("the boundary holds");
            Ground::new(
                &boundary,
                &self.directory("tmp"),
                &self.directory("workspace"),
            )
            .expect("the ground resolves")
        }

        /// A fake opener that writes each of its argv words on a line of its own,
        /// so a test reads the argv the process was really spawned with.
        fn recording_opener(&self) -> Opener {
            let record = self.root.path().join("argv");
            let script = self.root.path().join("fake-opener");
            std::fs::write(
                &script,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
                    record.display()
                ),
            )
            .expect("the script");
            std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .expect("the script is executable");
            Opener::new(script)
        }

        /// The argv the fake opener recorded, one word per line, once it has
        /// written them.
        fn recorded(&self) -> Vec<String> {
            let record = self.root.path().join("argv");
            let deadline = Instant::now() + PATIENCE;
            loop {
                if let Ok(written) = std::fs::read_to_string(&record)
                    && written.ends_with('\n')
                {
                    return written.lines().map(str::to_owned).collect();
                }
                assert!(
                    Instant::now() < deadline,
                    "the fake opener recorded nothing within {PATIENCE:?}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    impl Opener {
        /// Spawns the fake opener, retrying while the script it was handed is
        /// still open for writing elsewhere in this process: tests run on forked
        /// threads, so a fixture written a moment ago can be `ETXTBSY`.
        fn open_patiently(&self, opening: &Opening) -> Result<(), OpenError> {
            let deadline = Instant::now() + PATIENCE;
            loop {
                match self.open(opening) {
                    Err(error)
                        if error.kind() == ErrorKind::ExecutableFileBusy
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    outcome => return outcome,
                }
            }
        }
    }

    #[test]
    fn the_target_reaches_the_opener_as_exactly_one_argv_word() {
        let tree = Tree::new();
        let target = tree.in_workspace("my notes.html");
        let opening = tree
            .ground()
            .judge(r#"{"path":"my notes.html"}"#)
            .expect("a file with spaces in its name is still one target");

        tree.recording_opener()
            .open_patiently(&opening)
            .expect("the fake opener is spawned");

        assert_eq!(
            tree.recorded(),
            vec![target.display().to_string()],
            "one word, so a path holding a space is not two arguments"
        );
    }

    #[test]
    fn a_target_holding_shell_metacharacters_reaches_the_opener_whole() {
        let tree = Tree::new();
        let target = tree.in_workspace("notes; touch pwned.html");
        let opening = tree
            .ground()
            .judge(r#"{"path":"notes; touch pwned.html"}"#)
            .expect("the extension is the one that matters, not the punctuation");

        tree.recording_opener()
            .open_patiently(&opening)
            .expect("the fake opener is spawned");

        assert_eq!(
            tree.recorded(),
            vec![target.display().to_string()],
            "no shell ever reads the target, so `;` is a character in a name"
        );
    }

    #[test]
    fn the_host_opener_is_the_one_this_machine_opens_targets_with() {
        assert_eq!(
            Opener::host().program(),
            OsStr::new(HOST_OPENER),
            "the host's opener is a fact about the machine, not about the target"
        );
    }

    #[test]
    fn an_opener_that_cannot_be_spawned_is_an_error_naming_the_program() {
        let tree = Tree::new();
        tree.in_workspace("notes.html");
        let opening = tree
            .ground()
            .judge(r#"{"path":"notes.html"}"#)
            .expect("the file is inside the workspace");

        let failure = Opener::new("/nonexistent/opener")
            .open(&opening)
            .expect_err("there is no such program");

        assert!(
            failure.to_string().contains("/nonexistent/opener"),
            "the error says which program would not run: {failure}"
        );
    }

    #[test]
    fn a_judged_file_is_the_path_it_resolved_to() {
        let tree = Tree::new();
        let target = tree.in_workspace("notes.md");
        let opening = tree
            .ground()
            .judge(&format!(r#"{{"path":"{}"}}"#, target.display()))
            .expect("an absolute path inside the workspace opens");

        assert_eq!(
            opening.to_string(),
            target.display().to_string(),
            "what the opener is handed is what was judged, not the string it came from"
        );
    }
}
