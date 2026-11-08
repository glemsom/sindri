//! What a launch says about itself, measured as bytes: OSC 7501 on the stream that owns a
//! terminal, and nothing at all where there is no terminal to own.
//!
//! Two front ends are read here, because the sequence is written by whoever owns the tty and
//! they own it differently. Batch mode is given a pseudo-terminal on stderr — and, in the
//! other half of each test, a pipe or a file — and the terminal UI is spawned on one as it
//! always is. The wire is a loopback socket serving a recorded reply, so a run really starts,
//! really asks and really stops, and no test needs a network.
//!
//! Nothing here reads the sequence off a function's return value: a status that reaches the
//! stream a terminal reads is the only status there is (§14).

use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rustix::fs::{Mode, OFlags, open};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{Winsize, tcsetwinsize};
use tempfile::TempDir;

/// How long a sequence is given to arrive before a test calls it absent: generous, because a
/// test that is merely slow must not read as a program that never spoke.
const PATIENCE: Duration = Duration::from_secs(10);

/// The answer the recorded wire streams, so a run really decodes one.
const ANSWER: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_stream.sse");

/// A workspace, a config home and a state home, all of the test's own.
struct Ground {
    /// Held so the directories outlive the paths that name them.
    _root: TempDir,
    /// The one directory a launch stands in.
    workspace: PathBuf,
    /// The config home, which holds no file: the key comes from the environment.
    config: PathBuf,
    /// The state home, which holds no session.
    state: PathBuf,
}

impl Ground {
    /// A ground with an empty workspace and empty homes.
    fn new() -> Ground {
        let root = tempfile::tempdir().expect("a temp ground");
        let ground = Ground {
            workspace: root.path().join("workspace"),
            config: root.path().join("config"),
            state: root.path().join("state"),
            _root: root,
        };
        for path in [&ground.workspace, &ground.config, &ground.state] {
            std::fs::create_dir_all(path).expect("a directory");
        }
        ground
    }

    /// A child pointed at this ground and at nothing else: a key it never has to ask for, no
    /// environment of ours to inherit, and §6's escape hatch, because the sandbox is not what
    /// a status is about.
    fn command(&self, wire: &Wire) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sindri"));
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("XDG_CONFIG_HOME", &self.config)
            .env("XDG_STATE_HOME", &self.state)
            .env("HOME", &self.config)
            .current_dir(&self.workspace)
            .env("SINDRI_PROVIDER", "anthropic")
            .env("SINDRI_MODEL", "claude-sonnet-4-6")
            .env("SINDRI_BASE_URL", wire.base_url())
            .env("ANTHROPIC_API_KEY", "sk-ant-the-recorded-one")
            .arg("--yolo-unsafe");
        command
    }
}

/// A wire on a loopback socket: it answers the recorded reply to every request and counts
/// none of them, because what a run asked is `tests/batch.rs`'s subject and a status is not.
struct Wire {
    /// The port the socket listens on.
    port: u16,
    /// One message per request served, so a test can wait for a run to be waiting.
    arrived: Receiver<()>,
}

impl Wire {
    /// A wire that answers at once.
    fn answering() -> Wire {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("a bound address").port();
        let (arrived, arrivals) = channel();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let _ = arrived.send(());
                let _ = answer(&mut stream);
            }
        });
        Wire {
            port,
            arrived: arrivals,
        }
    }

    /// The base URL a run is pointed at, so no request leaves this machine.
    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Waits until a run has asked the wire, which is a run in flight.
    fn wait_for_request(&self) {
        self.arrived
            .recv_timeout(PATIENCE)
            .expect("a request should reach the wire");
    }
}

/// The recorded reply, written as a wire writes one: the request is read first, because a
/// socket nobody reads is a run waiting on a request still in the kernel, and then the reply
/// is sent whole with its length given.
fn answer(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut request = [0u8; 4096];
    let _ = stream.read(&mut request)?;
    let head = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        ANSWER.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(ANSWER.as_bytes())?;
    stream.flush()
}

/// A pseudo-terminal: the slave is the terminal a child is handed, and the master is where
/// the bytes it writes come back.
struct Pty {
    /// The end a test reads off.
    master: File,
    /// The end a child writes to.
    slave: File,
}

impl Pty {
    /// A fresh pseudo-terminal, nobody's controlling terminal, told its size the way a real one
    /// would be.
    ///
    /// A pty that never says how big it is leaves the answer to whoever asks outside this
    /// process: crossterm opens `/dev/tty`, and where there is none — a CI runner — it falls
    /// back to the descriptor itself, which reports the nought by nought a fresh pty holds. A
    /// frame painted into no cells at all draws no glyph and puts the caret nowhere, so a test
    /// that asks where the caret waits was passing on a developer's machine only because the
    /// developer's own terminal answered for the pty (§12).
    fn open() -> Pty {
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).expect("a pseudoterminal");
        grantpt(&master).expect("the slave is granted");
        unlockpt(&master).expect("the slave is unlocked");
        let name = ptsname(&master, Vec::new()).expect("the slave has a name");
        let path = PathBuf::from(name.to_str().expect("a printable name"));
        let slave = open(path.as_path(), OFlags::RDWR | OFlags::NOCTTY, Mode::empty())
            .expect("the slave opens");
        tcsetwinsize(
            &slave,
            Winsize {
                ws_row: 24,
                ws_col: 80,
                ws_xpixel: 800,
                ws_ypixel: 480,
            },
        )
        .expect("the terminal takes its size");
        Pty {
            master: File::from(master),
            slave: File::from(slave),
        }
    }

    /// The slave, as one of a child's standard streams.
    fn child_end(&self) -> File {
        self.slave
            .try_clone()
            .expect("a second handle on the slave")
    }

    /// The master, as the thread that reads the terminal's bytes holds it.
    fn reading_end(&self) -> File {
        self.master
            .try_clone()
            .expect("a second handle on the master")
    }

    /// A key, typed: the bytes a keyboard would put on the wire.
    fn type_bytes(&mut self, keys: &[u8]) {
        self.master.write_all(keys).expect("the key");
        self.master.flush().expect("the key");
    }
}

/// Every byte a child wrote to its terminal, read by a thread of its own so a test never
/// blocks on a sequence that may never come.
struct Drawn {
    /// What has landed so far.
    seen: Arc<Mutex<Vec<u8>>>,
}

impl Drawn {
    /// Watches `master` until the terminal closes.
    fn watch(master: File) -> Drawn {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let landing = Arc::clone(&seen);
        thread::spawn(move || {
            let mut master = master;
            let mut buffer = [0u8; 4096];
            loop {
                match master.read(&mut buffer) {
                    Ok(0) | Err(_) => return,
                    Ok(count) => landing
                        .lock()
                        .expect("the terminal's bytes")
                        .extend_from_slice(&buffer[..count]),
                }
            }
        });
        Drawn { seen }
    }

    /// Everything written so far, byte for byte.
    fn bytes(&self) -> Vec<u8> {
        self.seen.lock().expect("the terminal's bytes").clone()
    }

    /// Waits until `ready` is true of what has been written, and says whether it ever was.
    fn wait_until(&self, mut ready: impl FnMut(&str) -> bool) -> bool {
        let deadline = Instant::now() + PATIENCE;
        while Instant::now() < deadline {
            if ready(&text(&self.bytes())) {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }
}

/// A child under test, killed if a test fails before it ends.
struct Session {
    /// The launch under test, until it is waited on.
    child: Option<Child>,
}

impl Session {
    /// A batch run answering `wire`, with its answer piped so the two streams can be read
    /// apart and `stderr` handed in as whatever the test is about.
    fn batch(ground: &Ground, wire: &Wire, stderr: Stdio) -> Session {
        Session::spawn(
            ground
                .command(wire)
                .args(["-b", "what is this?"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(stderr),
        )
    }

    /// A batch run whose stderr is a terminal, and the terminal it is written to.
    fn batch_on_a_terminal(ground: &Ground, wire: &Wire) -> (Session, Drawn) {
        let pty = Pty::open();
        let drawn = Drawn::watch(pty.reading_end());
        let session = Session::batch(ground, wire, Stdio::from(pty.child_end()));
        (session, drawn)
    }

    /// The terminal UI, on a pseudo-terminal of its own, with the terminal in hand so a test
    /// can type a prompt into it.
    fn frame(ground: &Ground, wire: &Wire) -> (Session, Pty, Drawn) {
        let pty = Pty::open();
        let drawn = Drawn::watch(pty.reading_end());
        let session = Session::spawn(
            ground
                .command(wire)
                .stdin(Stdio::from(pty.child_end()))
                .stdout(Stdio::from(pty.child_end()))
                .stderr(Stdio::from(pty.child_end())),
        );
        (session, pty, drawn)
    }

    /// A batch run on a terminal, spawned from a command a test owns — for the one launch
    /// whose environment the test writes rather than the ground.
    fn batch_with(command: &mut Command, pty: &Pty) -> (Session, Drawn) {
        command
            .args(["-b", "what is this?"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(pty.child_end()));
        (Session::spawn(command), Drawn::watch(pty.reading_end()))
    }

    /// Spawns `command`, or says why it would not start.
    fn spawn(command: &mut Command) -> Session {
        Session {
            child: Some(command.spawn().expect("the binary should start")),
        }
    }

    /// Waits for the launch to end, and reports what it printed and what it ended as. A
    /// child that never ends would read as a broken harness rather than as a program that
    /// failed to quit, so the wait is bounded.
    fn wait(mut self) -> std::process::Output {
        let mut child = self
            .child
            .take()
            .expect("a launch that has not been waited on");
        let deadline = Instant::now() + PATIENCE;
        loop {
            if child.try_wait().expect("the launch's state").is_some() {
                return child
                    .wait_with_output()
                    .expect("the launch's streams are readable");
            }
            assert!(
                Instant::now() < deadline,
                "the launch should have quit by now, and did not"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A stream as a reader sees it.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The column the last frame left the caret on: typing paints one glyph per frame, so where
/// the caret waits is how a test knows the last glyph of a draft landed.
fn caret_column(stream: &str) -> Option<u16> {
    let mut column = None;
    for piece in stream.split("\u{1b}[") {
        let Some((moved, _)) = piece.split_once('H') else {
            continue;
        };
        if let Some((_, read)) = moved.split_once(';')
            && let Ok(parsed) = read.parse()
        {
            column = Some(parsed);
        }
    }
    column
}

/// Every status sequence in `stream`, in the order they were written: each one runs from
/// its opening to the ST that ends it, so a sequence left unterminated is not read as one.
fn sequences(stream: &str) -> Vec<&str> {
    stream
        .match_indices("\x1b]7501;")
        .filter_map(|(at, _)| {
            let sequence = &stream[at..];
            let end = sequence.find("\x1b\\")?;
            Some(&sequence[..end + 2])
        })
        .collect()
}

#[test]
fn a_batch_run_on_a_pipe_is_told_nothing() {
    // A sequence written into a pipe is bytes a script did not ask for and cannot use.
    let ground = Ground::new();
    let wire = Wire::answering();

    let output = Session::batch(&ground, &wire, Stdio::piped()).wait();

    assert_eq!(
        output.status.code(),
        Some(0),
        "the run answered: {}",
        text(&output.stderr)
    );
    assert!(
        text(&output.stderr).is_empty(),
        "a batch run on a pipe says nothing on stderr: {:?}",
        text(&output.stderr)
    );
    assert!(
        !text(&output.stdout).contains("7501"),
        "and the answer on stdout carries none either: {:?}",
        text(&output.stdout)
    );
}

#[test]
fn a_batch_run_whose_stderr_is_a_file_is_told_nothing() {
    // A file is not a terminal either, so a redirected stderr carries no escape
    // sequence at all.
    let ground = Ground::new();
    let wire = Wire::answering();
    let written = ground.state.join("stderr.log");
    let file = File::create(&written).expect("a file to redirect stderr into");

    let output = Session::batch(&ground, &wire, Stdio::from(file)).wait();

    assert_eq!(output.status.code(), Some(0));
    let stderr = std::fs::read(&written).expect("what the run wrote");
    assert!(
        sequences(&text(&stderr)).is_empty(),
        "a file gets no status: {:?}",
        text(&stderr)
    );
}

#[test]
fn a_batch_run_tells_the_terminal_it_is_on_how_the_run_is_going() {
    let ground = Ground::new();
    let wire = Wire::answering();

    let (session, drawn) = Session::batch_on_a_terminal(&ground, &wire);
    let output = session.wait();

    assert_eq!(
        output.status.code(),
        Some(0),
        "the run answered: {}",
        text(&output.stderr)
    );
    assert!(
        drawn.wait_until(|stream| sequences(stream).len() >= 2),
        "the terminal is told: {:?}",
        text(&drawn.bytes())
    );
    let said = text(&drawn.bytes());
    let said = sequences(&said);
    assert!(
        said[0].contains("\"phase\":\"working\"")
            && said[0].contains("\"message\":\"asking the model\""),
        "the run is working while the turn is on the wire: {:?}",
        said[0]
    );
    assert!(
        said[1].contains("\"phase\":\"done\"")
            && said[1].contains("\"message\":\"answered\"")
            && said[1].contains("\"turn\":1")
            && said[1].contains("\"workspace\":\"workspace\""),
        "and done when the answer lands, with the turn and the workspace: {:?}",
        said[1]
    );
}

#[test]
fn a_status_is_written_unqueried_and_a_terminal_that_answers_nothing_still_reads_it() {
    // No detection query, ever. The terminal here answers nothing at all — a
    // pseudo-terminal's slave never replies on its own — so a launch that waited for a
    // yes would hang, and no byte it writes asks anything.
    let ground = Ground::new();
    let wire = Wire::answering();

    let (session, drawn) = Session::batch_on_a_terminal(&ground, &wire);
    let output = session.wait();

    assert_eq!(output.status.code(), Some(0));
    assert!(
        drawn.wait_until(|stream| !sequences(stream).is_empty()),
        "written to a terminal that never answered: {:?}",
        text(&drawn.bytes())
    );
    let written = text(&drawn.bytes());
    for query in ["\x1b[c", "\x1b[>c", "\x1b[6n", "\x1b[?u"] {
        assert!(
            !written.contains(query),
            "no detection query is ever written: {query:?} in {written:?}"
        );
    }
    assert!(
        !sequences(&written)
            .iter()
            .any(|sequence| sequence.contains('?')),
        "and nothing inside a status a terminal would have to answer: {written:?}"
    );
}

#[test]
fn the_status_key_turns_the_telling_off() {
    let ground = Ground::new();
    let wire = Wire::answering();
    let pty = Pty::open();
    let mut command = ground.command(&wire);
    command.env("SINDRI_STATUS", "false");

    let (session, drawn) = Session::batch_with(&mut command, &pty);
    let output = session.wait();

    assert_eq!(output.status.code(), Some(0), "the run is unaffected");
    // The child has quit, so everything it wrote is already in the terminal; the thread that
    // reads it only has to catch up, which is a grace and not a wait for a sequence.
    thread::sleep(Duration::from_millis(200));
    assert!(
        sequences(&text(&drawn.bytes())).is_empty(),
        "with the key off, nothing is written: {:?}",
        text(&drawn.bytes())
    );
}

#[test]
fn a_frame_with_no_run_in_flight_is_told_idle() {
    let ground = Ground::new();
    let wire = Wire::answering();

    let (_session, _pty, drawn) = Session::frame(&ground, &wire);

    assert!(
        drawn.wait_until(|stream| sequences(stream)
            .iter()
            .any(|sequence| sequence.contains("\"phase\":\"idle\""))),
        "the frame tells its terminal that it is idle: {:?}",
        text(&drawn.bytes())
    );
}

#[test]
fn a_frame_tells_its_terminal_that_a_run_is_working_and_then_done() {
    // A status bar in another pane, or a terminal's own tab, draws the run from
    // these lines.
    let ground = Ground::new();
    let wire = Wire::answering();
    let (session, mut pty, drawn) = Session::frame(&ground, &wire);

    assert!(
        drawn.wait_until(|stream| !sequences(stream).is_empty()),
        "the frame is up: {:?}",
        text(&drawn.bytes())
    );
    // A prompt typed, then `Enter` on its own: a carriage return arriving with more of
    // its burst is a paste's line break and sends nothing. No `?` in the draft either:
    // that key opens a screen of its own.
    pty.type_bytes(b"triage this");
    assert!(
        drawn.wait_until(|stream| caret_column(stream) == Some(12)),
        "eleven glyphs typed, so the caret waits one column past the last: {:?}",
        text(&drawn.bytes())
    );
    pty.type_bytes(b"\r");
    wire.wait_for_request();

    assert!(
        drawn.wait_until(|stream| sequences(stream)
            .iter()
            .any(|sequence| sequence.contains("\"phase\":\"working\""))),
        "the run is told as working: {:?}",
        text(&drawn.bytes())
    );
    assert!(
        drawn.wait_until(|stream| sequences(stream)
            .iter()
            .any(|sequence| sequence.contains("\"phase\":\"done\""))),
        "and done when it stopped: {:?}",
        text(&drawn.bytes())
    );
    drop(session);
}
