//! A pseudo-terminal is opened here, the binary is spawned on its slave, and the bytes it draws —
//! and the bytes a key makes it give back — are read off the master. Every child gets `env_clear`
//! and homes of its own, so nothing of this process's environment, least of all a real key, reaches it.

use std::collections::BTreeMap;
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

use sindri::ledger::Rail;
use sindri::pen;
use sindri::plane::SEAM;

/// How long a frame is given to arrive before a test calls it absent: generous,
/// because a test that is merely slow must not read as a program that never drew.
const PATIENCE: Duration = Duration::from_secs(10);

const ANSWER: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_stream.sse");

const ANSWER_CACHED: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_stream_cached.sse");

/// The footer's first hint, as the glyphs a terminal paints and as the bytes that carry them: a key
/// and the word that says what it does are two looks, so the escape between them stands inside the
/// row's bytes and not inside the glyphs it is read back as.
const FIRST_HINT: &str = "Enter send";

const FIRST_KEY: &[u8] = b"Enter";

/// The stream's word, split the way it streams: a frame repaints only the cells a fragment
/// changed, so a test looks for both halves and never for one run of words.
const ANSWER_WORDS: [&str; 2] = ["Hello", "world"];

/// Those two halves as one answer: a request's body carries the words whole, where a frame
/// above them repaints them in fragments.
const ANSWER_SAID: &str = "Hello world";

/// The turns a request's body carries, in the order the wire reads them: each one's role and its
/// words, so a test asserts what a wire was asked rather than the bytes that asked it.
fn replayed(request: &str) -> Vec<(String, String)> {
    let (_, body) = request
        .split_once("\r\n\r\n")
        .expect("a request's head and its body");
    let body: serde_json::Value = serde_json::from_str(body).expect("a wire's body is JSON");
    body["messages"]
        .as_array()
        .expect("a body carries messages")
        .iter()
        .map(|message| {
            (
                String::from(message["role"].as_str().unwrap_or_default()),
                String::from(message["content"][0]["text"].as_str().unwrap_or_default()),
            )
        })
        .collect()
}

fn said(role: &str, words: &str) -> (String, String) {
    (String::from(role), String::from(words))
}

/// `CSI 16 t`, the one question a picture asks a terminal: report the size of a cell in pixels.
/// A terminal answers `CSI 6;height;width t` and this launch is never answered, so it settles for
/// the nominal cell — which is what makes the question visible as the bytes it is.
const ASK_CELL: &[u8] = b"\x1b[16t";

/// `CSI > 1 u` pushes the key protocol with `disambiguate escape codes` set: the flag that makes
/// `Shift+Enter` a report of its own rather than the byte the `Enter` key sends.
const ASK_KEYS: &[u8] = b"\x1b[>1u";

/// `CSI < u` pops that push, so the shell gets back the terminal it lent.
const GIVE_BACK_KEYS: &[u8] = b"\x1b[<u";

/// `CSI ? 1004 h`: the focus report is asked for when the screen is taken, and `l` gives it up.
const ASK_FOCUS: &[u8] = b"\x1b[?1004h";

const GIVE_UP_FOCUS: &[u8] = b"\x1b[?1004l";

/// `CSI ? 1000 h` and `CSI ? 1006 h`: the press and the wheel are reported, and a press names the
/// cell it landed on.
const TAKE_POINTER: &[u8] = b"\x1b[?1000h\x1b[?1006h";

const GIVE_UP_POINTER: &[u8] = b"\x1b[?1000l\x1b[?1006l";

const SERVED: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_models.json");

const FIRST_SERVED: &str = "claude-sonnet-4-6";
const SECOND_SERVED: &str = "claude-haiku-4-5";

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

fn on_a_branch(ground: &Ground) {
    for args in [
        &["init", "-q", "-b", "main"][..],
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "the first commit",
        ][..],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(&ground.workspace)
                .status()
                .expect("git runs")
                .success(),
            "the workspace becomes a repository on a branch"
        );
    }
}

/// A wire on a loopback socket holding one reply, so a request is in flight while a test types and
/// lands when the test lets it go.
struct Wire {
    port: u16,
    arrived: Receiver<()>,
    released: Sender<()>,
    heard: Arc<Mutex<Vec<String>>>,
}

impl Wire {
    fn holding() -> Wire {
        Wire::answering(ANSWER)
    }

    fn holding_a_cached_reply() -> Wire {
        Wire::answering(ANSWER_CACHED)
    }

    fn answering(reply: &'static str) -> Wire {
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
                // nothing waits on one the way a test waits on a reply it is holding.
                let listing = request.starts_with("GET ");
                listening.lock().expect("the wire's requests").push(request);
                if listing {
                    let _ = catalogue(&mut stream);
                    continue;
                }
                let _ = arrived.send(());
                let _ = released.recv_timeout(PATIENCE);
                let _ = answer(&mut stream, reply);
            }
        });
        Wire {
            port,
            arrived: arrivals,
            released: release,
            heard,
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

    fn release(&self) {
        let _ = self.released.send(());
    }

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

    fn requests(&self) -> Vec<String> {
        self.heard.lock().expect("the wire's requests").clone()
    }
}

/// The whole of an HTTP request: the head, then the body its `content-length` promised, because a
/// run waits on a reply nothing can send until its request has been read to the end.
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

/// The recorded reply, written as a wire writes one: its length is given so the client does not
/// wait for a close to find the end of it.
fn answer(stream: &mut TcpStream, reply: &'static str) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        reply.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(reply.as_bytes())?;
    stream.flush()
}

fn catalogue(stream: &mut TcpStream) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        SERVED.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(SERVED.as_bytes())?;
    stream.flush()
}

struct Pty {
    master: File,
    slave: File,
}

impl Pty {
    /// A fresh pseudo-terminal, nobody's controlling terminal, told its size: a pty that never says how
    /// big it is leaves crossterm to `/dev/tty` or to a descriptor reporting nought by nought, and a
    /// frame painted into no cells draws nothing — so a caret test passed only on a machine with a terminal.
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

    fn child_end(&self) -> File {
        self.slave
            .try_clone()
            .expect("a second handle on the slave")
    }

    /// The terminal's state, as the program left it: the flags are the terminal's, not a fd's, so the
    /// parent reads what the child installed.
    fn attrs(&self) -> Termios {
        tcgetattr(&self.slave).expect("the terminal's state")
    }

    fn type_bytes(&mut self, keys: &[u8]) {
        self.master.write_all(keys).expect("the key");
        self.master.flush().expect("the key");
    }
}

/// What a child has drawn so far, read off the master by a thread of its own so a test never blocks
/// on a frame that may never come.
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

    fn bytes(&self) -> Vec<u8> {
        self.seen.lock().expect("the frame").clone()
    }

    fn text(&self) -> String {
        text(&self.bytes())
    }

    /// Waits until what has been drawn says `ready`, and says whether it ever did: a frame is only as
    /// caught up as the bytes that reached the terminal, so a test waits for the fact it needs rather
    /// than for a sleep and a hope.
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

    /// What has been drawn once the terminal has gone quiet, and no later than the patience: an
    /// idle launch paints nothing more, so two frames are compared once theirs have landed.
    fn settled(&self) -> Vec<u8> {
        let mut last = self.bytes().len();
        let deadline = Instant::now() + PATIENCE;
        loop {
            thread::sleep(Duration::from_millis(50));
            let now = self.bytes().len();
            if now == last || Instant::now() >= deadline {
                return self.bytes();
            }
            last = now;
        }
    }

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

/// A launch on a terminal, held so a test that fails early still ends it: a child left holding a
/// pseudo-terminal would keep the harness waiting for a frame that never comes.
struct Session {
    child: Child,
}

impl Session {
    fn open(ground: &Ground) -> (Session, Pty, Drawn) {
        Session::starting(ground.command())
    }

    /// A launch as a filter: the prompt on its own command line and no screen at all, on a terminal so
    /// that a probe would have somewhere to land if anything asked.
    fn filtering(ground: &Ground, wire: &Wire) -> (Session, Pty, Drawn) {
        let mut command = ground.command();
        command
            .arg("-b")
            .arg("what does the wire say")
            .arg("--yolo-unsafe")
            .env("SINDRI_BASE_URL", wire.base_url());
        Session::starting(command)
    }

    /// A launch that can run a prompt: the ground's own child, pointed at `wire` so its run asks a socket
    /// on this machine, and taking the escape hatch from the sandbox because confinement is not what a
    /// cancellation test is about — the kill reaches the process group either way.
    fn asking(ground: &Ground, wire: &Wire) -> (Session, Pty, Drawn) {
        let mut command = ground.command();
        command
            .arg("--yolo-unsafe")
            .env("SINDRI_BASE_URL", wire.base_url());
        Session::starting(command)
    }

    /// A launch on a terminal that names itself, which is the whole of what a terminal that takes
    /// the window's sequences is read from.
    fn named(ground: &Ground, wire: &Wire, term: &str) -> (Session, Pty, Drawn) {
        let mut command = ground.command();
        command
            .arg("--yolo-unsafe")
            .env("SINDRI_BASE_URL", wire.base_url())
            .env("TERM", term);
        Session::starting(command)
    }

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

    fn interrupt(&self) -> bool {
        Command::new("kill")
            .args(["-INT", &self.child.id().to_string()])
            .status()
            .expect("`kill` should run")
            .success()
    }

    /// Waits for the launch to end, and reports what it ended as, within a bound: a child that never
    /// ends is a test that never finishes, which reads as a broken harness rather than as a program
    /// that failed to quit.
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

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    at(haystack, needle).is_some()
}

/// Where a sequence first sits in the bytes a terminal was written, and `None` if it was not.
fn at(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes
        .windows(needle.len())
        .position(|window| window == needle)
}

/// The pixels in a cell are the one thing a launch asks the terminal rather than draws into it, so it
/// asks once, before the event loop, and every frame that follows reads the answer it kept.
#[test]
fn the_cell_size_is_asked_of_the_terminal_once_however_many_frames_it_paints() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    // Three keys and three frames: a launch that asked once per frame would have asked four
    // times by the time the third digit is on the screen.
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
            .zip(at(&bytes, FIRST_KEY))
            .is_some_and(|(asked_at, boot_at)| asked_at < boot_at),
        "and it was asked before the first frame was painted, so the frame was drawn at \
         an answer rather than at a guess"
    );
}

#[test]
fn the_key_protocol_is_asked_for_before_the_first_frame_and_given_back_first_on_the_way_out() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    let bytes = drawn.bytes();
    let (screen, ask, frame) = (
        at(&bytes, b"\x1b[?1049h"),
        at(&bytes, ASK_KEYS),
        at(&bytes, FIRST_KEY),
    );
    assert!(
        matches!((screen, ask, frame), (Some(screen), Some(ask), Some(_)) if screen < ask),
        "the key protocol is asked for when the screen is taken and before the frame is drawn, \
         and these came at {screen:?}, {ask:?} and {frame:?}"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
    assert!(
        drawn.wait_for(GIVE_BACK_KEYS),
        "the protocol is given back: {:?}",
        drawn.text()
    );

    let bytes = drawn.bytes();
    let (given_back, screen) = (at(&bytes, GIVE_BACK_KEYS), at(&bytes, b"\x1b[?1049l"));
    assert!(
        matches!((given_back, screen), (Some(given_back), Some(screen)) if given_back < screen),
        "and it is given back first: the screen goes after the protocol, and these came at \
         {given_back:?} and {screen:?}"
    );
}

#[test]
fn a_terminal_that_names_itself_wears_the_title_is_asked_for_its_focus_and_gives_it_up() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::named(&ground, &wire, "xterm-kitty");

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    assert!(
        drawn.wait_for("\x1b]2;workspace \u{2014} idle\x1b\\".as_bytes()),
        "the title names the workspace the launch is in and the state it is in: {:?}",
        drawn.text()
    );
    assert!(
        drawn.wait_for(ASK_FOCUS),
        "the terminal is asked which of its windows is being read: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
    assert!(
        drawn.wait_for(GIVE_UP_FOCUS),
        "and gives the report up on the way out: {:?}",
        drawn.text()
    );
}

#[test]
fn a_terminal_that_names_itself_takes_the_pointer_and_gives_it_up_on_the_way_out() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::named(&ground, &wire, "xterm-kitty");

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    let bytes = drawn.bytes();
    let (screen, taken, frame) = (
        at(&bytes, b"\x1b[?1049h"),
        at(&bytes, TAKE_POINTER),
        at(&bytes, FIRST_KEY),
    );
    assert!(
        matches!((screen, taken, frame), (Some(screen), Some(taken), Some(frame)) if screen < taken && taken < frame),
        "the pointer is taken when the screen is taken and before the first frame is drawn, and \
         these came at {screen:?}, {taken:?} and {frame:?}"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
    assert!(
        drawn.wait_for(GIVE_UP_POINTER),
        "and it is given up on the way out: {:?}",
        drawn.text()
    );
}

#[test]
fn a_terminal_nobody_can_vouch_for_is_handed_no_pointer_at_all() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::named(&ground, &wire, "xterm-256color");

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());

    let bytes = drawn.bytes();
    assert!(
        !contains(&bytes, TAKE_POINTER) && !contains(&bytes, GIVE_UP_POINTER),
        "a terminal nobody can vouch for is handed no pointer sequence, and given none back: {:?}",
        text(&bytes)
    );
}

/// The notification bodies on the wire: the progress mark shares their sequence and is told apart
/// by the state digit after it.
fn notifications(bytes: &[u8]) -> usize {
    let text = text(bytes);
    text.match_indices("\x1b]9;")
        .filter(|(at, _)| !text[at + 4..].starts_with("4;"))
        .count()
}

#[test]
fn a_run_that_ends_while_nobody_is_looking_raises_one_notification() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::named(&ground, &wire, "xterm-kitty");

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    // The terminal says nobody is looking, which is the report no key can stand in for: typed
    // before the prompt, because a report and a stop that land in one pass are answered in the
    // loop's own order, and a test may not lean on which of the two is read first.
    pty.type_bytes(b"\x1b[O");
    ask(&mut pty, &drawn, &wire);
    wire.release();
    answered(&drawn);

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
    let bytes = drawn.bytes();
    assert_eq!(
        notifications(&bytes),
        1,
        "one notification for the run that ended while the window was unfocused: {:?}",
        text(&bytes)
    );
    assert!(
        contains(&bytes, b"\x1b]9;workspace: answered\x1b\\"),
        "and it says what the run did, in the workspace it did it in: {:?}",
        text(&bytes)
    );
}

#[test]
fn a_run_a_reader_watches_end_raises_no_notification() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::named(&ground, &wire, "xterm-kitty");

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);
    wire.release();
    answered(&drawn);

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
    assert_eq!(
        notifications(&drawn.bytes()),
        0,
        "a run the reader watched end is a run they saw end: {:?}",
        drawn.text()
    );
}

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
fn first_run_without_provider_or_key_opens_the_tui() {
    let ground = Ground::new();
    let mut command = ground.command();
    command
        .env_remove("SINDRI_PROVIDER")
        .env_remove("SINDRI_MODEL")
        .env_remove("ANTHROPIC_API_KEY");
    let (session, mut pty, drawn) = Session::starting(command);

    assert!(
        drawn.wait_for(b"\x1b[?1049h"),
        "the screen opens: {:?}",
        drawn.text()
    );
    assert!(
        drawn.wait_for(b"/settings"),
        "the splash offers settings: {:?}",
        drawn.text()
    );
    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0));
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
    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn once: {:?}",
        drawn.text()
    );

    // Everything after it is a repaint nobody asked for: no timer, no tick, no idle frame —
    // the loop is asleep in `read` until a key or a fact arrives. The count is taken once the frame
    // has landed: a paint is many writes, so the length the moment its last word arrives is short.
    let painted = drawn.settled().len();
    thread::sleep(Duration::from_millis(600));
    assert_eq!(
        drawn.bytes().len(),
        painted,
        "an idle launch writes no further byte: {}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "the quiet launch was asleep in read, not dead: {}",
        drawn.text()
    );
}

#[test]
fn a_quit_during_a_run_closes_the_session_that_run_holds() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    // The run is in flight and the wire is holding its reply, so the session is the run thread's
    // and not the loop's: the ending has to reach the thread to close it.
    ask_words(&mut pty, &drawn, &wire, "hi my name is Glenn");
    let directory = directories(&ground.state)
        .into_iter()
        .next()
        .expect("the prompt the launch started opened a session");
    let scratch = directory.join("tmp").join("scratch.txt");
    std::fs::write(&scratch, "kept\n").expect("a file in the session temp");

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());

    assert!(
        !scratch.exists(),
        "a launch that quit mid-run empties the temp of the session it held"
    );
    assert!(
        directory.join("session.json").is_file(),
        "and keeps the record the run wrote: {}",
        directory.display()
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
        drawn.wait_for(FIRST_KEY),
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
        "a terminal UI with no terminal to draw on is a usage error: {}",
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

/// A frame's bytes, read the way a terminal reads them: where the cursor was moved to, and which
/// glyph was painted there. A frame writes its geometry down nowhere but its escape sequences, so
/// this is how a test reads a row of it back.
enum Painted {
    Moved(u16, u16),
    Glyph(char),
    /// The terminal's own cursor shown or hidden, which is the mark a placing frame leaves.
    Shown(bool),
}

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
            if let Some(shown) = sequence.shown {
                painted.push(Painted::Shown(shown));
            }
            rest = &rest[sequence.taken..];
            continue;
        }
        painted.push(Painted::Glyph(glyph));
        rest = &rest[glyph.len_utf8()..];
    }
    painted
}

struct Sequence {
    move_to: Option<(u16, u16)>,
    /// Whether the sequence shows the terminal's own cursor or hides it, and none for a sequence
    /// that says nothing about it.
    shown: Option<bool>,
    taken: usize,
}

/// How much of a sequence a terminal swallows and paints no cell of: an `OSC`, `DCS`, `APC`, `PM` or
/// `SOS` hands over words of its own, closing on `ST` and for `OSC` alone on `BEL`. One still open is
/// held to the end of what was drawn, so a document read in halves puts no word in a row.
fn swallowed(rest: &str) -> Option<usize> {
    let introducer = rest.chars().next()?;
    let bel = introducer == ']';
    if !bel && !matches!(introducer, 'P' | '_' | '^' | 'X') {
        return None;
    }
    let words = &rest[introducer.len_utf8()..];
    let opened = introducer.len_utf8();
    let closing = words.find(|glyph: char| glyph == '\u{1b}' || (bel && glyph == '\u{7}'));
    Some(match closing {
        Some(at) if words[at..].starts_with('\u{7}') => opened + at + 1,
        Some(at) if words[at..].starts_with("\u{1b}\\") => opened + at + 2,
        _ => opened + words.len(),
    })
}

fn sequence(drawn: &str) -> Option<Sequence> {
    let rest = drawn.strip_prefix('\u{1b}')?;
    let Some(params) = rest.strip_prefix('[') else {
        if let Some(taken) = swallowed(rest) {
            return Some(Sequence {
                move_to: None,
                shown: None,
                taken: taken + 1,
            });
        }
        // `ESC O A` and the single-shift keys: no move, and as long as its last byte.
        let taken = 1 + rest.chars().next().map_or(0, char::len_utf8);
        return Some(Sequence {
            move_to: None,
            shown: None,
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
    let shown = match (last, &params[..end]) {
        ('h', "?25") => Some(true),
        ('l', "?25") => Some(false),
        _ => None,
    };
    Some(Sequence {
        move_to,
        shown,
        taken,
    })
}

/// One run of glyphs the stream wrote without the cursor leaving them. A frame paints only the cells
/// that changed, so a picture is replayed from these rather than read off the bytes that carried it.
struct Run {
    row: u16,
    column: u16,
    glyphs: String,
}

fn runs(drawn: &str) -> Vec<Run> {
    let (mut row, mut column) = (0u16, 0u16);
    let mut moved = false;
    let mut runs: Vec<Run> = Vec::new();
    for mark in painted(drawn) {
        match mark {
            Painted::Moved(at, col) => {
                row = at;
                column = col;
                moved = true;
            }
            Painted::Glyph(glyph) => {
                match runs.last_mut() {
                    Some(run) if !moved => run.glyphs.push(glyph),
                    _ => runs.push(Run {
                        row,
                        column,
                        glyphs: glyph.to_string(),
                    }),
                }
                moved = false;
                column = column.saturating_add(1);
            }
            Painted::Shown(_) => {}
        }
    }
    runs
}

/// The rows the composer's band took in the last frame that painted it: the gap between the row of
/// its corner and the row of the footer's first hint. The corner is the one drawn in the frame's
/// first column: a rail and a transcript both draw rules of their own after the composer's.
fn composer_rows(drawn: &str) -> Option<u16> {
    let (mut edge, mut footer) = (None, None);
    for run in runs(drawn) {
        if run.glyphs.starts_with('┌') && run.column == 1 {
            edge = Some(run.row);
        }
        if run.glyphs.contains(FIRST_HINT) {
            footer = Some(run.row);
        }
    }
    Some(footer? - edge?)
}

fn row_of(drawn: &str, needle: &str) -> Option<u16> {
    runs(drawn)
        .into_iter()
        .rfind(|run| run.glyphs.contains(needle))
        .map(|run| run.row)
}

/// The frame as the terminal holds it after every frame drawn so far: each row that has a glyph on it,
/// and the last write to a cell winning over the ones before it.
fn picture(drawn: &str) -> BTreeMap<u16, String> {
    let mut rows: BTreeMap<u16, BTreeMap<u16, char>> = BTreeMap::new();
    for run in runs(drawn) {
        let cells = rows.entry(run.row).or_default();
        let mut column = run.column;
        for glyph in run.glyphs.chars() {
            cells.insert(column, glyph);
            column = column.saturating_add(1);
        }
    }
    rows.into_iter()
        .map(|(row, cells)| {
            let first = *cells.keys().next().expect("a row holds a glyph");
            let last = *cells.keys().next_back().expect("a row holds a glyph");
            let text = (first..=last)
                .map(|column| cells.get(&column).copied().unwrap_or(' '))
                .collect();
            (row, text)
        })
        .collect()
}

/// Whether a row of the screen the terminal holds says `needle`: a card's word is painted over the
/// frame behind it, and painting the frame again takes it off.
fn showing(drawn: &str, needle: &str) -> bool {
    picture(drawn).values().any(|row| row.contains(needle))
}

/// A row of a screen with the card's border and inset taken off it, so a row of facts is read as
/// the screen draws it and not as the card frames it.
fn inside(row: &str) -> &str {
    row.trim_matches(['\u{2502}', ' '])
}

/// Whether the rail says `value` at the end of the row `label` heads. The blanks between the
/// glyphs are the layout's and not the fact's, so they are dropped from both sides: a block pads
/// its pairs into one slash column, and a path shorter than its column is drawn whole.
fn rail_reads(drawn: &str, label: &str, value: &str) -> bool {
    fn tightened(text: &str) -> String {
        text.chars()
            .filter(|glyph| !glyph.is_whitespace())
            .collect()
    }
    let asked = tightened(value);
    picture(drawn).values().any(|row| {
        row.split_once(&format!("{label} "))
            .is_some_and(|(_, rest)| tightened(rest).ends_with(&asked))
    })
}

/// The cell the last frame placed the terminal's own cursor in: a show, and then the move that
/// follows it with no glyph painted between them, which is how a frame that places a cursor ends.
fn placed(drawn: &str) -> Option<(u16, u16)> {
    let (mut shown, mut placed) = (false, None);
    for mark in painted(drawn) {
        match mark {
            Painted::Shown(on) => shown = on,
            Painted::Moved(row, column) if shown => {
                placed = Some((row, column));
                shown = false;
            }
            Painted::Moved(..) => {}
            Painted::Glyph(_) => shown = false,
        }
    }
    placed
}

/// Whether the bytes ask the terminal for a cursor shape or a cursor colour: the one thing a launch
/// never says, because the shape and the colour of its cursor are the reader's to choose.
fn asked_a_cursor(drawn: &str) -> bool {
    drawn.contains("\u{1b}]12;")
        || drawn.match_indices("\u{1b}[").any(|(at, _)| {
            let params = &drawn[at + 2..];
            let end = params.find(|glyph: char| ('\u{40}'..='\u{7e}').contains(&glyph));
            end.is_some_and(|end| params.as_bytes()[end] == b'q')
        })
}

#[test]
fn the_launch_places_the_terminals_own_caret_on_the_cell_the_draft_waits_in() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    pty.type_bytes(b"a draft");
    assert!(
        drawn.wait_until(|frame| {
            picture(frame)
                .values()
                .any(|glyphs| glyphs.contains("a draft"))
        }),
        "the typed glyphs are drawn in the band: {:?}",
        drawn.text()
    );
    let frame = drawn.text();
    let row = picture(&frame)
        .into_iter()
        .find(|(_, glyphs)| glyphs.contains("a draft"))
        .map(|(row, _)| row)
        .expect("the draft's glyphs are drawn on its row");
    assert_eq!(
        placed(&frame),
        Some((row, GUTTER + 8)),
        "the terminal's own caret is placed one column past the seven glyphs typed, on the \
         draft's own row: {frame:?}"
    );
    assert!(
        !asked_a_cursor(&frame),
        "and the launch asks for no shape and no colour, so the reader's own cursor is what they \
         see: {frame:?}"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "and the launch is still there to quit"
    );
}

#[test]
fn a_typed_glyph_and_an_erasure_move_the_placed_caret() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"abc");
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == GUTTER + 4)),
        "three glyphs typed wait the caret one column past the last: {:?}",
        drawn.text()
    );
    pty.type_bytes(&[0x7f]);
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == GUTTER + 3)),
        "an erased glyph takes the caret back with it: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "and the launch is still there to quit"
    );
}

#[test]
fn a_move_along_the_draft_moves_the_placed_caret_and_no_glyph() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"abc");
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == GUTTER + 4)),
        "three glyphs typed wait the caret one column past the last: {:?}",
        drawn.text()
    );
    let typed = drawn.text();

    pty.type_bytes(b"\x1b[D");
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == GUTTER + 3)),
        "a step back along the draft takes the caret back over a glyph: {:?}",
        drawn.text()
    );
    assert_eq!(
        picture(&drawn.text()),
        picture(&typed),
        "and no glyph of the frame changed: the caret moved and the draft did not"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "and the launch is still there to quit"
    );
}

#[test]
fn a_screen_over_the_band_places_no_caret_and_hides_the_terminals_own() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"?");
    assert!(
        drawn.wait_until(|frame| showing(frame, "Backspace")),
        "the keys screen is drawn over the band: {:?}",
        drawn.text()
    );

    let frame = drawn.text();
    assert!(
        frame.ends_with("\u{1b}[?25l"),
        "a screen takes the keys the draft behind it does not, so the frame places no caret and \
         the terminal's own is hidden rather than left standing on a cell nobody can type in: \
         {frame:?}"
    );

    pty.type_bytes(b"\x1b");
    assert!(
        drawn.wait_until(|frame| !showing(frame, "Backspace")),
        "Esc gives the frame back: {:?}",
        drawn.text()
    );
    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "and the launch is still there to quit"
    );
}

#[test]
fn a_card_that_takes_a_query_places_the_caret_past_the_glyphs_typed_at_it() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    let booted = placed(&drawn.text()).expect("the draft waits its caret at the boot frame");

    pty.type_bytes(b"@");
    assert!(
        drawn.wait_until(|frame| showing(frame, "query: ")),
        "the `@` card is drawn over the band: {:?}",
        drawn.text()
    );
    let opened = drawn.text();
    let query = row_of(&opened, "query: ").expect("the card draws the row its query is typed at");
    let (row, column) = placed(&opened).expect("a card that takes a query places the caret in it");
    let words = runs(&opened)
        .into_iter()
        .find(|run| run.glyphs.contains("query: "))
        .expect("the card draws the words its query is typed after");
    let at = words
        .glyphs
        .find("query: ")
        .expect("the words open the run they are drawn in");
    assert_eq!(
        (row, column),
        (
            words.row,
            words.column + pen::cells(&words.glyphs[..at]) + pen::cells("query: ")
        ),
        "the caret waits on the row the query is typed at, one column past the words `query: `: \
         {opened:?}"
    );

    pty.type_bytes(b"rs");
    assert!(
        drawn.wait_until(|frame| placed(frame) == Some((query, column + 2))),
        "two glyphs typed wait the caret two columns on, on that same row: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x7f]);
    assert!(
        drawn.wait_until(|frame| placed(frame) == Some((query, column + 1))),
        "an erased glyph takes it one column back: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"\x1b");
    assert!(
        drawn.wait_until(|frame| placed(frame) == Some(booted)),
        "Esc closes the card and hands the caret back to the draft's own row: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "and the launch is still there to quit"
    );
}

#[test]
fn the_placed_caret_follows_the_row_the_draft_window_scrolls_to() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    let draft = "line one\rline two\rline three\rline four\rline five\rline six\rline seven\r\
                 line eight\rline nine";
    pty.type_bytes(format!("\u{1b}[200~{draft}\u{1b}[201~").as_bytes());

    assert!(
        drawn.wait_until(|frame| {
            placed(frame).is_some_and(|(row, _)| row_of(frame, "line nine") == Some(row))
        }),
        "nine lines of draft are more rows than the band draws, so its window scrolls to the \
         row the caret waits on: {:?}",
        drawn.text()
    );
    let frame = drawn.text();
    let last = runs(&frame)
        .into_iter()
        .rfind(|run| run.glyphs.contains("line nine"))
        .expect("the last line of the draft is drawn");
    let words = pen::cells(
        &last.glyphs[..last
            .glyphs
            .find("line nine")
            .expect("the words of the last line")],
    );
    assert_eq!(
        placed(&frame),
        Some((last.row, last.column + words + 9)),
        "and the caret stands one column past the words of that last line, on the row they are \
         drawn on: {frame:?}"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "and the launch is still there to quit"
    );
}

#[test]
fn a_caret_at_the_boxes_right_margin_waits_on_that_column() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    let edge = runs(&drawn.text())
        .into_iter()
        .find(|run| run.glyphs.starts_with('\u{250c}'))
        .expect("the box's edge is drawn on the boot frame");
    let upright =
        edge.column + u16::try_from(edge.glyphs.chars().count()).expect("a count of glyphs") - 1;
    // A draft as wide as the box's room: the box less its two uprights and the two cells the gutter
    // is set from.
    let room = upright - pen::cells("  ") - 2;
    let draft = format!("\u{1b}[200~{}\u{1b}[201~", "x".repeat(usize::from(room)));
    pty.type_bytes(draft.as_bytes());

    assert!(
        drawn.wait_until(|frame| picture(frame)
            .values()
            .any(|row| row.matches('x').count() == usize::from(room))),
        "the whole draft reached one row of the frame: {:?}",
        drawn.text()
    );
    let frame = drawn.text();
    assert_eq!(
        placed(&frame).map(|(_, column)| column),
        Some(upright - 1),
        "a caret past the box's last column waits on the column just inside the right upright, \
         which is drawn at {upright}: {frame:?}"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(
        session.wait().code(),
        Some(0),
        "and the launch is still there to quit"
    );
}

/// Whether the screen is given back with the terminal's own cursor shown on the way out: the paste
/// report given up, then the cursor, and the screen left last, which is the order a shell reads.
fn gave_back_showing_the_cursor(bytes: &[u8]) -> bool {
    let given = bytes
        .windows(b"\x1b[?2004l".len())
        .rposition(|window| window == b"\x1b[?2004l");
    given.is_some_and(|given| {
        let tail = &bytes[given..];
        matches!(
            (at(tail, b"\x1b[?25h"), at(tail, b"\x1b[?1049l")),
            (Some(shown), Some(screen)) if shown < screen
        )
    })
}

#[test]
fn a_quit_gives_the_terminal_back_with_the_cursor_shown() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
    assert!(
        drawn.wait_for(b"\x1b[?1049l"),
        "the screen is given back: {:?}",
        drawn.text()
    );
    assert!(
        gave_back_showing_the_cursor(&drawn.bytes()),
        "and the cursor is shown before the screen goes, so a shell is handed a caret it can \
         see: {:?}",
        drawn.text()
    );
}

#[test]
fn a_bracketed_paste_arrives_as_one_draft_with_its_line_breaks_kept() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    assert_eq!(
        composer_rows(&drawn.text()),
        Some(3),
        "the composer starts at its three-row floor: the rule, the row the draft sits on \
         and the foot: {:?}",
        drawn.text()
    );

    // One paste, the way a terminal that offers a bracketed paste sends it: the line breaks are
    // the clipboard's, and not one of them is a pressed Enter.
    pty.type_bytes(b"\x1b[200~first line\rsecond line\r\nthird line\x1b[201~");

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
    assert_eq!(
        composer_rows(&frame),
        Some(7),
        "three lines make a seven-row composer: {frame:?}"
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
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    // A terminal with no bracketed paste sends the whole prompt as the bytes a typist would have
    // made, carriage return and all.
    pty.type_bytes(b"one\rtwo");

    assert!(
        drawn.wait_until(|frame| composer_rows(frame) == Some(6)),
        "two lines in one draft: the box grew a row for the break a paste kept rather \
         than sent: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0));
}

#[test]
fn a_reported_ctrl_enter_breaks_the_line_rather_than_sending() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"one");
    pty.type_bytes(b"\x1b[13;5u");
    pty.type_bytes(b"two");

    assert!(
        drawn.wait_until(|frame| composer_rows(frame) == Some(6)),
        "Ctrl+Enter is a line break and not the key that sends: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0));
}

#[test]
fn a_key_the_protocol_reports_by_name_is_the_key_the_terminal_speaks() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"abc");
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == GUTTER + 4)),
        "three glyphs typed, so the caret waits one column past the last: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"\x1b[106;5u");

    assert!(
        drawn.wait_until(|frame| composer_rows(frame) == Some(6)),
        "the reported Ctrl-J broke the line rather than sending it: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"\x1b[99;5u");

    assert!(
        drawn.wait_until(|frame| {
            composer_rows(frame) == Some(3)
                && placed(frame).is_some_and(|(_, column)| column == EMPTY_CARET)
        }),
        "the reported Ctrl-C cleared the draft the byte would have cleared: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"\x1b[113;5u");
    assert_eq!(
        session.wait().code(),
        Some(0),
        "and the reported Ctrl-Q still quits: {:?}",
        drawn.text()
    );
}

#[test]
fn a_reported_shift_enter_breaks_the_line_rather_than_sending() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"one");
    pty.type_bytes(b"\x1b[13;2u");
    pty.type_bytes(b"two");

    assert!(
        drawn.wait_until(|frame| composer_rows(frame) == Some(6)),
        "Shift+Enter is a newline and not the key that sends: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0));
}

#[test]
fn a_terminal_that_reports_nothing_is_handed_the_same_frame_and_the_same_draft() {
    let ground = Ground::new();

    let reported = drafted(&ground, &[b"one", b"\x1b[13;2u", b"two"]);
    let spoken = drafted(&ground, &[b"one", b"\n", b"two"]);

    assert_eq!(
        reported, spoken,
        "a terminal that speaks no protocol is handed the frame the one that does is handed"
    );
}

/// The frame a launch holds once a draft is typed into it as the given keys, and the launch quit:
/// two spells of one draft are compared on what a terminal was written.
fn drafted(ground: &Ground, keys: &[&[u8]]) -> String {
    let (session, mut pty, drawn) = Session::open(ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    for key in keys {
        pty.type_bytes(key);
    }
    assert!(
        drawn.wait_until(|frame| composer_rows(frame) == Some(6)),
        "two lines of draft: {:?}",
        drawn.text()
    );
    let frame = String::from_utf8_lossy(&drawn.settled()).into_owned();
    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0));

    frame
}

#[test]
fn enter_takes_the_draft_out_of_the_composer() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    pty.type_bytes(b"one");
    pty.type_bytes(b"\x1b[13;2u");
    pty.type_bytes(b"two");
    assert!(
        drawn.wait_until(|frame| composer_rows(frame) == Some(6)),
        "two lines of draft: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"\r");

    assert!(
        drawn.wait_until(|frame| composer_rows(frame) == Some(3)),
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
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    pty.type_bytes(b"a draft nobody sent");
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == 20 + GUTTER)),
        "nineteen glyphs typed, so the caret waits one column past the last: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x03]);

    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == EMPTY_CARET)),
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
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"?");
    assert!(
        drawn.wait_until(|frame| showing(frame, "Backspace")),
        "the keys screen lists the draft's keys, Backspace among them: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"\x1b");
    assert!(
        drawn.wait_until(|frame| !showing(frame, "Backspace")),
        "Esc closed the screen, and the frame the card was drawn over is back in its own ink: \
         {:?}",
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

fn directories(state: &std::path::Path) -> Vec<PathBuf> {
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
    found
}

fn document_in(directory: &Path) -> String {
    std::fs::read_to_string(directory.join("session.json")).expect("the session document")
}

fn document(state: &std::path::Path) -> String {
    let found = directories(state);
    let directory = found.first().expect("the launch recorded a session");
    document_in(directory)
}

/// The entries a session's transcript holds, one line each, in the order they were written.
fn transcript(directory: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(directory.join("transcript.jsonl"))
        .expect("the session's transcript")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("a transcript's line is JSON"))
        .collect()
}

/// The prompts a session was asked, in the order they were asked.
fn prompts(directory: &Path) -> Vec<String> {
    transcript(directory)
        .iter()
        .filter(|entry| entry["kind"] == "prompt")
        .map(|entry| String::from(entry["text"].as_str().expect("a prompt is words")))
        .collect()
}

/// The model a request's body names, which is the name the wire was asked with.
fn model_of(request: &str) -> String {
    let (_, body) = request
        .split_once("\r\n\r\n")
        .expect("a request's head and its body");
    let body: serde_json::Value = serde_json::from_str(body).expect("a wire's body is JSON");
    String::from(body["model"].as_str().expect("a request names the model"))
}

fn guid(state: &std::path::Path) -> String {
    let document: serde_json::Value =
        serde_json::from_str(&document(state)).expect("the session document is JSON");
    String::from(
        document["session"]
            .as_str()
            .expect("a document names its session"),
    )
}

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

/// The columns the composer sets a draft in before the first of its words: the band's upright, the
/// sigil and the blank after it.
const GUTTER: u16 = 3;

/// The column an empty draft's caret waits in: the placeholder's first glyph, one past the gutter's
/// blank cell.
const EMPTY_CARET: u16 = GUTTER + 1;

/// Types a draft and sends it, and waits until the run it started has a request in flight. The caret
/// is what the draft is waited on and not the words: a glyph typed is a glyph painted, so a caret
/// past the last of them is the one drawing that says the whole draft is in the box.
fn ask_words(pty: &mut Pty, drawn: &Drawn, wire: &Wire, words: &str) {
    pty.type_bytes(words.as_bytes());
    let typed = pen::cells(words) + 1 + GUTTER;
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == typed)),
        "{words:?} typed, so the caret waits one column past its last glyph: {:?}",
        drawn.text()
    );
    pty.type_bytes(b"\r");
    wire.wait_for_request();
}

fn ask(pty: &mut Pty, drawn: &Drawn, wire: &Wire) {
    ask_words(pty, drawn, wire, "triage this");
}

/// Waits for the recorded answer and for the run that said it to stop, so a test types its next key
/// into a launch whose run has settled: the words land while the run is still live, and the settled
/// marker is the check a live one has not drawn.
fn answered(drawn: &Drawn) {
    assert!(
        drawn.wait_until(|frame| ANSWER_WORDS.iter().all(|word| frame.contains(word))
            && frame.contains('\u{2713}')),
        "the run answered and stopped: {:?}",
        drawn.text()
    );
}

#[test]
fn ctrl_c_in_a_run_cancels_it_and_keeps_the_draft() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);

    pty.type_bytes(b"a second thought");
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == 17 + GUTTER)),
        "sixteen glyphs of a new draft, and the caret one column past them: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x03]);

    thread::sleep(Duration::from_millis(300));
    assert!(
        placed(&drawn.text()).is_some_and(|(_, column)| column == 17 + GUTTER),
        "the draft a cancel found is still in the composer: {:?}",
        drawn.text()
    );

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

    // The frame learns the run ended from the marker the run settles on, and not from the record:
    // a key typed while that report is still in flight would read as one pressed during a live run.
    assert!(
        drawn.wait_until(|frame| frame.contains('\u{2717}')),
        "the cancelled run's marker has settled: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x03]);
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == EMPTY_CARET)),
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

#[test]
fn a_cancelled_run_settles_as_cancelled_and_the_settled_frame_is_left_alone() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);

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

    pty.type_bytes(&[0x03]);

    assert!(
        drawn.wait_until(|frame| frame.contains("cancelled")),
        "a cancelled run settles as cancelled: {:?}",
        drawn.text()
    );
    wire.release();

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

#[test]
fn a_live_run_draws_the_facts_it_was_handed_and_its_first_turn_moves_them() {
    let ground = Ground::new();
    on_a_branch(&ground);
    let wire = Wire::holding_a_cached_reply();
    let mut command = ground.command();
    command
        .arg("--yolo-unsafe")
        .env("SINDRI_BASE_URL", wire.base_url())
        .env("SINDRI_CONTEXT_WINDOW", "200000");
    let (session, mut pty, drawn) = Session::starting(command);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    assert_eq!(
        row_of(&drawn.text(), "turns"),
        None,
        "a frame before any run knows no fact and draws no rail: {:?}",
        drawn.text()
    );

    ask(&mut pty, &drawn, &wire);

    assert!(
        drawn.wait_until(|frame| rail_reads(frame, "turns", "0 / 250")),
        "the frame a run's first turn is in flight from draws the facts the run was \
         opened with, and the cap with no turn spent: {:?}",
        drawn.text()
    );
    let recorded = guid(&ground.state);
    for (label, value) in [
        ("wire", "anthropic"),
        ("model", "claude-sonnet-4-6"),
        ("think", "off"),
        ("workspace", "/workspace"),
        ("branch", "main"),
        ("sandbox", "unsafe"),
        ("session", &recorded[..8]),
    ] {
        assert!(
            rail_reads(&drawn.text(), label, value),
            "the rail draws `{label} {value}` on the frame the run opened: {:?}",
            drawn.text()
        );
    }
    assert_eq!(
        row_of(&drawn.text(), "ctx"),
        None,
        "no reply has reported a size, so there is no ctx row to read even though a \
         window is named: {:?}",
        drawn.text()
    );
    assert_eq!(
        row_of(&drawn.text(), "cache"),
        None,
        "and no reply has reported what the provider served from its own cache: {:?}",
        drawn.text()
    );

    wire.release();

    assert!(
        drawn.wait_until(|frame| {
            rail_reads(frame, "turns", "1 / 250")
                && rail_reads(frame, "ctx", "░░░░░░ 11 / 200k")
                && rail_reads(frame, "cache", "36%")
        }),
        "the turn the run took is counted against the cap, the size the wire reported is \
         drawn against the window the config named, and the share of that input the \
         provider served from its own cache beside it: {:?}",
        drawn.text()
    );
    assert!(
        drawn.wait_until(|frame| frame.contains('✓')),
        "the run answered and settled on its marker: {:?}",
        drawn.text()
    );

    let frame = drawn.text();
    for (label, value) in [
        ("turns", "1 / 250"),
        ("ctx", "░░░░░░ 11 / 200k"),
        ("cache", "36%"),
        ("workspace", "/workspace"),
        ("branch", "main"),
        ("session", &recorded[..8]),
    ] {
        assert!(
            rail_reads(&frame, label, value),
            "the settled frame draws `{label} {value}`: {frame:?}"
        );
    }

    pty.type_bytes(b"\x11");
    assert_eq!(
        session.wait().code(),
        Some(0),
        "the launch outlives the run whose facts it drew: {:?}",
        drawn.text()
    );
    assert!(
        drawn.wait_for(b"\x1b[?1049l"),
        "and gives the screen back: {:?}",
        drawn.text()
    );
}

#[test]
fn the_stats_screen_draws_the_record_the_run_handed_its_frame() {
    let ground = Ground::new();
    on_a_branch(&ground);
    let wire = Wire::holding_a_cached_reply();
    let mut launch = ground.command();
    launch
        .arg("--yolo-unsafe")
        .env("SINDRI_BASE_URL", wire.base_url());
    let (session, mut pty, drawn) = Session::starting(launch);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);
    wire.release();
    assert!(
        drawn.wait_until(|frame| frame.contains('\u{2713}')),
        "the run answered and stopped: {:?}",
        drawn.text()
    );

    ask_words(&mut pty, &drawn, &wire, "and what did you just say?");
    wire.release();
    assert!(
        turn_counted(&ground.state, 2),
        "a second prompt ran on the session the first one opened: {}",
        document(&ground.state)
    );

    let recorded = guid(&ground.state);
    command(&mut pty, &drawn, "/stats");

    assert!(
        drawn.wait_until(|frame| picture(frame).values().any(|row| row.contains(&recorded))),
        "the screen draws the session's GUID in all thirty-six characters, which is a fact \
         the rail has no column for: {:?}",
        drawn.text()
    );
    for label in [
        "input", "output", "total", "cached", "stopped", "start", "end", "commit",
    ] {
        assert!(
            drawn.wait_until(|frame| {
                picture(frame)
                    .values()
                    .any(|row| inside(row).starts_with(label))
            }),
            "the screen draws `{label}`, which is a row only the record the run wrote has: {:?}",
            drawn.text()
        );
    }
    assert!(
        picture(&drawn.text()).values().any(|row| {
            let row = inside(row);
            row.starts_with("stopped") && row.trim_end().ends_with("final")
        }),
        "and the record says how the run stopped: {:?}",
        drawn.text()
    );
    for (label, value) in [("turns", "2 / 250"), ("input", "22")] {
        assert!(
            picture(&drawn.text()).values().any(|row| {
                let row = inside(row);
                row.starts_with(label) && row.contains(value)
            }),
            "the screen draws `{label} {value}`, the session's own count and not the second \
             prompt's alone: {:?}",
            drawn.text()
        );
    }

    pty.type_bytes(b"\x1b");
    assert!(
        drawn.wait_until(|frame| !showing(frame, &recorded)),
        "Esc closes the screen, and the card's rows are the frame it was drawn over again: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"\x11");
    assert_eq!(
        session.wait().code(),
        Some(0),
        "the launch outlives the screen it drew: {:?}",
        drawn.text()
    );
}

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
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);

    let interrupted = session.interrupt();
    assert!(interrupted, "the signal was delivered to the launch");

    // The signal and the wire's answer are two roads into the same run, and the run stops on an
    // answer that lands first: the word is given time to be heard before the answer is let go.
    thread::sleep(Duration::from_millis(200));
    wire.release();
    let status = session.wait();
    assert_eq!(
        status.code(),
        Some(130),
        "a launch the user's word ended: {:?}",
        drawn.text()
    );
    assert!(
        drawn.wait_for(b"\x1b[?1049l"),
        "the alternate screen is given back on a signal as it is on a panic: {:?}",
        drawn.text()
    );
    assert!(
        gave_back_showing_the_cursor(&drawn.bytes()),
        "with the cursor shown, a signal's way out as a quit's: {:?}",
        drawn.text()
    );
    assert!(
        recorded(&ground.state, r#""stopped": "cancelled""#),
        "and the record says what stopped the run: {}",
        document(&ground.state)
    );
    assert!(
        recorded(&ground.state, r#""end":"#),
        "a launch the user's word ended settles its record with an end: {}",
        document(&ground.state)
    );
    let directory = directories(&ground.state)
        .into_iter()
        .next()
        .expect("the interrupted launch kept its session");
    assert_eq!(
        std::fs::read_dir(directory.join("tmp"))
            .expect("the session's temp")
            .count(),
        0,
        "and empties the temp as a run that stopped does, keeping the session it recorded"
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

/// Types `prompt` and sends it, and waits until the box is empty again: the trace a prompt typed
/// while a run is live leaves when it goes into the queue instead of dying with the draft.
fn queue(pty: &mut Pty, drawn: &Drawn, prompt: &str) {
    pty.type_bytes(prompt.as_bytes());
    let typed = pen::cells(prompt) + 1 + GUTTER;
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == typed)),
        "{prompt:?} is in the box, the caret one column past its last glyph: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"\r");
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == EMPTY_CARET)),
        "{prompt:?} left the box for the queue: {:?}",
        drawn.text()
    );
}

#[test]
fn enter_during_a_run_keeps_the_prompt_in_the_composer_under_the_draft() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);

    queue(&mut pty, &drawn, "read the test that failed");

    let frame = drawn.text();
    let waited = picture(&frame)
        .into_iter()
        .find(|(_, glyphs)| glyphs.contains("\u{203a} read the test that failed"))
        .map(|(row, _)| row);
    assert!(
        waited
            .zip(placed(&frame))
            .is_some_and(|(waited, (caret, _))| waited > caret),
        "the frame draws the prompt waiting under the row the caret waits on: {frame:?}"
    );
    assert_eq!(
        composer_rows(&frame),
        Some(4),
        "and it is a row of the composer's own band: the draft's, the prompt's and the box's \
             own rule and foot, with no row of air either side of a draft that sits on one row"
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
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);

    queue(&mut pty, &drawn, "second");
    queue(&mut pty, &drawn, "third");

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
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);
    queue(&mut pty, &drawn, "keep this");
    queue(&mut pty, &drawn, "drop this");

    // The `x` typed straight after `Ctrl-X` is the proof the key was answered: the loop answers one
    // key before the next, so a glyph the frame painted is a `Ctrl-X` already acted on.
    pty.type_bytes(&[0x18]);
    pty.type_bytes(b"x");
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == GUTTER + 2)),
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
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);
    queue(&mut pty, &drawn, "keep this");
    queue(&mut pty, &drawn, "the newest");

    pty.type_bytes(b"\x1b[A");
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == 11 + GUTTER)),
        "the recalled prompt is the whole draft again: {:?}",
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
        asked.iter().all(|request| !request.contains("the newest")),
        "the recalled prompt is a draft and not a run: {asked:?}"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

/// A `/` line typed and sent: the words, and then `Enter` as a key of its own once the frame has
/// caught up. A burst of bytes is read as a paste, where a line break is a line break and never a
/// send, so a command line is only sent by letting the draft land first.
fn command(pty: &mut Pty, drawn: &Drawn, line: &str) {
    pty.type_bytes(line.as_bytes());
    let column = pen::cells(line) + 1 + GUTTER;
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, at)| at == column)),
        "the command line is the draft before it is sent, so the caret waits one column \
         past its last glyph: {:?}",
        drawn.text()
    );
    pty.type_bytes(b"\r");
}

/// The config file a launch would write, holding a reader's own comment and a key the save must not
/// touch: what a save that patched the whole file would destroy.
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

/// Waits until the config at `path` says `model = <name>`, and hands the file back: the save happens
/// on the launch's own thread, so a test waits for the file rather than for a sleep.
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
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

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

    pty.type_bytes(b"haiku");
    assert!(
        drawn.wait_until(|frame| {
            row_of(frame, "Claude Haiku 4.5") == row_of(frame, "query:").map(|query| query + 1)
        }),
        "the names are ranked as the query is typed, so the haiku model is the first row \
         under the query and the sonnet model is no row at all: {:?}",
        drawn.text()
    );
    pty.type_bytes(b"\r");

    let written = written_model(&config, SECOND_SERVED);
    assert!(
        written.contains("# the model I like"),
        "the save patched the one key it touched and kept the reader's comment: \
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
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    command(&mut pty, &drawn, "/model a-name-the-wire-never-served");

    let written = written_model(&config, "a-name-the-wire-never-served");
    assert!(
        written.contains("# the model I like"),
        "by the same owner-only save, patching the same one key: {written}"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

/// A launch that takes its model from the config file alone: the environment outranks the file, so a
/// `/model` the file holds is the name the next run is asked with only where nothing exported one.
fn reading_the_model_from_the_config(ground: &Ground, wire: &Wire) -> (Session, Pty, Drawn) {
    let mut command = ground.command();
    command
        .arg("--yolo-unsafe")
        .env_remove("SINDRI_MODEL")
        .env("SINDRI_BASE_URL", wire.base_url());
    Session::starting(command)
}

#[test]
fn a_model_chosen_mid_launch_supersedes_the_session_and_the_next_prompt_runs_on_it() {
    let ground = Ground::new();
    let config = a_readers_config(&ground);
    let wire = Wire::holding();
    let (session, mut pty, drawn) = reading_the_model_from_the_config(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask_words(&mut pty, &drawn, &wire, "hi my name is Glenn");
    wire.release();
    assert!(
        turn_counted(&ground.state, 1),
        "the first prompt is recorded: {}",
        document(&ground.state)
    );
    assert_eq!(
        model_of(&wire.requests()[0]),
        FIRST_SERVED,
        "the model the config named is the one the first prompt was asked with"
    );

    command(&mut pty, &drawn, "/model claude-haiku-4-5");
    written_model(&config, SECOND_SERVED);

    ask_words(&mut pty, &drawn, &wire, "what is my name?");

    let asked = wire.requests();
    assert_eq!(asked.len(), 2, "two prompts, two requests: {asked:?}");
    assert_eq!(
        model_of(&asked[1]),
        SECOND_SERVED,
        "the prompt after the visit is asked with the name the visit wrote"
    );

    wire.release();
    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());

    let found = directories(&ground.state);
    assert_eq!(
        found.len(),
        2,
        "a session records one wire, one model and one route, so a run under another model is a \
         session of its own: {found:?}"
    );
    let mut named: Vec<String> = found
        .iter()
        .map(|directory| document_in(directory))
        .map(|said| {
            let document: serde_json::Value =
                serde_json::from_str(&said).expect("the session document is JSON");
            String::from(
                document["model"]
                    .as_str()
                    .expect("a document names its model"),
            )
        })
        .collect();
    named.sort();
    assert_eq!(
        named,
        [SECOND_SERVED, FIRST_SERVED],
        "each record names the model its own turns were asked with, and no record names two"
    );
}

#[test]
fn slash_help_opens_the_keys_in_force_and_slash_settings_its_own_screen() {
    let ground = Ground::new();
    let (session, mut pty, drawn) = Session::open(&ground);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    // `Backspace` is a word only the keys screen says: the footer the composer keeps names two hints,
    // and the screen that lists every key in force names this one.
    command(&mut pty, &drawn, "/help");
    assert!(
        drawn.wait_until(|frame| showing(frame, "Backspace")),
        "`/help` opens the screen that lists the keys in force: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"\x1b");
    assert!(
        drawn.wait_until(|frame| !showing(frame, "Backspace")),
        "Esc closed the keys screen: {:?}",
        drawn.text()
    );
    command(&mut pty, &drawn, "/settings");
    assert!(
        drawn.wait_until(|frame| showing(frame, "╭─ /settings")),
        "`/settings` opens its own screen, its name in the top border of the card it is drawn \
         on: {:?}",
        drawn.text()
    );

    pty.type_bytes(b"\x1b");
    assert!(
        drawn.wait_until(|frame| !showing(frame, "╭─ /settings")),
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
        drawn.wait_for(FIRST_KEY),
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
fn the_rail_keeps_its_session_its_turns_and_its_size_across_two_prompts() {
    let ground = Ground::new();
    let wire = Wire::holding_a_cached_reply();
    let mut launch = ground.command();
    launch
        .arg("--yolo-unsafe")
        .env("SINDRI_BASE_URL", wire.base_url())
        .env("SINDRI_CONTEXT_WINDOW", "200000");
    let (session, mut pty, drawn) = Session::starting(launch);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    ask(&mut pty, &drawn, &wire);
    let recorded = guid(&ground.state);
    assert!(
        drawn.wait_until(|frame| rail_reads(frame, "session", &recorded[..8])
            && rail_reads(frame, "turns", "0 / 250")),
        "the session the first prompt opened is the fact the rail begins with: {:?}",
        drawn.text()
    );

    wire.release();
    assert!(
        drawn.wait_until(|frame| {
            rail_reads(frame, "turns", "1 / 250") && rail_reads(frame, "ctx", "░░░░░░ 11 / 200k")
        }),
        "the turn that ended is counted and the size it reported is drawn: {:?}",
        drawn.text()
    );

    ask_words(&mut pty, &drawn, &wire, "what is my name?");

    let frame = drawn.text();
    let temp = format!("{}/tmp", &recorded[..8]);
    for (label, value) in [
        ("session", &recorded[..8]),
        ("temp", &temp),
        ("turns", "1 / 250"),
        ("ctx", "░░░░░░ 11 / 200k"),
    ] {
        assert!(
            rail_reads(&frame, label, value),
            "the second prompt's frame keeps `{label} {value}`: the session the prompt works in, \
             the temp it writes in, the turn it counts and the size it last measured do not begin \
             again: {frame:?}"
        );
    }

    wire.release();
    assert!(
        drawn.wait_until(|frame| rail_reads(frame, "turns", "2 / 250")),
        "the second prompt's turn is counted on the same session, so the rail reads two: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
    assert_eq!(
        directories(&ground.state).len(),
        1,
        "and both prompts are one session on disk"
    );
}

#[test]
fn a_file_in_the_session_temp_outlives_the_next_prompt_and_goes_when_the_launch_ends() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    ask_words(&mut pty, &drawn, &wire, "hi my name is Glenn");
    let directory = directories(&ground.state)
        .into_iter()
        .next()
        .expect("the prompt the launch answered opened a session");
    let scratch = directory.join("tmp").join("scratch.txt");
    std::fs::write(&scratch, "kept\n").expect("a file in the session temp");
    wire.release();
    assert!(
        turn_counted(&ground.state, 1),
        "the run settled: {}",
        document_in(&directory)
    );
    assert!(
        scratch.is_file(),
        "the temp the prompt's commands wrote in is not emptied when that prompt ends"
    );

    ask_words(&mut pty, &drawn, &wire, "what is my name?");
    wire.release();
    assert!(
        turn_counted(&ground.state, 2),
        "the second prompt's run settled: {}",
        document_in(&directory)
    );
    assert!(
        recorded(&ground.state, r#""stopped": "final""#),
        "its ending is on the record before the launch is quit: {}",
        document_in(&directory)
    );
    assert!(
        scratch.is_file(),
        "the file survives the prompt after it, because the session it was written in is the one the \
         next prompt runs in: {}",
        scratch.display()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());

    assert!(
        !scratch.exists(),
        "and it is gone when the launch ends, with the conversation it was scratch for"
    );
    assert_eq!(
        directories(&ground.state).len(),
        1,
        "both prompts ran in the one session whose temp the file was written in"
    );
    assert!(
        directory.join("session.json").is_file(),
        "the record the launch wrote is kept: {}",
        directory.display()
    );
}

/// Waits until the record of the one session a launch holds counts `turns`, so a test sends its next
/// prompt into a session whose last run has stopped.
fn turn_counted(state: &std::path::Path, turns: u32) -> bool {
    recorded(state, &format!(r#""turns": {turns}"#))
}

#[test]
fn two_prompts_in_one_launch_leave_one_session_that_counts_both() {
    let ground = Ground::new();
    let wire = Wire::holding_a_cached_reply();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    ask_words(&mut pty, &drawn, &wire, "hi my name is Glenn");
    wire.release();
    assert!(
        turn_counted(&ground.state, 1),
        "the first prompt's run is written as the session's first turn: {}",
        document(&ground.state)
    );
    ask_words(&mut pty, &drawn, &wire, "what is my name?");
    wire.release();
    assert!(
        turn_counted(&ground.state, 2),
        "the second prompt's turn is counted on the same record, not in a record of its own: {}",
        document(&ground.state)
    );
    assert!(
        recorded(&ground.state, r#""stopped": "final""#),
        "and its ending, with what it spent, is on the record before the launch is quit: {}",
        document(&ground.state)
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());

    let found = directories(&ground.state);
    assert_eq!(
        found.len(),
        1,
        "two prompts in one launch are one session: {found:?}"
    );
    let recorded: serde_json::Value =
        serde_json::from_str(&document_in(&found[0])).expect("the session document is JSON");
    assert_eq!(
        recorded["turns"],
        serde_json::json!(2),
        "the record counts both turns: {recorded}"
    );
    assert_eq!(
        recorded["usage"],
        serde_json::json!({"input": 22, "output": 14, "total": 36, "cached_input": 8}),
        "and sums what both asks spent: {recorded}"
    );
    assert_eq!(
        prompts(&found[0]),
        ["hi my name is Glenn", "what is my name?"],
        "the session's transcript shows both prompts, in the order they were asked"
    );
    let listing = ground
        .command()
        .args(["sessions", "list"])
        .output()
        .expect("the listing runs on the state home the launch wrote");
    assert_eq!(
        text(&listing.stdout).lines().count(),
        1,
        "`sessions list` shows one entry for a launch that asked two prompts: {}",
        text(&listing.stdout)
    );
}

#[test]
fn slash_new_clears_the_frame_and_leaves_the_session_it_holds_alone() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    ask(&mut pty, &drawn, &wire);

    command(&mut pty, &drawn, "/new");
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == EMPTY_CARET)),
        "the command line left the composer, so it was answered rather than queued: {:?}",
        drawn.text()
    );

    let said = document(&ground.state);
    assert!(
        said.contains("\"stopped\": \"running\""),
        "`/new` empties the frame and never the session it holds, so the run in flight is still \
         the one the record says is running: {said}"
    );
    assert!(
        !said.contains("\"end\":"),
        "and the session is given no end it has not reached: {said}"
    );
    assert!(
        wire.requests().len() == 1,
        "the run in flight is not stopped by `/new`, and no second run was started: {:?}",
        wire.requests()
    );

    wire.release();
    assert!(
        drawn.wait_until(|frame| ANSWER_WORDS.iter().all(|word| frame.contains(word))),
        "the run the reader left goes on to answer: {:?}",
        drawn.text()
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

#[test]
fn a_second_prompt_in_one_launch_is_asked_with_the_first_beside_its_answer() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    ask_words(&mut pty, &drawn, &wire, "hi my name is Glenn");
    wire.release();
    answered(&drawn);

    ask_words(&mut pty, &drawn, &wire, "what is my name?");

    let asked = wire.requests();
    assert_eq!(asked.len(), 2, "two prompts, two requests: {asked:?}");
    assert_eq!(
        replayed(&asked[0]),
        [said("user", "hi my name is Glenn")],
        "the first prompt of a launch is asked with nothing before it"
    );
    assert_eq!(
        replayed(&asked[1]),
        [
            said("user", "hi my name is Glenn"),
            said("assistant", ANSWER_SAID),
            said("user", "what is my name?"),
        ],
        "the second carries the first prompt and the answer it got ahead of it"
    );

    wire.release();
    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

#[test]
fn a_second_prompt_numbers_its_turn_against_the_session() {
    let ground = Ground::new();
    let wire = Wire::holding_a_cached_reply();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    ask_words(&mut pty, &drawn, &wire, "hi my name is Glenn");
    wire.release();
    assert!(
        turn_counted(&ground.state, 1),
        "the first prompt's run is written as the session's first turn: {}",
        document(&ground.state)
    );
    ask_words(&mut pty, &drawn, &wire, "what is my name?");
    wire.release();
    assert!(
        turn_counted(&ground.state, 2),
        "the second prompt's run is written as the session's second turn: {}",
        document(&ground.state)
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());

    let found = directories(&ground.state);
    let written = transcript(&found[0]);
    let turns: Vec<u64> = written
        .iter()
        .filter(|entry| entry["kind"] == "turn")
        .map(|entry| entry["turn"].as_u64().expect("a turn is numbered"))
        .collect();
    assert_eq!(
        turns,
        [1, 2],
        "the second prompt's turn is numbered against the session and not against its own run: \
         {written:?}"
    );
}

#[test]
fn a_prompt_queued_behind_a_live_run_joins_the_conversation_at_the_turn_it_was_asked() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    ask_words(&mut pty, &drawn, &wire, "hi my name is Glenn");
    queue(&mut pty, &drawn, "what is my name?");
    queue(&mut pty, &drawn, "and where do I live?");

    wire.release();
    assert!(
        wire.wait_for_prompt("what is my name?"),
        "the prompt queued first becomes the run after it: {:?}",
        wire.requests()
    );
    wire.release();
    assert!(
        wire.wait_for_prompt("and where do I live?"),
        "and the one behind it starts next: {:?}",
        wire.requests()
    );

    let asked = wire.requests();
    assert_eq!(asked.len(), 3, "three prompts, three requests: {asked:?}");
    assert_eq!(
        replayed(&asked[1]),
        [
            said("user", "hi my name is Glenn"),
            said("assistant", ANSWER_SAID),
            said("user", "what is my name?"),
        ],
        "each queued prompt joins the conversation at the turn it was asked, behind the prompt it \
         waited on and the answer that turn got"
    );
    assert_eq!(
        replayed(&asked[2]),
        [
            said("user", "hi my name is Glenn"),
            said("assistant", ANSWER_SAID),
            said("user", "what is my name?"),
            said("assistant", ANSWER_SAID),
            said("user", "and where do I live?"),
        ],
        "and behind the turns the queue ran before it, in the order it ran them"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

#[test]
fn slash_new_empties_what_the_next_prompt_is_asked_with() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    ask_words(&mut pty, &drawn, &wire, "hi my name is Glenn");
    wire.release();
    answered(&drawn);

    command(&mut pty, &drawn, "/new");
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == EMPTY_CARET)),
        "the command line left the composer, so it was answered before a key followed it: {:?}",
        drawn.text()
    );

    ask_words(&mut pty, &drawn, &wire, "what is my name?");

    let asked = wire.requests();
    assert_eq!(asked.len(), 2, "two prompts, two requests: {asked:?}");
    assert_eq!(
        replayed(&asked[1]),
        [said("user", "what is my name?")],
        "`/new` starts the conversation again, so nothing of the prompt before it or the answer it \
         got is carried"
    );

    wire.release();
    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
    let found = directories(&ground.state);
    assert_eq!(
        found.len(),
        1,
        "and the prompt `/new` made the first of a conversation is still the launch's session: \
         {found:?}"
    );
    assert_eq!(
        prompts(&found[0]),
        ["hi my name is Glenn", "what is my name?"],
        "a record of the launch, holding both prompts it asked"
    );
}

#[test]
fn slash_new_under_a_live_run_keeps_the_turns_it_superseded_out_of_the_next_prompt() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    ask_words(&mut pty, &drawn, &wire, "hi my name is Glenn");
    queue(&mut pty, &drawn, "what is my name?");

    command(&mut pty, &drawn, "/new");
    assert!(
        drawn.wait_until(|frame| !showing(frame, "hi my name is Glenn")),
        "`/new` is answered with the frame cleared, so the run it superseded is left where it \
         was: {:?}",
        drawn.text()
    );

    wire.release();
    assert!(
        wire.wait_for_prompt("what is my name?"),
        "the queued prompt becomes the run after it: {:?}",
        wire.requests()
    );

    let asked = wire.requests();
    assert_eq!(asked.len(), 2, "two prompts, two requests: {asked:?}");
    assert_eq!(
        replayed(&asked[1]),
        [said("user", "what is my name?")],
        "the run `/new` superseded belongs to the conversation it emptied, so the turn it ends on \
         is never carried into the one that replaced it"
    );

    wire.release();
    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

#[test]
fn a_cancelled_run_leaves_the_turn_it_ended_in_what_the_next_prompt_carries() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::asking(&ground, &wire);

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );

    ask_words(&mut pty, &drawn, &wire, "hi my name is Glenn");
    pty.type_bytes(&[0x03]);
    // The key and the wire's answer are two roads into one run: a glyph typed after it is the proof
    // the cancel was answered, because the loop answers one key before the next.
    pty.type_bytes(b"x");
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == GUTTER + 2)),
        "the cancel was answered, so the draft has the glyph typed after it: {:?}",
        drawn.text()
    );
    wire.release();
    assert!(
        drawn.wait_until(|frame| frame.contains('\u{2717}')),
        "the run the reader stopped has settled as stopped: {:?}",
        drawn.text()
    );
    pty.type_bytes(&[0x03]);
    assert!(
        drawn.wait_until(|frame| placed(frame).is_some_and(|(_, column)| column == EMPTY_CARET)),
        "the run ended, so Ctrl-C is the idle key and clears the proof from the draft: {:?}",
        drawn.text()
    );

    ask_words(&mut pty, &drawn, &wire, "what is my name?");

    let asked = wire.requests();
    assert_eq!(asked.len(), 2, "two prompts, two requests: {asked:?}");
    assert_eq!(
        replayed(&asked[1]),
        [
            said("user", "hi my name is Glenn"),
            said("assistant", ANSWER_SAID),
            said("user", "what is my name?"),
        ],
        "a stop that is not final leaves the turn that ended before it in what the next prompt \
         carries"
    );

    wire.release();
    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

/// The row and the column a placement at `at` is made at, read off the cursor move it is written
/// after: the one thing that says where on the screen a plane is.
fn placed_at(bytes: &[u8], at: usize) -> (u16, u16) {
    let before = String::from_utf8_lossy(&bytes[..at]);
    let (_, moved) = before
        .rsplit_once("\x1b7\x1b[")
        .expect("a plane is placed where the cursor was saved and moved to it");
    let (row, column) = moved
        .strip_suffix('H')
        .and_then(|at| at.split_once(';'))
        .expect("the placement names the row and the column it is made at");
    (
        row.parse().expect("a row of the terminal"),
        column.parse().expect("a column of the terminal"),
    )
}

/// The columns and the rows a placement at `at` covers, read off its own header.
fn covers(bytes: &[u8], at: usize) -> (u16, u16) {
    let header = String::from_utf8_lossy(&bytes[at..]);
    let named = |key: &str| -> u16 {
        header
            .split_once(key)
            .and_then(|(_, tail)| tail.split_once(','))
            .expect("the placement names it before its pixels")
            .0
            .parse()
            .expect("a count of cells")
    };
    (named("c="), named("r="))
}

/// The frame a launch opens on is drawn before any prompt and is the bands' frame as much as any
/// other: a terminal that places pictures is handed both grounds from that first paint.
#[test]
fn the_first_frame_a_launch_draws_stands_on_the_grounds_its_bands_earn() {
    let ground = Ground::new();
    let wire = Wire::holding();
    let (session, mut pty, drawn) = Session::named(&ground, &wire, "xterm-kitty");

    assert!(
        drawn.wait_for(FIRST_KEY),
        "the boot frame is drawn: {:?}",
        drawn.text()
    );
    let bytes = drawn.bytes();
    let placed = at(&bytes, b"\x1b_Ga=T,f=100,i=333,q=2,z=-1,c=").unwrap_or_else(|| {
        panic!(
            "the band of the boot frame is placed on a ground of its own cells: {:?}",
            drawn.text()
        )
    });
    let (band, width) = (placed_at(&bytes, placed).0, covers(&bytes, placed).0);
    assert_eq!(
        placed_at(&bytes, placed).1,
        1,
        "the composer's plane is placed at the frame's first column, on the row {band} its band \
         starts at"
    );
    assert_eq!(
        covers(&bytes, placed).1,
        3,
        "and it covers the band's three rows: {:?}",
        drawn.text()
    );

    let rail = at(&bytes, b"\x1b_Ga=T,f=100,i=334,q=2,z=-1,c=").unwrap_or_else(|| {
        panic!(
            "the rail's band of the boot frame is placed on a plane of its own: {:?}",
            drawn.text()
        )
    });
    assert!(
        rail < placed,
        "and the rail is placed before the composer's band, because the rail is drawn first"
    );
    let (top, left) = placed_at(&bytes, rail);
    let (columns, rows) = covers(&bytes, rail);
    assert_eq!(top, 1, "at the frame's first row");
    assert_eq!(
        u32::from(left) + u32::from(columns) - 1,
        u32::from(width),
        "over the rail's own columns and the seam it widens across the transcript, which end at \
         the frame's right-hand edge"
    );
    assert_eq!(
        u32::from(columns),
        u32::from(Rail::columns(width)) + u32::from(SEAM),
        "and the plane takes {SEAM} columns of the transcript besides, so the rail comes out of \
         the transcript's edge rather than being parted from it"
    );
    assert_eq!(
        rows,
        band - 1,
        "and down every row of the band it shares with the transcript, up to the row the \
         composer's band starts at"
    );

    pty.type_bytes(&[0x11]);
    assert_eq!(session.wait().code(), Some(0), "{:?}", drawn.text());
}

/// The rail's rows as the failing runs read them: the session's temp beside the session's row, and
/// the turns the launch had counted.
fn rail_picture() -> BTreeMap<u16, String> {
    BTreeMap::from([
        (1, String::from("temp  41b0c336/tmp")),
        (3, String::from("turns  2 / 250")),
    ])
}

#[test]
fn a_status_document_told_after_a_rail_row_puts_no_word_of_it_in_that_row() {
    let document = "\u{1b}]7501;{\"phase\":\"done\",\"message\":\"12 files read\"}\u{1b}\\";
    let drawn = format!("\u{1b}[1;1Htemp  41b0c336/tmp\u{1b}[3;1Hturns  2 / 250{document}");
    assert_eq!(
        picture(&drawn),
        rail_picture(),
        "the frame that painted the turns and told a status document after them reads as the rows \
         it painted: the document is the terminal's and tails no row of the rail {drawn:?}"
    );
}

#[test]
fn every_document_a_frame_hands_its_terminal_stands_in_no_cell_of_the_picture() {
    let rows = rail_picture();
    for (shape, document) in [
        ("a title", "\u{1b}]2;forge \u{2014} done\u{1b}\\"),
        ("a progress mark", "\u{1b}]9;4;3\u{1b}\\"),
        ("a mark taken away", "\u{1b}]9;4;0\u{1b}\\"),
        ("a notification", "\u{1b}]9;four files moved\u{7}"),
        ("a clipboard copy", "\u{1b}]52;c;Zm9yZ2U=\u{1b}\\"),
        (
            "a link opened",
            "\u{1b}]8;;https://example.com/forge\u{1b}\\",
        ),
        ("a link closed", "\u{1b}]8;;\u{1b}\\"),
        ("a sized heading", "\u{1b}]66;s=2;The fence rule\u{1b}\\"),
        ("a placement", "\u{1b}_Ga=T,f=100,i=333,q=2;QUJD\u{1b}\\"),
    ] {
        let drawn = format!("\u{1b}[1;1Htemp  41b0c336/tmp{document}\u{1b}[3;1Hturns  2 / 250");
        assert_eq!(
            picture(&drawn),
            rows,
            "a frame hands its terminal {shape} between two painted rows, and the words of it are \
             the terminal's: no one of them stands in a rail {drawn:?}"
        );
    }
}

#[test]
fn a_document_a_frame_was_read_in_the_middle_of_paints_no_cell_of_its_words() {
    let drawn = "\u{1b}[1;1Htemp  41b0c336/tmp\u{1b}[3;1Hturns  2 / 250\u{1b}]7501;{\"phase\":\"do";
    assert_eq!(
        picture(drawn),
        rail_picture(),
        "a frame is read as fast as it is drawn and a document can arrive in halves: a terminal \
         holds one it is still reading and paints none of its words {drawn:?}"
    );
}
