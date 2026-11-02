//! The host's opener: `xdg-open` spawned detached, with no shell in the argv and
//! all three streams on `/dev/null`, so nothing an opener prints can reach the
//! frame's terminal. A probe window of a quarter second is the only failure
//! signal that is honest — an exit inside it is the opener refusing, and a child
//! still running at its end was asked and had not refused.

use std::ffi::{OsStr, OsString};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use sindri_core::agent::{Opened, Opener};

/// How long the opener is watched before the call is answered `started`. Long
/// enough for a program that refuses a target to say so by exiting, and short
/// enough that a call never spends the command budget waiting.
pub const PROBE: Duration = Duration::from_millis(250);

/// How often the child is looked at inside the window.
const POLL: Duration = Duration::from_millis(10);

/// The opener this machine has: the desktop's own, on the host's `PATH`.
pub struct Desktop {
    program: OsString,
    probe: Duration,
}

impl Desktop {
    pub fn new() -> Desktop {
        Desktop::at("xdg-open")
    }

    fn at(program: impl Into<OsString>) -> Desktop {
        Desktop {
            program: program.into(),
            probe: PROBE,
        }
    }
}

impl Default for Desktop {
    fn default() -> Desktop {
        Desktop::new()
    }
}

impl Opener for Desktop {
    fn open(&self, target: &str) -> Opened {
        spawn(&self.program, target, self.probe)
    }
}

/// One target, handed over: the argv is the opener and the target, so the target
/// is an argument and never a word a shell reads.
fn spawn(program: &OsStr, target: &str, probe: Duration) -> Opened {
    let spawned = Command::new(program)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            return Opened::Failed {
                reason: format!(
                    "{} could not be started: {error}",
                    program.to_string_lossy()
                ),
            };
        }
    };
    match watch(&mut child, probe) {
        Some(opened) => opened,
        None => {
            // Still running when the window closed: the call is over, and a
            // thread reaps the child rather than leaving it behind as a zombie
            // for the rest of the run.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Opened::Started
        }
    }
}

/// The opener watched for one window: `Some` when it ended inside it, `None`
/// when it is still running and the call is already answered.
fn watch(child: &mut Child, probe: Duration) -> Option<Opened> {
    let deadline = Instant::now() + probe;
    loop {
        match child.try_wait() {
            // A success the opener reported is not a human who looked: it is an
            // opener that did not refuse, said in the only other way it can say
            // anything.
            Ok(Some(status)) if status.success() => return Some(Opened::Started),
            Ok(Some(status)) => {
                return Some(Opened::Failed {
                    reason: match status.code() {
                        Some(code) => format!("the opener exited {code}"),
                        None => "the opener was killed by a signal".to_string(),
                    },
                });
            }
            Ok(None) => {}
            Err(error) => {
                return Some(Opened::Failed {
                    reason: format!("the opener could not be watched: {error}"),
                });
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        reason = "a panicking test is how a broken contract reports itself"
    )]

    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::{Desktop, PROBE};
    use sindri_core::agent::{Opened, Opener};

    /// A fake opener: the path of the script and the log it writes.
    struct Faked {
        _root: tempfile::TempDir,
        program: PathBuf,
        log: PathBuf,
    }

    impl Faked {
        /// An opener whose body is this shell script, written with its log's own
        /// absolute path inside it.
        fn saying(body: &str) -> Faked {
            let root = tempfile::TempDir::new().expect("a directory for the fake opener");
            let program = root.path().join("xdg-open");
            let log = root.path().join("log");
            let script = body.replace("$LOG", &log.display().to_string());
            std::fs::write(&program, script).expect("write the fake opener");
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
                .expect("make the fake opener executable");
            Faked {
                _root: root,
                program,
                log,
            }
        }

        fn opener(&self) -> Desktop {
            Desktop::at(self.program.clone())
        }

        fn logged(&self) -> String {
            std::fs::read_to_string(&self.log).unwrap_or_default()
        }
    }

    /// The one fake opener every stream test uses: it writes its own argv and
    /// what each of its three descriptors points at, then says something on
    /// stdout that must never reach sindri's terminal.
    const RECORDING: &str = "#!/bin/sh\n\
        streams=\"\"\n\
        for fd in 0 1 2; do streams=\"$streams\nfd$fd: $(readlink /proc/$$/fd/$fd)\"; done\n\
        {\n\
          echo \"args: $#\"\n\
          for arg in \"$@\"; do echo \"arg: $arg\"; done\n\
          echo \"$streams\"\n\
        } > $LOG\n\
        echo a-marker-no-frame-may-show\n";

    #[test]
    fn the_opener_is_spawned_without_a_shell_and_with_no_streams() {
        let fake = Faked::saying(RECORDING);

        let opened = fake.opener().open("https://example.com/x");

        assert_eq!(
            opened,
            Opened::Started,
            "an opener that said nothing refused nothing"
        );
        let log = fake.logged();
        assert!(
            log.contains("args: 1") && log.contains("arg: https://example.com/x"),
            "the argv is the target and nothing else: {log}"
        );
        for fd in 0..=2 {
            assert!(
                log.contains(&format!("fd{fd}: /dev/null")),
                "descriptor {fd} is on /dev/null, so no opener can write into the frame: {log}"
            );
        }
    }

    #[test]
    fn the_target_is_one_argument_even_when_it_holds_shell_words() {
        let fake = Faked::saying(RECORDING);
        // A mark a shell would leave: the spawner's own working directory is the
        // one a `sh -c` would have written it in.
        let mark = "hoisted-by-a-shell";
        let target = &format!("https://example.com/a; touch {mark}")[..];

        let opened = fake.opener().open(target);

        assert_eq!(opened, Opened::Started);
        let log = fake.logged();
        assert!(
            log.contains(&format!("arg: {target}")),
            "the target arrives whole, as one argv word: {log}"
        );
        let hoisted = std::path::Path::new(mark).exists();
        let _ = std::fs::remove_file(mark);
        assert!(!hoisted, "no shell read the target: nothing in it ran");
    }

    #[test]
    fn a_file_path_is_one_argument_and_never_a_word_a_shell_reads() {
        let fake = Faked::saying(RECORDING);
        let root = tempfile::TempDir::new().expect("a place for the file");
        let file = root.path().join("report;touch hoisted-by-a-path.html");
        std::fs::write(&file, "<html>a report</html>").expect("a report");
        let target = file.display().to_string();

        let opened = fake.opener().open(&target);

        assert_eq!(opened, Opened::Started);
        let log = fake.logged();
        assert!(
            log.contains("args: 1") && log.contains(&format!("arg: {target}")),
            "the path arrives whole, as one argv word: {log}"
        );
        let hoisted = std::path::Path::new("hoisted-by-a-path").exists();
        let _ = std::fs::remove_file("hoisted-by-a-path");
        assert!(!hoisted, "no shell read the path: nothing in it ran");
    }

    #[test]
    fn an_opener_that_exits_non_zero_inside_the_window_fails_with_its_status() {
        let fake = Faked::saying("#!/bin/sh\nexit 4\n");

        let opened = fake.opener().open("https://example.com/x");

        assert_eq!(
            opened,
            Opened::Failed {
                reason: "the opener exited 4".to_string()
            },
            "the opener's own refusal, reported as it gave it"
        );
    }

    #[test]
    fn an_opener_still_running_when_the_window_closes_is_started() {
        let fake = Faked::saying("#!/bin/sh\nexec sleep 1\n");
        let started = Instant::now();

        let opened = fake.opener().open("https://example.com/x");
        let took = started.elapsed();

        assert_eq!(opened, Opened::Started);
        assert!(
            took < Duration::from_millis(500),
            "the call does not wait for a browser, so it does not spend the command budget: {took:?}"
        );
        assert!(
            took >= PROBE,
            "the probe window is the wait, and it is a quarter second: {took:?}"
        );
    }

    #[test]
    fn an_opener_that_cannot_be_started_is_failed_naming_what_was_missing() {
        let fake = Faked::saying(RECORDING);
        std::fs::remove_file(&fake.program).expect("the opener is gone");

        let opened = fake.opener().open("https://example.com/x");

        let Opened::Failed { reason } = opened else {
            panic!("a missing opener is a failure, not a start: {opened:?}");
        };
        assert!(
            reason.contains("xdg-open") && reason.contains("could not be started"),
            "the reason names the opener that was missing: {reason}"
        );
    }
}
