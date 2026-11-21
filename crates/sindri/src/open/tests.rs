use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sindri_core::config::Config;
use sindri_core::open::{Ground, Handover, Opener};
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::turn::Exit;
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
    fn recording_opener(&self) -> HostOpener {
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
        HostOpener::new(script)
    }

    /// Waits until the script of the test has written `name` into the tree.
    fn wait_for(&self, name: &str) {
        let path = self.root.path().join(name);
        let deadline = Instant::now() + PATIENCE;
        while !path.exists() {
            assert!(
                Instant::now() < deadline,
                "the fake opener never wrote {}",
                path.display()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// What a script wrote into the tree, once the last of it is down: the file exists before
    /// the shell has written a byte to it.
    fn wrote(&self, name: &str) -> String {
        let path = self.root.path().join(name);
        let deadline = Instant::now() + PATIENCE;
        loop {
            let written = std::fs::read_to_string(&path).unwrap_or_default();
            if written.ends_with('\n') {
                return written;
            }
            assert!(
                Instant::now() < deadline,
                "the fake opener never finished writing {}",
                path.display()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// A fake opener whose whole body is `body`, so a test says what the
    /// program it spawns does.
    fn script(&self, name: &str, body: &str) -> HostOpener {
        let script = self.root.path().join(name);
        std::fs::write(&script, format!("#!/bin/sh\n{body}")).expect("the script");
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("the script is executable");
        HostOpener::new(script)
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

impl HostOpener {
    /// Spawns the fake opener, retrying while the script it was handed is
    /// still open for writing elsewhere in this process: tests run on forked
    /// threads, so a fixture written a moment ago can be `ETXTBSY`.
    fn open_patiently(&self, opening: &Opening) -> Handover {
        let deadline = Instant::now() + PATIENCE;
        loop {
            match Opener::open(self, opening) {
                Handover::Failed { words, .. }
                    if words.contains("busy") && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                handover => return handover,
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

    assert_eq!(
        tree.recording_opener().open_patiently(&opening),
        Handover::Opened,
        "the fake opener exits 0 once it has written its argv"
    );

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

    assert_eq!(
        tree.recording_opener().open_patiently(&opening),
        Handover::Opened,
        "the fake opener exits 0 once it has written its argv"
    );

    assert_eq!(
        tree.recorded(),
        vec![target.display().to_string()],
        "no shell ever reads the target, so `;` is a character in a name"
    );
}

#[test]
fn the_host_opener_is_the_one_this_machine_opens_targets_with() {
    assert_eq!(
        HostOpener::host().program(),
        OsStr::new(HOST_OPENER),
        "the host's opener is a fact about the machine, not about the target"
    );
}

#[test]
fn an_opener_that_cannot_be_spawned_is_reported_failed_naming_the_program() {
    let tree = Tree::new();
    tree.in_workspace("notes.html");
    let opening = tree
        .ground()
        .judge(r#"{"path":"notes.html"}"#)
        .expect("the file is inside the workspace");

    let handover = Opener::open(&HostOpener::new("/nonexistent/opener"), &opening);

    let Handover::Failed { exit, words } = handover else {
        panic!("there is no such program, so nothing opened: {handover:?}");
    };
    assert_eq!(
        exit,
        Exit::Code(1),
        "a call that opened nothing settles under a cross"
    );
    assert!(
        words.contains("/nonexistent/opener"),
        "and says which program the host was asked for: {words}"
    );
}

#[test]
fn the_opener_is_spawned_with_the_readers_own_environment() {
    let tree = Tree::new();
    tree.in_workspace("notes.html");
    let opening = tree
        .ground()
        .judge(r#"{"path":"notes.html"}"#)
        .expect("the file is inside the workspace");
    let opener = tree.script(
        "fake-opener",
        &format!(
            "printf '%s\\n' \"$PATH\" > '{}'\n",
            tree.root.path().join("path").display()
        ),
    );

    assert_eq!(
        opener.open_patiently(&opening),
        Handover::Opened,
        "the opener that wrote its environment exited 0"
    );
    assert_eq!(
        tree.wrote("path").trim(),
        std::env::var("PATH").expect("the test process has a PATH"),
        "the opener runs in the launch's own environment, which is how the display reaches it"
    );
}

#[test]
fn a_browser_that_outlives_the_call_is_not_killed_and_is_reaped_when_it_ends() {
    let tree = Tree::new();
    tree.in_workspace("notes.html");
    let opening = tree
        .ground()
        .judge(r#"{"path":"notes.html"}"#)
        .expect("the file is inside the workspace");
    let opener = tree
        .script(
            "fake-opener",
            &format!(
                "printf '%s\n' \"$$\" > '{}'\nsleep 0.5\ntouch '{}'\n",
                tree.root.path().join("pid").display(),
                tree.root.path().join("ended").display()
            ),
        )
        .within(Duration::from_millis(100));

    assert_eq!(
        opener.open_patiently(&opening),
        Handover::Handed,
        "the browser is still running when the call settles"
    );

    let lived = tree.wrote("pid");
    let child = lived.trim();
    assert!(
        Path::new(&format!("/proc/{child}")).exists(),
        "the handover left the browser running under the pid it wrote: {child}"
    );

    tree.wait_for("ended");
    let deadline = Instant::now() + PATIENCE;
    while Path::new(&format!("/proc/{child}")).exists() {
        assert!(
            Instant::now() < deadline,
            "the browser ended but nothing reaped it, so {child} is still a zombie"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn an_opener_still_running_when_its_grace_runs_out_is_reported_handed() {
    let tree = Tree::new();
    tree.in_workspace("notes.html");
    let opening = tree
        .ground()
        .judge(r#"{"path":"notes.html"}"#)
        .expect("the file is inside the workspace");
    let opener = tree
        .script("fake-opener", "sleep 1\n")
        .within(Duration::from_millis(100));

    assert_eq!(
        opener.open_patiently(&opening),
        Handover::Handed,
        "a browser that keeps the opener alive hands the target over and fails nothing"
    );
}

#[test]
fn an_opener_that_exits_non_zero_is_reported_failed_in_one_line_of_its_own_stderr() {
    let tree = Tree::new();
    tree.in_workspace("notes.html");
    let opening = tree
        .ground()
        .judge(r#"{"path":"notes.html"}"#)
        .expect("the file is inside the workspace");
    let opener = tree.script(
        "fake-opener",
        "printf '%s\n' 'no handler for .html' 'try a browser' >&2\nexit 3\n",
    );

    assert_eq!(
        opener.open_patiently(&opening),
        Handover::Failed {
            exit: Exit::Code(3),
            words: String::from("no handler for .html"),
        },
        "the row carries the opener's own exit and the first line it wrote"
    );
}

#[test]
fn an_opener_that_exits_zero_is_reported_opened() {
    let tree = Tree::new();
    tree.in_workspace("notes.html");
    let opening = tree
        .ground()
        .judge(r#"{"path":"notes.html"}"#)
        .expect("the file is inside the workspace");

    assert_eq!(
        tree.recording_opener().open_patiently(&opening),
        Handover::Opened,
        "an opener that exited 0 opened the target"
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
