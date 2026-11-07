//! The terminal UI's contract with a terminal, measured where a person's eyes are:
//! a pseudo-terminal is opened here, the binary is spawned on its slave, and the
//! bytes it draws — and the bytes typing a key makes it give back — are read off
//! the master as the sequences they are.
//!
//! Every child gets `env_clear` and homes of its own holding a key, so no launch
//! stops to paste one and nothing of this process's environment — least of all a
//! real key — can reach the binary.
//!
//! A run is measured here too: a prompt typed and sent starts one against a wire on a
//! loopback socket that holds its reply until a test lets it go, so a request is in
//! flight while the test types, and `Ctrl-C` and an outside `SIGINT` are each proved to
//! stop it where §4 says they do (§4, §12).

use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rustix::fs::{Mode, OFlags, open};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{InputModes, LocalModes, Termios, Winsize, tcgetattr, tcsetwinsize};
use tempfile::TempDir;

/// How long a frame is given to arrive before a test calls it absent: generous,
/// because a test that is merely slow must not read as a program that never drew.
const PATIENCE: Duration = Duration::from_secs(10);

/// The answer the wire streams, as a reply was recorded: an Anthropic event stream of
/// two text fragments, so a run really decodes one and the frame really draws it.
const ANSWER: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_stream.sse");

/// The word the recorded stream says, split the way it streams: a frame paints the cells
/// a fragment changed, so a test looks for both halves rather than for one run of words.
const ANSWER_WORDS: [&str; 2] = ["Hello", "world"];

/// `CSI 16 t`, the one question a picture asks a terminal: report the size of a cell in
/// pixels. A terminal answers `CSI 6 ; height ; width t`, and the launch a test opens here
/// is never answered, so it settles for the nominal cell — which is exactly what makes the
/// question visible as the bytes it is (§12, ADR-0003).
const ASK_CELL: &[u8] = b"\x1b[16t";

/// A recorded Anthropic `GET /v1/models` catalogue: the names a wire really served, which is
/// the only place a `/model` screen's names may come from (§10).
const SERVED: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_models.json");

/// The two identifiers the recorded catalogue holds, named once so an assertion reads as the
/// wire's list. The first is also the model the launch is configured with, so a test that
/// writes the second is a test that wrote a name the run was not already asking.
const FIRST_SERVED: &str = "claude-sonnet-4-6";
const SECOND_SERVED: &str = "claude-haiku-4-5";

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

    /// A child pointed at this ground and at nothing else: a key it never has to
    /// ask for, and no environment of ours to inherit.
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sindri"));
        command
            .env_clear()
            .env("XDG_CONFIG_HOME", &self.config)
            .env("XDG_STATE_HOME", &self.state)
            .env("HOME", &self.config)
            .current_dir(&self.workspace)
            .env("SINDRI_PROVIDER", "anthropic")
            .env("SINDRI_MODEL", "claude-sonnet-4-6")
            .env("ANTHROPIC_API_KEY", "sk-ant-the-recorded-one");
        command
    }
}

/// A wire on a loopback socket, for the one thing a terminal UI test asks of one: that a
/// request is in flight while the test types, and that it lands once the test lets it go.
///
/// It is `tests/batch.rs`'s socket cut down to a single held reply: what a wire's own
/// decoding promises is that suite's subject, and here it is only the thing a run waits
/// on — which is where a cancellation has to arrive (§4).
struct Wire {
    /// The port the socket listens on.
    port: u16,
    /// One message per request served, so a test knows a run is waiting on the wire.
    arrived: Receiver<()>,
    /// One message released, so the wire answers and the run it was in flight with ends.
    released: Sender<()>,
    /// Every request read, in the order the runs made them: which prompt a run asked is
    /// read off the request itself, which is how a test proves the order prompts ran in
    /// rather than trusting the frame to have painted them in that order (§12).
    heard: Arc<Mutex<Vec<String>>>,
}

impl Wire {
    /// A wire that answers nothing until it is released.
    fn holding() -> Wire {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("a bound address").port();
        let (arrived, arrivals) = channel();
        let (release, released) = channel();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let listening = Arc::clone(&heard);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let request = read_request(&mut stream).unwrap_or_default();
                // A wire asked what it serves answers at once: a listing is not a run, and
                // nothing waits on one the way a test waits on a reply it is holding (§10).
                let listing = request.starts_with("GET ");
                listening.lock().expect("the wire's requests").push(request);
                if listing {
                    let _ = catalogue(&mut stream);
                    continue;
                }
                let _ = arrived.send(());
                let _ = released.recv_timeout(PATIENCE);
                let _ = answer(&mut stream);
            }
        });
        Wire {
            port,
            arrived: arrivals,
            released: release,
            heard,
        }
    }

    /// The base URL a run is pointed at, so no request leaves this machine.
    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Waits until a run has a request in flight.
    fn wait_for_request(&self) {
        self.arrived
            .recv_timeout(PATIENCE)
            .expect("a request should reach the wire");
    }

    /// Lets the reply go, so the request a run is in flight with ends.
    fn release(&self) {
        let _ = self.released.send(());
    }

    /// Waits until a request carrying `prompt` reaches the wire, and says whether one did:
    /// which prompt ran is the prompt the run asked for.
    fn wait_for_prompt(&self, prompt: &str) -> bool {
        let deadline = Instant::now() + PATIENCE;
        while Instant::now() < deadline {
            if self
                .requests()
                .iter()
                .any(|request| request.contains(prompt))
            {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// Every request the wire has read, in the order the runs made them.
    fn requests(&self) -> Vec<String> {
        self.heard.lock().expect("the wire's requests").clone()
    }
}

/// The whole of an HTTP request: the head, then the body its `content-length` promised.
/// A request read to its end is a request the socket can answer, and a run waiting on a
/// reply has one nothing has read the body of.
fn read_request(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&request[..end]).into_owned();
            let promised = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if request.len() >= end + 4 + promised {
                return Ok(String::from_utf8_lossy(&request).into_owned());
            }
        }
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return Ok(String::from_utf8_lossy(&request).into_owned()),
            Ok(read) => request.extend_from_slice(&buffer[..read]),
        }
    }
}

/// The recorded reply, written as a wire writes one: its length is given so the client
/// does not wait for a close to find the end of it.
fn answer(stream: &mut TcpStream) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        ANSWER.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(ANSWER.as_bytes())?;
    stream.flush()
}

/// The recorded catalogue, written as the listing route answers one: the names a wire says
/// it serves, in its own order, so a `/model` screen has something to rank (§10).
fn catalogue(stream: &mut TcpStream) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        SERVED.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(SERVED.as_bytes())?;
    stream.flush()
}

/// A pseudo-terminal: the slave is the terminal a child is handed, and the master
/// is where its screen comes back and where a test types.
struct Pty {
    /// The end a test reads frames off and writes keys to.
    master: File,
    /// The end a child reads keys from and draws its frames on.
    slave: File,
}

impl Pty {
    /// A fresh pseudo-terminal, nobody's controlling terminal, told its size the way a real one
    /// would be.
    ///
    /// A pty that never says how big it is leaves the answer to whoever asks outside this
    /// process: crossterm opens `/dev/tty`, and where there is none — a CI runner — it falls
    /// back to the descriptor itself, which reports the nought by nought a fresh pty holds. A
    /// frame painted into no cells draws no glyph and puts the caret nowhere, so every test
    /// that reads the caret was passing on a developer's machine only because the developer's
    /// own terminal answered for the pty (§12).
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
            // The cells are said, the pixels are not: a terminal that reports no pixel
            // fields is the shape `the_cell_size_is_asked_of_the_terminal_once` needs, and it
            // is the common case — most terminals fill neither field.
            Winsize {
                ws_row: 40,
                ws_col: 120,
                ws_xpixel: 0,
                ws_ypixel: 0,
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

    /// The terminal's state, as the program left it: the flags are the terminal's,
    /// not a fd's, so the parent reads what the child installed.
    fn attrs(&self) -> Termios {
        tcgetattr(&self.slave).expect("the terminal's state")
    }

    /// A key, typed: the bytes a keyboard would put on the wire.
    fn type_bytes(&mut self, keys: &[u8]) {
        self.master.write_all(keys).expect("the key");
        self.master.flush().expect("the key");
    }
}

/// What a child has drawn so far, read off the master by a thread of its own so a
/// test never blocks on a frame that may never come.
struct Drawn {
    /// Every byte the child has written to its terminal, in order.
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
                // A closed terminal — or one whose child has gone — is the end of
                // the frame, not a failure: nothing more will ever be drawn.
                match master.read(&mut buffer) {
                    Ok(0) | Err(_) => return,
                    Ok(count) => landing
                        .lock()
                        .expect("the frame")
                        .extend_from_slice(&buffer[..count]),
                }
            }
        });
        Drawn { seen }
    }

    /// Everything drawn so far, byte for byte.
    fn bytes(&self) -> Vec<u8> {
        self.seen.lock().expect("the frame").clone()
    }

    /// Everything drawn so far, as a reader sees it.
    fn text(&self) -> String {
        text(&self.bytes())
    }

    /// Waits until what has been drawn says `ready`, and says whether it ever did: a
    /// frame is only as caught up as the bytes that reached the terminal, so a test waits
    /// for the fact it needs rather than for a sleep and a hope.
    fn wait_until(&self, mut ready: impl FnMut(&str) -> bool) -> bool {
        let deadline = Instant::now() + PATIENCE;
        while Instant::now() < deadline {
            if ready(&self.text()) {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// Waits until `needle` has been drawn, and says whether it ever was.
    fn wait_for(&self, needle: &[u8]) -> bool {
        let deadline = Instant::now() + PATIENCE;
        while Instant::now() < deadline {
            if contains(&self.bytes(), needle) {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }
}

/// A launch on a terminal, held so a test that fails early still ends it: a child
/// left holding a pseudo-terminal would keep the harness waiting for a frame.
struct Session {
    /// The launch under test.
    child: Child,
}

impl Session {
    /// A launch of the binary on its own pseudo-terminal.
    fn open(ground: &Ground) -> (Session, Pty, Drawn) {
        Session::starting(ground.command())
    }

    /// A launch as a filter: the prompt on its own command line and no screen at all, on a
    /// terminal so that a probe would have somewhere to land if anything asked (§3).
    fn filtering(ground: &Ground, wire: &Wire) -> (Session, Pty, Drawn) {
        let mut command = ground.command();
        command
            .arg("-b")
            .arg("what does the wire say")
            .arg("--yolo-unsafe")
            .env("SINDRI_BASE_URL", wire.base_url());
        Session::starting(command)
    }

    /// A launch that can run a prompt: the ground's own child, pointed at `wire` so its
    /// run asks a socket on this machine, and taking §6's escape hatch because the
    /// sandbox is not what a cancellation test is about — the kill reaches the process
    /// group either way, which `tests/bash.rs` proves where the boundary is the subject.
    fn asking(ground: &Ground, wire: &Wire) -> (Session, Pty, Drawn) {
        let mut command = ground.command();
        command
            .arg("--yolo-unsafe")
            .env("SINDRI_BASE_URL", wire.base_url());
        Session::starting(command)
    }

    /// A launch of `command` on its own pseudo-terminal.
    fn starting(mut command: Command) -> (Session, Pty, Drawn) {
        let pty = Pty::open();
        let child = command
            .stdin(Stdio::from(pty.child_end()))
            .stdout(Stdio::from(pty.child_end()))
            .stderr(Stdio::from(pty.child_end()))
            .spawn()
            .expect("the binary should start");
        let drawn = Drawn::watch(
            pty.master
                .try_clone()
                .expect("a second handle on the master"),
        );
        (Session { child }, pty, drawn)
    }

    /// Sends `SIGINT` to the launch, as `kill -INT` would: the user's word from outside the
    /// keyboard, which is the other door into the one cancellation (§4).
    fn interrupt(&self) -> bool {
        Command::new("kill")
            .args(["-INT", &self.child.id().to_string()])
            .status()
            .expect("`kill` should run")
            .success()
    }

    /// Waits for the launch to end, and reports what it ended as. A child that never
    /// ends is a test that never finishes, which reads as a broken harness rather
    /// than as a program that failed to quit, so the wait is bounded.
    fn wait(mut self) -> std::process::ExitStatus {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(status) = self.child.try_wait().expect("the launch's state") {
                return status;
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
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A stream as a reader sees it.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Whether `haystack` holds `needle`, as bytes rather than as glyphs.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// §12 and ADR-0003: the pixels in a cell are the one thing a launch asks the terminal
/// rather than draws into it, so it asks once, before the event loop, and every frame that
/// follows reads the answer it kept.
#[test]
fn the_cell_size_is_asked_of_the_terminal_once_however_many_frames_it_paints() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    // Three keys and three frames: a launch that asked once per frame would have asked
    // four times by the time the third digit is on the screen.
    pty.type_bytes(b"777");
    assert!(
        drawn.wait_until(|drawn| drawn.matches('7').count() >= 3),
        "the draft is drawn a key at a time, so three keys are three frames: {:?}",
        drawn.text()
    );
    pty.type_bytes(&[0x11]);
    session.wait();

    let asked = drawn
        .bytes()
        .windows(ASK_CELL.len())
        .filter(|window| *window == ASK_CELL)
        .count();
    assert_eq!(
        asked,
        1,
        "the terminal was asked for the cell's pixels once: {:?}",
        drawn.text()
    );
    let bytes = drawn.bytes();
    assert!(
        bytes
            .windows(ASK_CELL.len())
            .position(|window| window == ASK_CELL)
            .zip(bytes.windows(10).position(|window| window == b"Enter send"))
            .is_some_and(|(asked_at, boot_at)| asked_at < boot_at),
        "and it was asked before the first frame was painted, so the frame was drawn at \
         an answer rather than at a guess"
    );
}

/// The other half of ADR-0003's rule: a cell's pixels are the terminal UI's fact, so a run
/// as a filter — which has a terminal here, and uses it only for its answer — asks nothing
/// and pays for nothing.
#[test]
fn a_run_as_a_filter_never_asks_the_terminal_its_cell_size() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, _pty, drawn) = Session::filtering(&ground, &wire);

    wire.wait_for_request();
    wire.release();
    let status = session.wait();

    assert_eq!(status.code(), Some(0), "the filter ran: {:?}", drawn.text());
    assert!(
        drawn.wait_for(b"Hello"),
        "its answer is what it wrote: {:?}",
        drawn.text()
    );
    assert!(
        !contains(&drawn.bytes(), ASK_CELL),
        "a filter asked the terminal nothing: {:?}",
        drawn.text()
    );
    assert!(
        !contains(&drawn.bytes(), b"\x1b[?1049h"),
        "and took no screen of its own: {:?}",
        drawn.text()
    );
}

#[test]
fn ctrl_q_quits_and_the_screen_it_took_is_given_back() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(b"\x1b[?1049h"),
        "the alternate screen is taken: {:?}",
        drawn.text()
    );
    pty.type_bytes(&[0x11]);
    let status = session.wait();

    assert_eq!(
        status.code(),
        Some(0),
        "Ctrl-Q quits a launch with no run to stop: {:?}",
        drawn.text()
    );
    assert!(
        drawn.wait_for(b"\x1b[?1049l"),
        "and the screen it took is given back: {:?}",
        drawn.text()
    );
}

#[test]
fn the_terminal_it_installs_cannot_eat_ctrl_q() {
    let ground = Ground::new();
    let (_session, pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(b"\x1b[?1049h"),
        "the screen is taken before its state is read: {:?}",
        drawn.text()
    );
    let installed = pty.attrs();

    assert!(
        !installed.input_modes.contains(InputModes::IXON),
        "flow control is off, so ^Q arrives as a key rather than vanishing as XON"
    );
    assert!(
        !installed.local_modes.contains(LocalModes::ICANON),
        "raw mode: a key arrives without waiting for a newline"
    );
    assert!(
        !installed.local_modes.contains(LocalModes::ECHO),
        "raw mode: nothing typed is echoed back over the frame"
    );
}

#[test]
fn an_idle_terminal_is_painted_once_and_then_left_alone() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(b"\x1b[?1049h"),
        "the screen is taken: {:?}",
        drawn.text()
    );
    // §12's one frame a launch owes: the boot frame, whose footer carries the two
    // hints no width takes away. Nothing has been typed and no fact has arrived, so
    // once this is on the screen there is nothing left to draw.
    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn once: {:?}",
        drawn.text()
    );

    // Everything after it is a repaint nobody asked for: no timer, no tick, no
    // idle frame — the loop is asleep in `read` until a key or a fact arrives.
    let painted = drawn.bytes().len();
    thread::sleep(Duration::from_millis(600));
    assert_eq!(
        drawn.bytes().len(),
        painted,
        "an idle launch writes no further byte: {}",
        drawn.text()
    );

    // And it was waiting rather than gone: the same launch still answers a key.
    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "the quiet launch was asleep in read, not dead: {}",
        drawn.text()
    );
}

#[test]
fn an_at_walks_the_workspace_the_launch_stands_in() {
    let ground = Ground::new();
    std::fs::write(ground.workspace.join("notes.md"), "").expect("a file in the workspace");
    std::fs::create_dir_all(ground.workspace.join("target")).expect("a directory");
    std::fs::write(ground.workspace.join("target/built.rs"), "").expect("a built file");
    std::fs::write(ground.workspace.join(".gitignore"), "target/\n").expect("a gitignore");
    let (_session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    pty.type_bytes(b"@");

    assert!(
        drawn.wait_for(b"notes.md"),
        "`@` opens the files of the workspace the launch stands in, and the file it holds \
         there is a row of the screen: {:?}",
        drawn.text()
    );
    assert!(
        !text(&drawn.bytes()).contains("built.rs"),
        "and a path the workspace's own `.gitignore` ignores is never offered: {:?}",
        drawn.text()
    );
}

#[test]
fn a_launch_with_no_screen_is_refused_at_exit_two() {
    let ground = Ground::new();
    let pty = Pty::open();

    let output = ground
        .command()
        .stdin(Stdio::from(pty.child_end()))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the binary should run");

    assert_eq!(
        output.status.code(),
        Some(2),
        "a terminal UI with no terminal to draw on is a usage error (§3): {}",
        text(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "nothing reaches a stdout that is no screen: {}",
        text(&output.stdout)
    );
    assert!(
        text(&output.stderr).contains("terminal"),
        "the refusal says what is missing: {}",
        text(&output.stderr)
    );
}

/// A frame's bytes, read the way a terminal reads them: where the cursor was moved to,
/// and which glyph was painted there.
///
/// A frame writes its own geometry down nowhere but its escape sequences, so this is how
/// a test reads a row of it back — and a test that reads rows needs no terminal size, of
/// which a bare pseudo-terminal has none worth guessing at.
enum Painted {
    /// The cursor moved to a row and a column, counted from one as a terminal counts.
    Moved(u16, u16),
    /// A glyph was painted where the cursor was.
    Glyph(char),
}

/// Everything drawn so far, as a terminal would read it.
fn painted(drawn: &str) -> Vec<Painted> {
    let mut painted = Vec::new();
    let mut rest = drawn;
    while let Some(glyph) = rest.chars().next() {
        if glyph == '\u{1b}'
            && let Some(sequence) = sequence(rest)
        {
            if let Some((row, column)) = sequence.move_to {
                painted.push(Painted::Moved(row, column));
            }
            rest = &rest[sequence.taken..];
            continue;
        }
        painted.push(Painted::Glyph(glyph));
        rest = &rest[glyph.len_utf8()..];
    }
    painted
}

/// One escape sequence at the front of the bytes: the cell it moved the cursor to, when
/// it moved one, and the bytes it took up.
struct Sequence {
    /// The row and column the move named, counted from one, or nothing for a sequence
    /// that is not a move — a colour, a mode, a key the frame answers with a repaint.
    move_to: Option<(u16, u16)>,
    /// How many bytes the sequence took up.
    taken: usize,
}

/// The escape sequence at the front of `drawn`, or nothing when the bytes there are not
/// one.
fn sequence(drawn: &str) -> Option<Sequence> {
    let rest = drawn.strip_prefix('\u{1b}')?;
    let Some(params) = rest.strip_prefix('[') else {
        // `ESC O A` and the single-shift keys: no move, and as long as its last byte.
        let taken = 1 + rest.chars().next().map_or(0, char::len_utf8);
        return Some(Sequence {
            move_to: None,
            taken,
        });
    };
    let end = params.find(|glyph: char| ('\u{40}'..='\u{7e}').contains(&glyph))?;
    let last = params[end..].chars().next()?;
    let taken = 2 + end + last.len_utf8();
    let move_to = (last == 'H')
        .then(|| {
            let (row, column) = params[..end].split_once(';')?;
            Some((row.parse().ok()?, column.parse().ok()?))
        })
        .flatten();
    Some(Sequence { move_to, taken })
}

/// The rows the composer's band took in the last frame that painted it: the gap between
/// the row of its hairline and the row of the footer's first hint, which is the box's
/// height (§12).
fn composer_rows(drawn: &str) -> Option<u16> {
    let mut row = 0u16;
    let (mut hairline, mut footer) = (None, None);
    let mut painted_here = String::new();
    for mark in painted(drawn) {
        match mark {
            Painted::Moved(at, _) => {
                row = at;
                painted_here.clear();
            }
            Painted::Glyph(glyph) => {
                painted_here.push(glyph);
                if glyph == '─' {
                    hairline = Some(row);
                }
                if painted_here.ends_with("Enter send") {
                    footer = Some(row);
                }
            }
        }
    }
    Some(footer? - hairline?)
}

/// The row `needle` was last painted on, counted from one as a terminal counts, or nothing
/// when it never was: the frame's own geometry, so a test reads "above the composer" off
/// the picture rather than assuming where a band landed.
fn row_of(drawn: &str, needle: &str) -> Option<u16> {
    let mut row = 0u16;
    let mut painted_here = String::new();
    let mut found = None;
    for mark in painted(drawn) {
        match mark {
            Painted::Moved(at, _) => {
                row = at;
                painted_here.clear();
            }
            Painted::Glyph(glyph) => {
                painted_here.push(glyph);
                if painted_here.ends_with(needle) {
                    found = Some(row);
                }
            }
        }
    }
    found
}

/// Where `needle` was last drawn in `drawn`, or nothing when it never was: a frame paints
/// the cells that changed and leaves the rest, so what a test reads back is a stream of
/// every frame that has been painted, and the last one is the end of it.
fn last(drawn: &str, needle: &str) -> Option<usize> {
    drawn.rfind(needle)
}

/// The cell the last frame left the caret on: typing paints one glyph per frame, so where
/// the caret waits is how a test knows the last glyph of a draft landed.
fn caret_at(drawn: &str) -> Option<(u16, u16)> {
    painted(drawn)
        .into_iter()
        .filter_map(|mark| match mark {
            Painted::Moved(at, column) => Some((at, column)),
            Painted::Glyph(_) => None,
        })
        .next_back()
}

#[test]
fn a_bracketed_paste_arrives_as_one_draft_with_its_line_breaks_kept() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    assert_eq!(
        composer_rows(&drawn.text()),
        Some(2),
        "the composer starts at its two-row floor: {:?}",
        drawn.text()
    );

    // One paste, the way a terminal that offers a bracketed paste sends it: the line
    // breaks are the clipboard's, and not one of them is a pressed Enter.
    pty.type_bytes(b"\x1b[200~first line\rsecond line\r\nthird line\x1b[201~");

    // A frame paints the cells that changed and no others, so a blank a line already had
    // is skipped and a line of words arrives in runs: the words are what is looked for.
    assert!(
        drawn.wait_until(|frame| frame.contains("third")),
        "the paste reached the frame: {:?}",
        drawn.text()
    );
    let frame = drawn.text();
    for word in ["first", "line", "second", "third"] {
        assert!(
            frame.contains(word),
            "the draft holds {word:?}, as the clipboard wrote it"
        );
    }
    // One draft and not three prompts: a box that had sent on a line break would be back
    // at its two-row floor, and this one is a row taller for each line it kept.
    assert_eq!(
        composer_rows(&frame),
        Some(4),
        "three lines make a four-row composer: {frame:?}"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "and the launch is still there to quit"
    );
}

#[test]
fn a_paste_that_arrives_as_a_burst_is_one_draft_too() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    // A terminal with no bracketed paste sends the whole prompt as the bytes a typist
    // would have made, carriage return and all.
    pty.type_bytes(b"one\rtwo");

    assert!(
        drawn.wait_until(|frame| composer_rows(frame) == Some(3)),
        "two lines in one draft: the box grew a row for the break a paste kept rather \
         than sent: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0));
}

#[test]
fn shift_enter_from_the_kitty_protocol_breaks_the_line_rather_than_sending() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"one");
    pty.type_bytes(b"\x1b[13;2u");
    pty.type_bytes(b"two");

    assert!(
        drawn.wait_until(|frame| composer_rows(frame) == Some(3)),
        "Shift+Enter is a newline and not the key that sends: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0));
}

#[test]
fn enter_takes_the_draft_out_of_the_composer() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    pty.type_bytes(b"one");
    pty.type_bytes(b"\x1b[13;2u");
    pty.type_bytes(b"two");
    assert!(
        drawn.wait_until(|frame| composer_rows(frame) == Some(3)),
        "two lines of draft: {:?}",
        drawn.text()
    );

    // Enter sends: the draft is the run's now, and what a reader sees is the box back at
    // its floor with the prompt gone from it (§12).
    pty.type_bytes(b"\r");

    assert!(
        drawn.wait_until(|frame| composer_rows(frame) == Some(2)),
        "the draft left the composer and the band is back at its floor: {:?}",
        drawn.text()
    );
    wire.wait_for_request();
    wire.release();
    assert!(
        drawn.wait_until(|frame| ANSWER_WORDS.iter().all(|word| frame.contains(word))),
        "the run the draft started answered into the frame: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

#[test]
fn ctrl_c_when_idle_clears_the_draft_and_leaves_the_launch_running() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    pty.type_bytes(b"a draft nobody sent");
    assert!(
        drawn.wait_until(|frame| caret_at(frame).is_some_and(|(_, column)| column == 20)),
        "nineteen glyphs typed, so the caret waits one column past the last: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x03]);

    // No run is wired into this launch yet (#43), so `Ctrl-C` is the idle key and clears
    // the draft. The caret going home is the proof: a draft kept would leave it in the
    // nineteenth column and paint nothing at all.
    assert!(
        drawn.wait_until(|frame| caret_at(frame).is_some_and(|(_, column)| column == 1)),
        "Ctrl-C cleared the draft: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "Ctrl-C cancelled no run, and did not quit"
    );
}

#[test]
fn question_mark_opens_the_keys_screen_and_escape_gives_the_frame_back() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    // `?` is the key the footer names, and with an empty draft it is a key rather than a
    // glyph: one screen listing the keys in force, over the whole frame. A frame paints
    // the cells that changed and skips the blanks between runs of words, so a word the
    // screen says and the draft never does is what is looked for.
    pty.type_bytes(b"?");
    assert!(
        drawn.wait_until(|frame| frame.contains("Backspace")),
        "the keys screen lists the draft's keys, Backspace among them: {:?}",
        drawn.text()
    );

    // Esc gives the frame back: the hints the screen covered are painted again, and
    // painted after the screen's own words rather than left over from before it.
    pty.type_bytes(b"\x1b");
    assert!(
        drawn.wait_until(|frame| last(frame, "Enter send") > last(frame, "Backspace")),
        "Esc closed the screen and the composer's footer is back under it: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "and the launch is still there to quit once the screen is closed: {:?}",
        drawn.text()
    );
}

/// A session's document, once a launch has written one: the run under `state`, read the way
/// a reader reads it back.
fn document(state: &std::path::Path) -> String {
    let sessions = state.join("sindri").join("sessions");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&sessions)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    let directory = found.first().expect("the launch recorded a session");
    std::fs::read_to_string(directory.join("session.json")).expect("the session document")
}

/// Waits until the document under `state` says `needle`, and says whether it ever did: a
/// record is written as a run goes, so a test waits for the fact rather than for a sleep.
fn recorded(state: &std::path::Path, needle: &str) -> bool {
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        if document(state).contains(needle) {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}

/// Types a draft and sends it, and waits until the run it started has a request in flight:
/// the state a cancellation has to arrive in (§4).
///
/// The caret is what the draft is waited on and not the words: a glyph typed is a glyph
/// painted, so the words of a draft arrive one frame each, and the caret landing past the last
/// of them is the one drawing that says the whole draft is in the box.
///
/// The wait is not only for the frame's sake: a terminal handed the draft and the `Enter` in
/// one read hands over a paste, and no line break in a paste sends (§12).
fn ask(pty: &mut Pty, drawn: &Drawn, wire: &Wire) {
    pty.type_bytes(b"triage this");
    assert!(
        drawn.wait_until(|frame| caret_at(frame).is_some_and(|(_, column)| column == 12)),
        "eleven glyphs typed, so the caret waits one column past the last: {:?}",
        drawn.text()
    );
    pty.type_bytes(b"\r");
    wire.wait_for_request();
}

#[test]
fn ctrl_c_in_a_run_cancels_it_and_keeps_the_draft() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);

    // A draft typed while the run is live: the one a cancel has to keep (§12).
    pty.type_bytes(b"a second thought");
    assert!(
        drawn.wait_until(|frame| caret_at(frame).is_some_and(|(_, column)| column == 17)),
        "sixteen glyphs of a new draft, and the caret one column past them: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x03]);

    // `Ctrl-C` cancels the run and the draft stays: a caret gone home would be the idle key,
    // which clears the draft, and a launch that quit would have given the screen back.
    thread::sleep(Duration::from_millis(300));
    assert!(
        caret_at(&drawn.text()).is_some_and(|(_, column)| column == 17),
        "the draft a cancel found is still in the composer: {:?}",
        drawn.text()
    );

    // The run stops when the request in flight does, and says so in the record it was
    // writing as it went (§4, §13).
    wire.release();
    assert!(
        recorded(&ground.state, r#""stopped": "cancelled""#),
        "the record's stop reason is the user's own word: {}",
        document(&ground.state)
    );
    assert!(
        recorded(&ground.state, r#""turns": 1"#),
        "the run it cancelled is the run that was recorded: {}",
        document(&ground.state)
    );

    // The composer is idle again, so the same key now clears the draft — which is how a test
    // reads back that the run really ended rather than only that the record did.
    pty.type_bytes(&[0x03]);
    assert!(
        drawn.wait_until(|frame| caret_at(frame).is_some_and(|(_, column)| column == 1)),
        "the run ended, so Ctrl-C is the idle key and clears the draft: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "a cancelled run is not a launch that ends: {:?}",
        drawn.text()
    );
    assert!(
        drawn.wait_for(b"\x1b[?1049l"),
        "and Ctrl-Q still gives the screen back: {:?}",
        drawn.text()
    );
}

/// §12's pulse, at the terminal a reader really watches: while a run is live the marker
/// moves once per 80 ms, and the moment the run stops it settles on the word it stopped
/// with — `✗ cancelled 5.0s` — and the frame is left alone from then on, because an idle
/// frame is never drawn (#39).
#[test]
fn a_cancelled_run_settles_as_cancelled_and_the_settled_frame_is_left_alone() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);

    // The run is live and waiting on a wire: the marker at the head of its answer steps
    // through the braille glyphs while it waits, which is the whole of what tells a reader
    // that something is still happening.
    assert!(
        drawn.wait_until(is_braille),
        "a live run wears a braille marker: {:?}",
        drawn.text()
    );
    let stepping = drawn.bytes().len();
    thread::sleep(Duration::from_millis(400));
    assert!(
        drawn.bytes().len() > stepping,
        "a live run is a frame that moves: {:?}",
        drawn.text()
    );

    // The user's own word, and the run stops when the request in flight does.
    pty.type_bytes(&[0x03]);
    wire.release();

    // Cancelled is settled as cancelled: the marker says the word, and not the cross a
    // failure wears with nothing beside it.
    assert!(
        drawn.wait_until(|frame| frame.contains("cancelled")),
        "a cancelled run settles as cancelled: {:?}",
        drawn.text()
    );

    // Settling draws once and nothing after: the run is over, so there is no clock left to
    // wake the loop and the settled frame is the last byte the launch writes.
    thread::sleep(Duration::from_millis(300));
    let settled = drawn.bytes().len();
    thread::sleep(Duration::from_millis(600));
    assert_eq!(
        drawn.bytes().len(),
        settled,
        "a settled frame is left alone, and no idle one is ever drawn: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "the launch outlives the run it cancelled: {:?}",
        drawn.text()
    );
}

/// Whether a frame is wearing a braille marker: one of the ten glyphs the pulse steps
/// through, which is a cell no other row of this frame draws.
fn is_braille(frame: &str) -> bool {
    frame
        .chars()
        .any(|glyph| ('\u{2800}'..='\u{28ff}').contains(&glyph))
}

#[test]
fn an_outside_sigint_stops_the_run_and_gives_the_terminal_back() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);

    // The user's word from outside the keyboard: `Ctrl-C` reaches a raw terminal as a byte,
    // so a `kill -INT` is the other door into the one cancellation (§4).
    let interrupted = session.interrupt();
    assert!(interrupted, "the signal was delivered to the launch");

    // The run stops when the request in flight does, and the launch gives the terminal back
    // rather than dying with it: the screen it took is put back, and §15's cancellation code
    // is the launch's.
    wire.release();
    let status = session.wait();
    assert_eq!(
        status.code(),
        Some(130),
        "a launch the user's word ended (§15): {:?}",
        drawn.text()
    );
    assert!(
        drawn.wait_for(b"\x1b[?1049l"),
        "the alternate screen is given back on a signal as it is on a panic: {:?}",
        drawn.text()
    );
    assert!(
        recorded(&ground.state, r#""stopped": "cancelled""#),
        "and the record says what stopped the run: {}",
        document(&ground.state)
    );
    let given_back = pty.attrs();
    assert!(
        given_back.local_modes.contains(LocalModes::ICANON)
            && given_back.local_modes.contains(LocalModes::ECHO),
        "the state raw mode replaced is back, so a shell can use the terminal the launch left"
    );
    assert!(
        given_back.input_modes.contains(InputModes::IXON),
        "flow control included"
    );
}

/// Types `prompt` and sends it, and waits until the box is empty again: the trace a prompt
/// typed while a run is live leaves when it went into the queue instead of being dropped
/// with the draft it was (§12).
///
/// The caret is waited on twice and not the words: a frame paints one cell per glyph, so a
/// caret one column past the last glyph says the whole draft is in the box, and a caret
/// gone home says it left. The first wait is not only the frame's sake — a terminal handed
/// the draft and the `Enter` in one read hands over a paste, and no line break in a paste
/// sends (§12).
fn queue(pty: &mut Pty, drawn: &Drawn, prompt: &str) {
    pty.type_bytes(prompt.as_bytes());
    let typed = u16::try_from(prompt.chars().count()).expect("a prompt narrower than a frame") + 1;
    assert!(
        drawn.wait_until(|frame| caret_at(frame).is_some_and(|(_, column)| column == typed)),
        "{prompt:?} is in the box, the caret one column past its last glyph: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"\r");
    assert!(
        drawn.wait_until(|frame| caret_at(frame).is_some_and(|(_, column)| column == 1)),
        "{prompt:?} left the box for the queue: {:?}",
        drawn.text()
    );
}

#[test]
fn enter_during_a_run_keeps_the_prompt_above_the_composer() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);

    // `Enter` with a run in flight cannot run the prompt, so the prompt waits above the
    // composer and the frame says how many are waiting in a row of its own (§12).
    queue(&mut pty, &drawn, "read the test that failed");

    let frame = drawn.text();
    let count = row_of(&frame, "1 queued");
    assert!(
        count.is_some(),
        "the frame counts the prompt waiting above the composer: {frame:?}"
    );
    assert_eq!(
        composer_rows(&frame),
        Some(2),
        "and the box under it is still at its two-row floor, so the count is not in it"
    );
    assert!(
        count
            .zip(caret_at(&frame))
            .is_some_and(|(count, (caret, _))| count < caret),
        "the count sits above the composer's own row: {frame:?}"
    );

    wire.release();
    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

#[test]
fn the_prompts_typed_during_a_run_run_in_the_order_they_were_typed() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);

    // Two prompts typed while the first run waits on the wire: neither runs beside it, and
    // neither is thrown away.
    queue(&mut pty, &drawn, "second");
    queue(&mut pty, &drawn, "third");

    // The run in flight stops when its request is answered, and what runs next is the queue,
    // oldest first: the wire is asked for the prompt typed first, then the one behind it (§12).
    wire.release();
    assert!(
        wire.wait_for_prompt("second"),
        "the prompt typed first is the run that starts first: {:?}",
        wire.requests()
    );
    wire.release();
    assert!(
        wire.wait_for_prompt("third"),
        "and the one behind it starts next: {:?}",
        wire.requests()
    );

    let asked = wire.requests();
    assert_eq!(asked.len(), 3, "three runs, three requests: {asked:?}");
    for (at, prompt) in ["triage this", "second", "third"].iter().enumerate() {
        assert!(
            asked[at].contains(prompt),
            "request {at} asks for {prompt:?}: {}",
            asked[at]
        );
    }

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

#[test]
fn ctrl_x_drops_the_newest_prompt_the_queue_holds() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);
    queue(&mut pty, &drawn, "keep this");
    queue(&mut pty, &drawn, "drop this");

    // `Ctrl-X` drops the newest queued prompt (§12). The `x` typed straight after it is the
    // proof the key was answered: the loop answers one key before the next, so a glyph the
    // frame painted is a `Ctrl-X` already acted on.
    pty.type_bytes(&[0x18]);
    pty.type_bytes(b"x");
    assert!(
        drawn.wait_until(|frame| caret_at(frame).is_some_and(|(_, column)| column == 2)),
        "the glyph typed after Ctrl-X is in the draft, so the drop has happened: {:?}",
        drawn.text()
    );

    wire.release();
    assert!(
        wire.wait_for_prompt("keep this"),
        "the prompt the queue still holds is the run that starts: {:?}",
        wire.requests()
    );
    wire.release();
    thread::sleep(Duration::from_millis(300));

    let asked = wire.requests();
    assert_eq!(asked.len(), 2, "two runs and no third: {asked:?}");
    assert!(
        asked.iter().all(|request| !request.contains("drop this")),
        "the prompt dropped never ran: {asked:?}"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

#[test]
fn up_on_the_first_line_takes_the_newest_queued_prompt_back() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);
    queue(&mut pty, &drawn, "keep this");
    queue(&mut pty, &drawn, "the newest");

    // `↑` with the caret on the composer's first line and nothing typed: the thing above the
    // caret is the queue, so the newest queued prompt is the draft again (§12) — the caret
    // lands one column past its last glyph, which is how a test reads the words as back.
    pty.type_bytes(b"\x1b[A");
    assert!(
        drawn.wait_until(|frame| caret_at(frame).is_some_and(|(_, column)| column == 11)),
        "the recalled prompt is the whole draft again: {:?}",
        drawn.text()
    );

    // And it left the queue: the run that starts next is the prompt still waiting in it,
    // because the recalled one is a draft — nothing runs it until `Enter` does.
    wire.release();
    assert!(
        wire.wait_for_prompt("keep this"),
        "the prompt the queue still holds is the run that starts: {:?}",
        wire.requests()
    );
    wire.release();
    thread::sleep(Duration::from_millis(300));

    let asked = wire.requests();
    assert_eq!(asked.len(), 2, "two runs and no third: {asked:?}");
    assert!(
        asked.iter().all(|request| !request.contains("the newest")),
        "the recalled prompt is a draft and not a run: {asked:?}"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

/// A `/` line typed and sent: the words, and then `Enter` as a key of its own once the frame
/// has caught up with them.
///
/// The `Enter` is a separate write and a separate read on purpose: §12's decoder reads a
/// burst of bytes as a paste — where a line break is a line break and never a send — so the
/// only way a test presses `Enter` on a command line is to let the draft land first.
fn command(pty: &mut Pty, drawn: &Drawn, line: &str) {
    pty.type_bytes(line.as_bytes());
    let column = line.chars().count() as u16 + 1;
    assert!(
        drawn.wait_until(|frame| caret_at(frame).is_some_and(|(_, at)| at == column)),
        "the command line is the draft before it is sent, so the caret waits one column \
         past its last glyph: {:?}",
        drawn.text()
    );
    pty.type_bytes(b"\r");
}

/// The config file a launch would write, holding a reader's own comment and a key the save
/// must not touch: what an owner-only save that patched the whole file would destroy.
fn a_readers_config(ground: &Ground) -> PathBuf {
    let path = ground.config.join("sindri").join("config.toml");
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("the config's directory");
    std::fs::write(
        &path,
        "# the model I like, and a comment the save must not eat\n\
         model = \"claude-sonnet-4-6\"\n\
         max_turns = 42\n",
    )
    .expect("a config of the reader's own");
    path
}

/// Waits until the config at `path` says `model = <name>`, and hands the file back: the save
/// happens on the launch's own thread, so a test waits for the file rather than for a sleep.
fn written_model(path: &Path, name: &str) -> String {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        if text.contains(&format!("model = \"{name}\"")) {
            return text;
        }
        assert!(
            Instant::now() < deadline,
            "the config should have been written with `{name}` by now, and says:\n{text}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn slash_model_offers_the_names_the_wire_serves_and_enter_writes_the_one_chosen() {
    let ground = Ground::new();
    let config = a_readers_config(&ground);
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    // `/model` is a command line and not a prompt: no run is started and the request that
    // was never made is not one the wire heard.
    command(&mut pty, &drawn, "/model");
    assert!(
        drawn.wait_until(|frame| { frame.contains(FIRST_SERVED) && frame.contains(SECOND_SERVED) }),
        "the screen offers the names the wire served: {:?}",
        drawn.text()
    );
    assert!(
        wire.requests()
            .iter()
            .all(|request| request.starts_with("GET ")),
        "a command line is the frame's and never the model's: the wire was asked what it \
         serves and for no completion at all: {:?}",
        wire.requests()
    );

    // A query typed into the screen ranks it: `haiku` leaves the second name, so `Enter`
    // takes the name the query named rather than the first the wire listed (§12). The
    // query is waited for before the `Enter`, because a terminal handed both in one read
    // hands over a paste, and no line break in a paste sends.
    pty.type_bytes(b"haiku");
    assert!(
        drawn.wait_until(|frame| row_of(frame, "Claude Haiku 4.5") == Some(3)),
        "the names are ranked as the query is typed, so the haiku model is the first row \
         under the box and the sonnet model is no row at all: {:?}",
        drawn.text()
    );
    pty.type_bytes(b"\r");

    let written = written_model(&config, SECOND_SERVED);
    assert!(
        written.contains("# the model I like"),
        "the save patched the one key it touched and kept the reader's comment (ADR-0009): \
         {written}"
    );
    assert!(
        written.contains("max_turns = 42"),
        "and every key it was not handed: {written}"
    );
    let mode = std::fs::metadata(&config)
        .expect("the file the save left")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "written owner-only, like every config write");

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

#[test]
fn slash_model_with_a_name_sets_a_model_the_wire_never_offered() {
    let ground = Ground::new();
    let config = a_readers_config(&ground);
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    // A name no listing holds: the point of the command form, and the reason a wire that
    // cannot answer is a hint rather than a refusal (§10, §12).
    command(&mut pty, &drawn, "/model a-name-the-wire-never-served");

    let written = written_model(&config, "a-name-the-wire-never-served");
    assert!(
        written.contains("# the model I like"),
        "by the same owner-only save, patching the same one key: {written}"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

#[test]
fn slash_help_opens_the_keys_in_force_and_slash_settings_its_own_screen() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    // `Backspace` is a word only the keys screen says: the footer the composer keeps names
    // two hints, and the screen that lists every key in force names this one.
    command(&mut pty, &drawn, "/help");
    assert!(
        drawn.wait_until(|frame| frame.contains("Backspace")),
        "`/help` opens the screen that lists the keys in force: {:?}",
        drawn.text()
    );

    // Esc gives the frame back, and the footer coming back after the screen's own words is
    // how the frame says so — the same drawing `?` is closed on.
    pty.type_bytes(b"\x1b");
    assert!(
        drawn.wait_until(|frame| last(frame, "Enter send") > last(frame, "Backspace")),
        "Esc closed the keys screen: {:?}",
        drawn.text()
    );
    command(&mut pty, &drawn, "/settings");
    assert!(
        drawn.wait_until(|frame| row_of(frame, "settings") == Some(1)),
        "`/settings` opens its own screen, headed on the frame's first row: {:?}",
        drawn.text()
    );

    // And it gives the frame back the way the keys screen does; `Ctrl-Q` is the composer's
    // key, so the screen is closed before the launch is asked to end.
    pty.type_bytes(b"\x1b");
    assert!(
        drawn.wait_until(|frame| last(frame, "Enter send") > last(frame, "settings")),
        "Esc closed the settings screen: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

#[test]
fn slash_quit_quits_as_ctrl_q_does() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    command(&mut pty, &drawn, "/quit");

    assert_eq!(
        session.wait().code(),
        Some(0),
        "the command line is the one way out as a line: {:?}",
        drawn.text()
    );
    assert!(
        drawn.wait_for(b"\x1b[?1049l"),
        "and the screen it took is given back: {:?}",
        drawn.text()
    );
}

#[test]
fn slash_new_clears_the_frame_and_leaves_the_session_it_superseded_running() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(b"Enter send"),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    // A run in flight, its reply held at the wire: the session it is recorded under exists
    // and says `running` because no stop reason has landed yet (§13).
    ask(&mut pty, &drawn, &wire);

    command(&mut pty, &drawn, "/new");
    assert!(
        drawn.wait_until(|frame| caret_at(frame).is_some_and(|(_, column)| column == 1)),
        "the command line left the composer, so it was answered rather than queued: {:?}",
        drawn.text()
    );

    let said = document(&ground.state);
    assert!(
        said.contains("\"stopped\": \"running\""),
        "a session `/new` superseded keeps saying `running`, because it did not stop — it \
         was left: {said}"
    );
    assert!(
        !said.contains("\"end\":"),
        "and it is given no end it did not reach: {said}"
    );
    assert!(
        wire.requests().len() == 1,
        "the run in flight is not stopped by `/new`, and no second run was started: {:?}",
        wire.requests()
    );

    // The run it left still reaches its own stop reason, in its own record: nothing of the
    // conversation was written off, only the frame's hold of it was dropped.
    wire.release();
    assert!(
        drawn.wait_until(|frame| ANSWER_WORDS.iter().all(|word| frame.contains(word))),
        "the run the reader left goes on to answer: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}
