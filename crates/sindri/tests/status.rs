//! What a launch says about itself, measured as bytes: OSC 7501 on the stream that owns a terminal, and nothing
//! at all where there is no terminal to own. Batch mode gets a pseudo-terminal on stderr — a pipe or a file in
//! the other half of each test — the UI is spawned on one, and the wire is a loopback socket.

mod frame; // one paint-and-read for every test

use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::process::CommandExt;
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

/// How long a sequence is given to arrive before a test calls it absent: generous, because a test that is merely
/// slow must not read as a program that never spoke.
const PATIENCE: Duration = Duration::from_secs(10);

/// The size the pty is told it has, and so the size every frame here is read at: the two cannot
/// disagree about what a launch drew.
const COLS: u16 = 80;
const ROWS: u16 = 24;

const ANSWER: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_stream.sse");

/// The recorded reply that asks for `bash ls` and answers nothing: the turn the run waits on.
const TOOL_USE: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_tool_use.sse");

struct Ground {
    /// Held so the directories outlive the paths that name them.
    _root: TempDir,
    workspace: PathBuf,
    config: PathBuf,
    state: PathBuf,
}

impl Ground {
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

    /// A child pointed at this ground and at nothing else: a key it never has to ask for, no environment of
    /// ours, and the escape hatch, because the sandbox is not what a status is about.
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

struct Wire {
    port: u16,
    arrived: Receiver<()>,
}

impl Wire {
    fn answering() -> Wire {
        Wire::scripted(&[ANSWER])
    }

    /// A wire that answers each request with the next body of `bodies`, and the last one for every
    /// request past it, so a test scripts the turn that asks for a call and the one after it.
    fn scripted(bodies: &'static [&'static str]) -> Wire {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("a bound address").port();
        let (arrived, arrivals) = channel();
        thread::spawn(move || {
            for (index, stream) in listener.incoming().enumerate() {
                let Ok(mut stream) = stream else { break };
                read_request(&mut stream);
                let _ = arrived.send(());
                let body = bodies[index.min(bodies.len().saturating_sub(1))];
                let _ = answer(&mut stream, body);
            }
        });
        Wire {
            port,
            arrived: arrivals,
        }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn wait_for_request(&self) {
        self.arrived
            .recv_timeout(PATIENCE)
            .expect("a request should reach the wire");
    }
}

/// The whole request, head and the body it promises read before it is answered: a socket nobody finishes
/// reading is a run waiting on its own request still in the kernel.
fn read_request(stream: &mut TcpStream) {
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    while split(&request).is_none() {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => request.extend_from_slice(&buffer[..read]),
        }
    }
    let (head, mut body) = split(&request).expect("a head, once the blank line is read");
    let promised = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        })
        .unwrap_or(0);
    while body < promised {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                request.extend_from_slice(&buffer[..read]);
                body += read;
            }
        }
    }
}

fn split(request: &[u8]) -> Option<(String, usize)> {
    let end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&request[..end]).into_owned();
    Some((head, request.len() - (end + 4)))
}

/// The recorded reply, as a wire writes one.
fn answer(stream: &mut TcpStream, body: &str) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

struct Pty {
    master: File,
    slave: File,
}

impl Pty {
    /// A fresh pseudo-terminal, nobody's controlling terminal, told its size: crossterm opens `/dev/tty`, and
    /// where there is none — a CI runner — it falls back to the descriptor, which reports the nought by nought
    /// a fresh pty holds, so a caret assertion would pass only on a developer's machine.
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
                ws_row: ROWS,
                ws_col: COLS,
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

    fn child_end(&self) -> File {
        self.slave
            .try_clone()
            .expect("a second handle on the slave")
    }

    fn reading_end(&self) -> File {
        self.master
            .try_clone()
            .expect("a second handle on the master")
    }

    fn type_bytes(&mut self, keys: &[u8]) {
        self.master.write_all(keys).expect("the key");
        self.master.flush().expect("the key");
    }
}

struct Drawn {
    seen: Arc<Mutex<Vec<u8>>>,
}

impl Drawn {
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

    fn bytes(&self) -> Vec<u8> {
        self.seen.lock().expect("the terminal's bytes").clone()
    }

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

struct Session {
    child: Option<Child>,
}

impl Session {
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

    fn batch_on_a_terminal(ground: &Ground, wire: &Wire) -> (Session, Drawn) {
        let pty = Pty::open();
        let drawn = Drawn::watch(pty.reading_end());
        let session = Session::batch(ground, wire, Stdio::from(pty.child_end()));
        (session, drawn)
    }

    fn frame(ground: &Ground, wire: &Wire) -> (Session, Pty, Drawn) {
        Session::frame_running(&mut ground.command(wire))
    }

    /// A frame on a command the test shaped itself: the one it stands on is the terminal it is read
    /// as, and a frame asks that terminal what it speaks before it wears a title.
    fn frame_running(command: &mut Command) -> (Session, Pty, Drawn) {
        let pty = Pty::open();
        let drawn = Drawn::watch(pty.reading_end());
        let session = Session::spawn(
            command
                .stdin(Stdio::from(pty.child_end()))
                .stdout(Stdio::from(pty.child_end()))
                .stderr(Stdio::from(pty.child_end())),
        );
        (session, pty, drawn)
    }

    fn batch_with(command: &mut Command, pty: &Pty) -> (Session, Drawn) {
        command
            .args(["-b", "what is this?"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(pty.child_end()));
        (Session::spawn(command), Drawn::watch(pty.reading_end()))
    }

    fn spawn(command: &mut Command) -> Session {
        // A child sharing this session would ask a controlling terminal for a size and paint a
        // frame the pty never heard of.
        unsafe {
            command.pre_exec(|| {
                rustix::process::setsid().map_err(std::io::Error::from)?;
                Ok(())
            });
        }
        Session {
            child: Some(command.spawn().expect("the binary should start")),
        }
    }

    /// Waits for the launch to end; the wait is bounded, so a child that never quits fails rather than hangs.
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

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The titles the window wore, in the order it wore them.
fn titles(stream: &str) -> Vec<&str> {
    stream
        .match_indices("\x1b]2;")
        .filter_map(|(at, _)| {
            let sequence = &stream[at..];
            let end = sequence.find("\x1b\\")?;
            Some(&sequence[..end + 2])
        })
        .collect()
}

/// Types `prompt` into the frame and sends it: the caret is read where the band's two-column gutter
/// puts it, so the bytes have reached the composer before the carriage return does — one arriving with
/// more of its burst is a paste's line break and sends nothing.
fn send_prompt(pty: &mut Pty, drawn: &Drawn, prompt: &str) {
    let column =
        u16::try_from(prompt.chars().count() + 2).expect("a prompt narrower than the frame");
    pty.type_bytes(prompt.as_bytes());
    assert!(
        drawn.wait_until(|stream| frame::read(stream, COLS, ROWS)
            .caret()
            .is_some_and(|(_, at)| at == column)),
        "{prompt:?} is in the composer: {:?}",
        text(&drawn.bytes())
    );
    pty.type_bytes(b"\r");
}

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
fn a_run_that_answered_reports_the_documents_it_wrote() {
    let ground = Ground::new();
    let wire = Wire::answering();

    let answered = Session::batch(&ground, &wire, Stdio::piped()).wait();
    assert_eq!(
        answered.status.code(),
        Some(0),
        "the run answered: {}",
        text(&answered.stderr)
    );

    let mut command = ground.command(&wire);
    command
        .arg("sessions")
        .arg("show")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let shown = Session::spawn(&mut command).wait();

    assert_eq!(
        shown.status.code(),
        Some(0),
        "the session the run wrote is one `show` reads: {}",
        text(&shown.stderr)
    );
    let read = text(&shown.stdout);
    for expected in [
        "status 2 documents",
        "  working workspace turn 1: asking the model",
        "  done workspace turn 1: answered",
    ] {
        assert!(
            read.contains(expected),
            "`sessions show` reports {expected:?}, matched as a stretch because the lines around it \
             are the session's own: {read:?}"
        );
    }
}

#[test]
fn a_batch_run_whose_stderr_is_a_file_is_told_nothing() {
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
    // No detection query, ever: a pseudo-terminal's slave never answers on its own, so a launch that waited
    // for a yes would hang.
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
    // The child has quit, so everything it wrote is already in the terminal; the thread that reads it only
    // has to catch up.
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
    let ground = Ground::new();
    let wire = Wire::answering();
    let (session, mut pty, drawn) = Session::frame(&ground, &wire);

    assert!(
        drawn.wait_until(|stream| !sequences(stream).is_empty()),
        "the frame is up: {:?}",
        text(&drawn.bytes())
    );
    send_prompt(&mut pty, &drawn, "triage this");
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

#[test]
fn a_batch_run_whose_turn_asks_for_a_command_is_told_it_is_blocked_between_working_and_done() {
    let ground = Ground::new();
    let wire = Wire::scripted(&[TOOL_USE, ANSWER]);

    let (session, drawn) = Session::batch_on_a_terminal(&ground, &wire);
    let output = session.wait();

    assert_eq!(
        output.status.code(),
        Some(0),
        "the run answered after its command: {}",
        text(&output.stderr)
    );
    let seen = text(&drawn.bytes());
    let said = sequences(&seen);
    let blocked = said
        .iter()
        .position(|sequence| sequence.contains("\"phase\":\"blocked\""))
        .unwrap_or_else(|| panic!("the run is told as blocked while it waits: {seen:?}"));
    assert!(
        said[blocked].contains("\"message\":\"waiting for a command\""),
        "and says what it is waiting for: {:?}",
        said[blocked]
    );
    assert!(
        said[..blocked]
            .iter()
            .any(|sequence| sequence.contains("\"phase\":\"working\"")),
        "the blocked word comes after the run was told it was asking: {said:?}"
    );
    assert!(
        said[blocked + 1..]
            .iter()
            .any(|sequence| sequence.contains("\"phase\":\"done\"")),
        "and before the one that says it answered: {said:?}"
    );
}

#[test]
fn a_frame_says_in_its_title_the_phase_its_run_was_handed() {
    let ground = Ground::new();
    let wire = Wire::scripted(&[TOOL_USE, ANSWER]);
    // A terminal that speaks the sequences a window is drawn with, and names none of the picture
    // protocols: this is a test about a title and nothing else.
    let mut command = ground.command(&wire);
    command.env("TERM", "alacritty");
    let (session, mut pty, drawn) = Session::frame_running(&mut command);

    assert!(
        drawn.wait_until(|stream| !titles(stream).is_empty()),
        "the frame is up: {:?}",
        text(&drawn.bytes())
    );
    send_prompt(&mut pty, &drawn, "list the files");
    wire.wait_for_request();

    assert!(
        drawn.wait_until(|stream| titles(stream)
            .iter()
            .any(|title| title.contains("workspace \u{2014} blocked"))),
        "the title wears the blocked phase the run was handed: {:?}",
        text(&drawn.bytes())
    );
    assert!(
        drawn.wait_until(|stream| {
            let worn = titles(stream);
            worn.iter()
                .position(|title| title.contains("workspace \u{2014} blocked"))
                .is_some_and(|blocked| {
                    worn[blocked + 1..]
                        .iter()
                        .any(|title| title.contains("workspace \u{2014} done"))
                })
        }),
        "and a later title says it is done: {:?}",
        text(&drawn.bytes())
    );
    drop(session);
}
