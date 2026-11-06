//! What batch mode hands a script, measured where a script measures it: the built
//! binary is spawned and its stdout, its stderr and its exit code are read back,
//! so the contract is proved as bytes rather than as a function's return value.
//!
//! The wire is a loopback socket serving the bytes a fixture recorded, so the real
//! client decodes a real event stream and no test needs a network or a key. The
//! homes are temp directories of the test's own and every child gets `env_clear`,
//! so nothing of the test process's environment — least of all a real key — can
//! reach the binary.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tempfile::TempDir;

/// An Anthropic Messages reply, as the wire streamed it: two text fragments that
/// make one answer, and a usage report of eleven tokens in and seven out.
const ANTHROPIC: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_stream.sse");

/// What a wire says when the request itself was wrong, so no retry can help.
const BAD_REQUEST: &str = r#"{"error":{"message":"unknown model","type":"invalid_request_error"}}"#;

/// The answer the recorded stream carries, named once so an assertion is about
/// the answer rather than about a slice of a fixture.
const ANSWER: &str = "Hello world";

/// One reply a [`Wire`] hands back: the status, the content type and the body.
#[derive(Clone, Copy)]
struct Reply {
    /// The HTTP status, as a wire writes it.
    status: u16,
    /// The content type, which is what says whether a body is an event stream.
    content_type: &'static str,
    /// The body, byte for byte.
    body: &'static str,
}

impl Reply {
    /// An event stream, as a reply was recorded mid-stream.
    fn stream(body: &'static str) -> Reply {
        Reply {
            status: 200,
            content_type: "text/event-stream",
            body,
        }
    }

    /// A refusal, as the wire worded it.
    fn refusal(status: u16, body: &'static str) -> Reply {
        Reply {
            status,
            content_type: "application/json",
            body,
        }
    }
}

/// A wire on a loopback socket: it answers in order, keeps every request for a
/// later assertion, and can hold a reply until the test lets it go, which is how
/// a cancellation is made to arrive while a request is in flight.
struct Wire {
    /// The port the socket listens on.
    port: u16,
    /// Every request, head and body, in the order it arrived.
    seen: Arc<Mutex<Vec<String>>>,
    /// One message per request served, so a test knows the child is waiting.
    arrived: Receiver<()>,
    /// The replies it hands out, in order; the last one repeats.
    _replies: Vec<Reply>,
}

impl Wire {
    /// A wire answering `replies`, and holding none of them.
    fn answering(replies: Vec<Reply>) -> Wire {
        Wire::start(replies, None)
    }

    /// A wire that answers `replies` only once it is released, one release per
    /// request, so a test can act while a request is in flight. The sender comes
    /// back with it.
    fn holding(replies: Vec<Reply>) -> (Wire, Sender<()>) {
        let (release, gate) = channel();
        (Wire::start(replies, Some(gate)), release)
    }

    /// The socket, the thread that serves it and the channel that says a request
    /// has arrived.
    fn start(replies: Vec<Reply>, gate: Option<Receiver<()>>) -> Wire {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("a bound address").port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (arrived, arrivals) = channel();
        let log = Arc::clone(&seen);
        let keeping = replies.clone();
        std::thread::spawn(move || {
            for (index, stream) in listener.incoming().enumerate() {
                let Ok(mut stream) = stream else { break };
                let request = read_request(&mut stream);
                log.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(request);
                let _ = arrived.send(());
                if let Some(gate) = &gate {
                    let _ = gate.recv_timeout(Duration::from_secs(10));
                }
                let reply = replies[index.min(replies.len().saturating_sub(1))];
                let _ = write_reply(&mut stream, reply);
            }
        });
        Wire {
            port,
            seen,
            arrived: arrivals,
            _replies: keeping,
        }
    }

    /// The base URL a run is pointed at, so no request leaves the machine.
    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Waits until the child has a request in flight.
    fn wait_for_request(&self) {
        self.arrived
            .recv_timeout(Duration::from_secs(10))
            .expect("a request should reach the wire");
    }

    /// Every request this wire was sent, in order.
    fn requests(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// The whole of an HTTP request: the head, then the body its `content-length`
/// promised. The head ends at the first blank line, which is the only place the
/// two parts can be told apart.
fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    while split(&request).is_none() {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return String::from_utf8_lossy(&request).into_owned(),
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
    String::from_utf8_lossy(&request).into_owned()
}

/// Where the head ends and its body begins, once the blank line has arrived.
fn split(request: &[u8]) -> Option<(String, usize)> {
    let end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&request[..end]).into_owned();
    Some((head, request.len() - (end + 4)))
}

/// One reply, as the socket writes it: its length is given so the client does not
/// wait for a close to find the end of it.
fn write_reply(stream: &mut TcpStream, reply: Reply) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} OK\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        reply.status,
        reply.content_type,
        reply.body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(reply.body.as_bytes())?;
    stream.flush()
}

/// A workspace, a config home and a state home, all of the test's own.
struct Ground {
    /// Held so the directories outlive the paths that name them.
    _root: TempDir,
    /// The one directory the run stands in.
    workspace: PathBuf,
    /// The state home the session directory is made under.
    state: PathBuf,
    /// The config home, which holds no file unless a test writes one.
    config: PathBuf,
}

impl Ground {
    /// A ground with an empty workspace and empty homes.
    fn new() -> Ground {
        let root = tempfile::tempdir().expect("a temp ground");
        let workspace = root.path().join("workspace");
        let state = root.path().join("state");
        let config = root.path().join("config");
        for path in [&workspace, &state, &config] {
            std::fs::create_dir_all(path).expect("a directory");
        }
        Ground {
            _root: root,
            workspace,
            state,
            config,
        }
    }

    /// A child pointed at this ground and at nothing else: no inherited
    /// environment at all, and the workspace as its working directory.
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

    /// A child pointed at `wire`, with the prompt already on the command line.
    fn asking(&self, wire: &Wire, prompt: &str) -> Command {
        let mut command = self.command();
        command
            .env("SINDRI_BASE_URL", wire.base_url())
            .args(["-b", prompt]);
        command
    }

    /// Every session directory this ground's state home holds.
    fn sessions(&self) -> Vec<PathBuf> {
        let root = self.state.join("sindri").join("sessions");
        let mut found: Vec<PathBuf> = std::fs::read_dir(root)
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

    /// The one session this ground holds, which is the one a test's run wrote.
    fn session(&self) -> PathBuf {
        let sessions = self.sessions();
        assert_eq!(sessions.len(), 1, "every run records exactly one session");
        sessions[0].clone()
    }
}

/// A stream as a reader sees it.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The lines of a stream, without the empty one a trailing newline makes.
fn lines(output: &str) -> Vec<&str> {
    output.lines().filter(|line| !line.is_empty()).collect()
}

/// The JSON object a `--format json` run wrote, and its bytes.
fn object(stdout: &str) -> (serde_json::Value, &str) {
    let line = stdout
        .strip_suffix('\n')
        .expect("the object ends with one newline");
    assert!(!line.contains('\n'), "one object on one line: {stdout}");
    let value = serde_json::from_str(line).expect("the line is one JSON object");
    (value, line)
}

#[test]
fn a_prompt_is_answered_on_stdout_with_exactly_one_newline() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);

    let output = ground
        .asking(&wire, "triage this")
        .output()
        .expect("the binary should run");

    assert_eq!(output.status.code(), Some(0), "an answer (§15)");
    assert_eq!(
        text(&output.stdout),
        format!("{ANSWER}\n"),
        "the answer and exactly one newline, and nothing else"
    );
    assert!(
        !text(&output.stderr).contains(ANSWER),
        "chatter goes to stderr and the answer never does: {}",
        text(&output.stderr)
    );
    assert!(
        ground.session().join("session.json").exists(),
        "a run records the session it was (§13)"
    );
}

#[test]
fn a_run_that_answered_records_its_turn_and_why_it_stopped() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);

    ground
        .asking(&wire, "triage this")
        .output()
        .expect("the binary should run");

    let session = std::fs::read_to_string(ground.session().join("session.json"))
        .expect("the session document");
    let document: serde_json::Value = serde_json::from_str(&session).expect("a JSON document");
    assert_eq!(document["stopped"], "final", "{session}");
    assert_eq!(document["turns"], 1, "{session}");
    assert_eq!(
        document["usage"]["input"], 11,
        "the tokens the wire reported are the tokens the record keeps: {session}"
    );

    let transcript =
        std::fs::read_to_string(ground.session().join("transcript.jsonl")).expect("the transcript");
    let kinds: Vec<String> = transcript
        .lines()
        .map(|line| {
            let entry: serde_json::Value = serde_json::from_str(line).expect("an entry");
            entry["kind"].as_str().expect("a kind").to_owned()
        })
        .collect();
    assert_eq!(
        kinds,
        ["turn", "assistant"],
        "the turn, then the answer: {transcript}"
    );
}

#[test]
fn a_piped_stdin_is_context_that_follows_the_prompt() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);

    let mut child = ground
        .asking(&wire, "triage this")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary should start");
    child
        .stdin
        .take()
        .expect("a piped stdin")
        .write_all(b"three things went wrong\n")
        .expect("the piped context");
    let output = child.wait_with_output().expect("the binary should finish");

    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let body = &wire.requests()[0];
    let prompt = body
        .find("triage this")
        .unwrap_or_else(|| panic!("the prompt is what was asked: {body}"));
    let context = body
        .find("three things went wrong")
        .unwrap_or_else(|| panic!("the pipe is context, and it was read: {body}"));
    assert!(
        prompt < context,
        "the piped context is appended after the prompt, not before it: {body}"
    );
}

#[test]
fn with_no_prompt_the_pipe_is_the_prompt() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);

    let mut child = ground
        .command()
        .env("SINDRI_BASE_URL", wire.base_url())
        .arg("-b")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary should start");
    child
        .stdin
        .take()
        .expect("a piped stdin")
        .write_all(b"what failed overnight\n")
        .expect("the piped prompt");
    let output = child.wait_with_output().expect("the binary should finish");

    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), format!("{ANSWER}\n"));
    assert!(
        wire.requests()[0].contains("what failed overnight"),
        "the pipe is the prompt when no prompt was given: {}",
        wire.requests()[0]
    );
}

#[test]
fn a_batch_run_with_no_prompt_at_all_is_a_usage_error() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);

    let output = ground
        .command()
        .env("SINDRI_BASE_URL", wire.base_url())
        .arg("-b")
        .stdin(Stdio::null())
        .output()
        .expect("the binary should run");

    assert_eq!(output.status.code(), Some(2), "a usage error (§15)");
    assert!(
        output.stdout.is_empty(),
        "a refused launch writes nothing to stdout: {}",
        text(&output.stdout)
    );
    assert!(
        text(&output.stderr).contains("prompt"),
        "the refusal says what is missing: {}",
        text(&output.stderr)
    );
}

#[test]
fn a_prompt_of_nothing_at_all_is_a_usage_error() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);

    let output = ground
        .command()
        .env("SINDRI_BASE_URL", wire.base_url())
        .args(["-b", "   "])
        .stdin(Stdio::null())
        .output()
        .expect("the binary should run");

    assert_eq!(
        output.status.code(),
        Some(2),
        "whitespace is not a prompt: {}",
        text(&output.stderr)
    );
    assert!(
        wire.requests().is_empty(),
        "nothing was asked of a wire that had nothing to ask"
    );
}

#[test]
fn a_pipe_is_words_to_ask_and_never_a_credential() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);

    let mut child = ground
        .command()
        .env_remove("ANTHROPIC_API_KEY")
        .env("SINDRI_BASE_URL", wire.base_url())
        .args(["-b", "triage this"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary should start");
    child
        .stdin
        .take()
        .expect("a piped stdin")
        .write_all(b"sk-ant-a-prompt-that-looks-like-a-key\n")
        .expect("the piped words");
    let output = child.wait_with_output().expect("the binary should finish");

    let err = text(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "a run with no key is refused rather than fed the pipe: {err}"
    );
    assert!(
        err.contains("api_key"),
        "and the refusal names the key that is missing: {err}"
    );
    assert!(
        !ground.config.join("sindri").join("config.toml").exists(),
        "the pipe was not written down as a credential"
    );
    assert!(output.stdout.is_empty(), "and nothing reached stdout");
}

#[test]
fn a_pipe_without_a_batch_flag_is_refused() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);

    let mut child = ground
        .command()
        .env("SINDRI_BASE_URL", wire.base_url())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary should start");
    child
        .stdin
        .take()
        .expect("a piped stdin")
        .write_all(b"triage this\n")
        .expect("the piped words");
    let output = child.wait_with_output().expect("the binary should finish");

    assert_eq!(output.status.code(), Some(2), "§3: a pipe needs -b");
    assert!(
        output.stdout.is_empty(),
        "and nothing reaches stdout: {}",
        text(&output.stdout)
    );
    assert!(
        wire.requests().is_empty(),
        "nothing was asked of a wire in a launch that was refused"
    );
}

#[test]
fn a_launch_with_no_prompt_accepts_json_and_verbose_and_writes_nothing_to_stdout() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);

    let output = ground
        .command()
        .env("SINDRI_BASE_URL", wire.base_url())
        .args(["--format", "json", "-v"])
        .stdin(Stdio::null())
        .output()
        .expect("the binary should run");

    assert_eq!(
        output.status.code(),
        Some(2),
        "off a terminal a launch with no prompt is a refusal (§3)"
    );
    assert!(
        output.stdout.is_empty(),
        "and an object a launch cannot claim is never written (A5): {}",
        text(&output.stdout)
    );
}

#[test]
fn a_json_run_prints_one_compact_object_and_nothing_else() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);

    let output = ground
        .asking(&wire, "triage this")
        .args(["--format", "json"])
        .output()
        .expect("the binary should run");

    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    let (value, line) = object(&stdout);
    assert_eq!(value["answer"], ANSWER, "{line}");
    assert_eq!(value["stopped"], "final", "{line}");
    assert_eq!(value["turns"], 1, "{line}");
    assert_eq!(value["model"], "claude-sonnet-4-6", "{line}");
    assert_eq!(value["enforcement"], "bubblewrap", "{line}");
    assert_eq!(value["usage"]["input"], 11, "{line}");
    assert_eq!(
        value["session"],
        ground
            .session()
            .file_name()
            .expect("the GUID names the directory")
            .to_string_lossy()
            .as_ref(),
        "the session field is the directory a script joins on: {line}"
    );
    assert!(
        !text(&output.stderr).contains("answer"),
        "the object is stdout's alone: {}",
        text(&output.stderr)
    );
}

#[test]
fn verbose_puts_the_run_on_stderr_and_ends_with_the_session_guid() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);

    let output = ground
        .asking(&wire, "triage this")
        .arg("-v")
        .output()
        .expect("the binary should run");

    let err = text(&output.stderr);
    let written = lines(&err);
    assert!(
        written.len() > 1,
        "the run is on stderr, line by line: {err}"
    );
    let guid = ground
        .session()
        .file_name()
        .expect("the GUID names the directory")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        written.last().copied(),
        Some(guid.as_str()),
        "the last line of -v is the session GUID (§13, §15): {err}"
    );
    assert!(
        err.contains("claude-sonnet-4-6"),
        "and the run it was, named as the config named it: {err}"
    );
    assert_eq!(
        text(&output.stdout),
        format!("{ANSWER}\n"),
        "-v changes stderr and never stdout"
    );
}

#[test]
fn a_wire_that_refuses_exits_one_and_writes_nothing_to_stdout() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::refusal(400, BAD_REQUEST)]);

    let output = ground
        .asking(&wire, "triage this")
        .output()
        .expect("the binary should run");

    assert_eq!(output.status.code(), Some(1), "a model or provider error");
    assert!(
        output.stdout.is_empty(),
        "a failed run writes nothing at all to stdout (§15): {}",
        text(&output.stdout)
    );
    assert!(
        text(&output.stderr).contains("unknown model"),
        "the wire's own words reach stderr: {}",
        text(&output.stderr)
    );
}

#[test]
fn a_json_run_that_failed_reports_why_and_leaves_the_answer_out() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::refusal(400, BAD_REQUEST)]);

    let output = ground
        .asking(&wire, "triage this")
        .args(["--format", "json"])
        .output()
        .expect("the binary should run");

    assert_eq!(output.status.code(), Some(1));
    let stdout = text(&output.stdout);
    let (value, line) = object(&stdout);
    assert_eq!(value["stopped"], "error", "{line}");
    assert!(
        value.get("answer").is_none() && !line.contains("null"),
        "a field the run knows nothing about is left out, never null: {line}"
    );
}

#[test]
fn a_session_that_cannot_be_created_is_a_setup_failure() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);
    let blocked = ground._root.path().join("not-a-directory");
    std::fs::write(&blocked, b"").expect("a file where a directory would go");

    let output = ground
        .asking(&wire, "triage this")
        .env("XDG_STATE_HOME", &blocked)
        .output()
        .expect("the binary should run");

    assert_eq!(
        output.status.code(),
        Some(3),
        "a sandbox or setup failure (§15): {}",
        text(&output.stderr)
    );
    assert!(output.stdout.is_empty(), "and nothing on stdout");
}

#[test]
fn a_turn_cap_that_leaves_no_turn_exits_one_hundred() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);

    let output = ground
        .asking(&wire, "triage this")
        .env("SINDRI_MAX_TURNS", "0")
        .output()
        .expect("the binary should run");

    assert_eq!(output.status.code(), Some(100), "the turn cap (§15)");
    assert!(
        output.stdout.is_empty(),
        "the cap is not an answer, so nothing is printed as one: {}",
        text(&output.stdout)
    );
    assert!(
        wire.requests().is_empty(),
        "and no request was made, because no turn was started"
    );
}

#[test]
fn a_command_budget_of_no_time_exits_one_hundred_and_one() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);

    let output = ground
        .asking(&wire, "triage this")
        .env("SINDRI_COMMAND_BUDGET_MS", "0")
        .output()
        .expect("the binary should run");

    assert_eq!(output.status.code(), Some(101), "a spent budget (§15)");
    assert!(output.stdout.is_empty(), "{}", text(&output.stdout));
}

#[test]
fn a_signal_stops_the_run_with_nothing_on_stdout() {
    let ground = Ground::new();
    let (wire, release) = Wire::holding(vec![Reply::stream(ANTHROPIC)]);

    let child = ground
        .asking(&wire, "triage this")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary should start");

    wire.wait_for_request();
    signal(&child);
    // The reply is let go after the signal, so the run sees the user's word
    // before it can settle as an answer (ADR-0006).
    std::thread::sleep(Duration::from_millis(100));
    let _ = release.send(());
    let output = child.wait_with_output().expect("the binary should finish");

    assert_eq!(
        output.status.code(),
        Some(130),
        "a cancellation with nothing on stdout (§15): {}",
        text(&output.stderr)
    );
    assert!(output.stdout.is_empty(), "{}", text(&output.stdout));
}

/// Sends `INT` to `child`, as a terminal's `^C` would.
fn signal(child: &Child) {
    let status = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("`kill` should run");
    assert!(status.success(), "the signal should be delivered");
}
