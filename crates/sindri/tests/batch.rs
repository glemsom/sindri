//! What batch mode hands a script, measured where a script measures it: the built binary is spawned and its
//! stdout, stderr and exit code read back as bytes, against a loopback socket serving a recorded stream, so no
//! test needs a network or a key, and every child gets `env_clear` and this machine's `PATH`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use sindri_core::bash::BWRAP;
use tempfile::TempDir;

const ANTHROPIC: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_stream.sse");

const COPILOT_CHAT: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/copilot_chat_stream.sse");

const COPILOT_RESPONSES: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/openai_responses_stream.sse");

const COPILOT_ANSWER: &str = "Hello from Copilot";

/// A signed-in store's Copilot session: never an api key, and never printed anywhere a run writes.
const SESSION: &str = "tid=recorded_copilot_session";

const BAD_REQUEST: &str = r#"{"error":{"message":"unknown model","type":"invalid_request_error"}}"#;

const ANSWER: &str = "Hello world";

#[derive(Clone, Copy)]
struct Reply {
    status: u16,
    content_type: &'static str,
    body: &'static str,
}

impl Reply {
    fn stream(body: &'static str) -> Reply {
        Reply {
            status: 200,
            content_type: "text/event-stream",
            body,
        }
    }

    fn refusal(status: u16, body: &'static str) -> Reply {
        Reply {
            status,
            content_type: "application/json",
            body,
        }
    }
}

/// A loopback wire: it answers in order, keeps every request, and can hold a reply until the test releases it.
struct Wire {
    port: u16,
    seen: Arc<Mutex<Vec<String>>>,
    arrived: Receiver<()>,
    _replies: Vec<Reply>,
}

impl Wire {
    fn answering(replies: Vec<Reply>) -> Wire {
        Wire::start(replies, None)
    }

    /// A wire that answers only once it is released, one release per request, so a test can act mid-request.
    fn holding(replies: Vec<Reply>) -> (Wire, Sender<()>) {
        let (release, gate) = channel();
        (Wire::start(replies, Some(gate)), release)
    }

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

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn wait_for_request(&self) {
        self.arrived
            .recv_timeout(Duration::from_secs(10))
            .expect("a request should reach the wire");
    }

    fn requests(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

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

fn split(request: &[u8]) -> Option<(String, usize)> {
    let end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&request[..end]).into_owned();
    Some((head, request.len() - (end + 4)))
}

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

struct Ground {
    /// Held so the directories outlive the paths that name them.
    _root: TempDir,
    workspace: PathBuf,
    state: PathBuf,
    config: PathBuf,
}

impl Ground {
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

    /// A child pointed at this ground and at nothing else, with none of this process's environment but
    /// `PATH`: without it a child finds only `/bin:/usr/bin`, so the bubblewrap a devbox or Nix install puts
    /// on `PATH` would be invisible and every test here would read the card instead of the contract.
    fn launch(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sindri"));
        command
            .env_clear()
            .env("PATH", path())
            .env("XDG_CONFIG_HOME", &self.config)
            .env("XDG_STATE_HOME", &self.state)
            .env("HOME", &self.config)
            .current_dir(&self.workspace);
        if sandbox_refusal().is_some() {
            command.arg("--yolo-unsafe");
        }
        command
    }

    fn command(&self) -> Command {
        let mut command = self.launch();
        command
            .env("SINDRI_PROVIDER", "anthropic")
            .env("SINDRI_MODEL", "claude-sonnet-4-6")
            .env("ANTHROPIC_API_KEY", "sk-ant-the-recorded-one");
        command
    }

    fn enforcement(&self) -> &'static str {
        match sandbox_refusal() {
            None => "bubblewrap",
            Some(_) => "unsafe",
        }
    }

    fn asking(&self, wire: &Wire, prompt: &str) -> Command {
        let mut command = self.command();
        command
            .env("SINDRI_BASE_URL", wire.base_url())
            .args(["-b", prompt]);
        command
    }

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

    fn signed_in(&self) -> &Ground {
        let directory = self.config.join("sindri").join("auth");
        std::fs::create_dir_all(&directory).expect("the auth directory");
        let record = format!(
            r#"{{"token":"{SESSION}","expires_at":4000000000,"endpoints":{{"api":"https://api.githubcopilot.com"}}}}"#
        );
        let path = directory.join("copilot.json");
        std::fs::write(&path, record).expect("the sign-in record");
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600))
            .expect("an owner-only record");
        self
    }

    fn copilot(&self, wire: &Wire, prompt: &str) -> Command {
        let mut command = self.launch();
        command
            .env("SINDRI_PROVIDER", "copilot")
            .env("SINDRI_MODEL", "gpt-4o")
            .env("SINDRI_BASE_URL", wire.base_url())
            .args(["-b", prompt]);
        command
    }

    fn session(&self) -> PathBuf {
        let sessions = self.sessions();
        assert_eq!(sessions.len(), 1, "every run records exactly one session");
        sessions[0].clone()
    }
}

fn path() -> String {
    std::env::var("PATH").expect("this machine has a PATH")
}

/// Why this machine cannot confine a command, or `None` where it can: asked once, of the machine rather than of
/// the tree. A machine that answers no is handed `--yolo-unsafe`, and the answer goes to stderr's own
/// descriptor, because a passing test's output is captured and a green run must still name the tier it missed.
fn sandbox_refusal() -> Option<&'static str> {
    static REFUSAL: OnceLock<Option<String>> = OnceLock::new();
    REFUSAL
        .get_or_init(|| {
            let refusal = probe();
            if let Some(problem) = &refusal {
                let line = format!(
                    "skip batch: this machine cannot confine a command ({problem})\n  \
                     every run here boots with --yolo-unsafe, so batch mode's contract is \
                     proved unconfined and the sandbox tier is what this run leaves uncovered\n"
                );
                let _ = std::io::stderr().write_all(line.as_bytes());
            }
            refusal
        })
        .as_deref()
}

/// The three checks `scripts/sandbox-probe.sh` makes, in its order, keeping the network in the argv as boot's own
/// invocation does: a runner that refuses a network namespace alone is not a machine that refuses the sandbox
/// a run would be made in.
fn probe() -> Option<String> {
    let version = match Command::new(BWRAP).arg("--version").output() {
        Ok(version) => version,
        Err(error) => return Some(format!("bwrap not on PATH: {error}")),
    };
    if !version.status.success() {
        return Some(format!("bwrap --version: {}", said(&version)));
    }
    let made = Command::new(BWRAP)
        .args([
            "--unshare-all",
            "--share-net",
            "--die-with-parent",
            "--ro-bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--",
            "true",
        ])
        .output();
    match made {
        Err(error) => Some(format!("bwrap could not be run: {error}")),
        Ok(made) if !made.status.success() => Some(format!(
            "bwrap cannot create an unprivileged sandbox: {}",
            said(&made)
        )),
        Ok(_) => None,
    }
}

fn said(output: &std::process::Output) -> String {
    let stderr = text(&output.stderr);
    let stderr = stderr.trim();
    match stderr.is_empty() {
        true => output.status.to_string(),
        false => stderr.to_owned(),
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn lines(output: &str) -> Vec<&str> {
    output.lines().filter(|line| !line.is_empty()).collect()
}

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
fn a_run_records_the_thinking_its_ask_sends() {
    for (launched, thinks) in [(Some("true"), true), (Some("false"), false), (None, false)] {
        let ground = Ground::new();
        let wire = Wire::answering(vec![Reply::stream(ANTHROPIC)]);
        let mut command = ground.asking(&wire, "say hello");
        if let Some(value) = launched {
            command.env("SINDRI_THINKING", value);
        }

        let output = command.output().expect("the binary should run");
        assert_eq!(
            output.status.code(),
            Some(0),
            "SINDRI_THINKING={launched:?}: {}",
            text(&output.stderr)
        );

        let written = std::fs::read_to_string(ground.session().join("session.json"))
            .expect("the session document");
        let document: serde_json::Value = serde_json::from_str(&written).expect("a JSON document");
        assert_eq!(
            document["thinking"],
            serde_json::json!(thinks),
            "a run launched with {launched:?} says whether it thinks rather than leaving \
             the field out: {written}"
        );
        assert_eq!(
            wire.requests()[0].contains(r#""thinking""#),
            thinks,
            "and the value recorded is the value its ask sends: {:?}",
            wire.requests()[0]
        );
    }
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
        "and an object a launch cannot claim is never written (§3): {}",
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
    assert_eq!(
        value["enforcement"],
        ground.enforcement(),
        "the object names the posture the run took, and this run took the one it was \
         handed: {line}"
    );
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
    // The reply is let go after the signal, so the run sees the user's word before it can settle as an
    // answer.
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

    // The record is written as the run goes, so a run the user's word stopped is a session that still reads.
    let document = std::fs::read_to_string(ground.session().join("session.json"))
        .expect("the session document");
    assert!(
        document.contains(r#""stopped": "cancelled""#),
        "the record's stop reason is the user's word: {document}"
    );

    let directory = ground.session();
    let guid = directory
        .file_name()
        .expect("a session directory is named by its GUID")
        .to_string_lossy()
        .into_owned();
    let read_back = Command::new(env!("CARGO_BIN_EXE_sindri"))
        .env_clear()
        .env("XDG_STATE_HOME", &ground.state)
        .env("HOME", &ground.config)
        .args(["sessions", "show", &guid])
        .output()
        .expect("the binary should run");
    assert_eq!(
        read_back.status.code(),
        Some(0),
        "the cancelled session reads back: {}",
        text(&read_back.stderr)
    );
    assert!(
        text(&read_back.stdout).contains("cancelled"),
        "and says the user's word stopped it: {}",
        text(&read_back.stdout)
    );
}

fn signal(child: &Child) {
    let status = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("`kill` should run");
    assert!(status.success(), "the signal should be delivered");
}

#[test]
fn a_signed_in_copilot_answers_a_prompt_on_the_chat_route() {
    let ground = Ground::new();
    ground.signed_in();
    let wire = Wire::answering(vec![Reply::stream(COPILOT_CHAT)]);

    let output = ground
        .copilot(&wire, "say hello")
        .output()
        .expect("the binary should run");

    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), format!("{COPILOT_ANSWER}\n"));

    let request = &wire.requests()[0];
    assert!(
        request.starts_with("POST /chat/completions "),
        "a name that asks for no other route is served by Chat Completions: {request}"
    );
    let head = request.to_lowercase();
    assert!(
        head.contains(&format!("authorization: bearer {}", SESSION.to_lowercase())),
        "the session the store holds is what the wire is asked with: {request}"
    );
    for name in ["editor-version", "copilot-integration-id", "user-agent"] {
        assert!(
            head.contains(name),
            "the editor identity rig stamps rides along: {request}"
        );
    }

    let document = std::fs::read_to_string(ground.session().join("session.json"))
        .expect("the session document");
    assert!(
        document.contains(r#""route": "chat""#),
        "the route the run answered on is kept, so a mystery is debuggable: {document}"
    );
    assert!(
        document.contains(r#""provider": "copilot""#),
        "and the wire it answered on: {document}"
    );
}

#[test]
fn a_named_route_takes_a_signed_in_copilot_to_the_other_endpoint() {
    let ground = Ground::new();
    ground.signed_in();
    let wire = Wire::answering(vec![Reply::stream(COPILOT_RESPONSES)]);

    let output = ground
        .copilot(&wire, "say hello")
        .args(["--route", "responses"])
        .output()
        .expect("the binary should run");

    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let request = &wire.requests()[0];
    assert!(
        request.starts_with("POST /responses "),
        "the route a person named is the route the run takes: {request}"
    );
    let document = std::fs::read_to_string(ground.session().join("session.json"))
        .expect("the session document");
    assert!(
        document.contains(r#""route": "responses""#),
        "and the record says which endpoint answered it: {document}"
    );
}

#[test]
fn a_copilot_session_reaches_no_window_a_run_writes() {
    let ground = Ground::new();
    ground.signed_in();
    let wire = Wire::answering(vec![Reply::stream(COPILOT_CHAT)]);

    let output = ground
        .copilot(&wire, "say hello")
        .args(["--format", "json"])
        .output()
        .expect("the binary should run");

    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    let (report, line) = object(&stdout);
    assert_eq!(
        report["usage"]["input"],
        serde_json::json!(12),
        "the usage the wire reported is in the object: {line}"
    );

    let session = ground.session();
    let record = std::fs::read_to_string(session.join("session.json")).expect("the document");
    let transcript =
        std::fs::read_to_string(session.join("transcript.jsonl")).expect("the transcript");
    for (window, written) in [
        ("stdout", line.to_owned()),
        ("stderr", text(&output.stderr)),
        ("session.json", record),
        ("transcript.jsonl", transcript),
    ] {
        assert!(
            !written.contains(SESSION),
            "a copilot session is a secret and never rides {window}: {written}"
        );
    }
}

#[test]
fn a_copilot_run_with_no_sign_in_names_the_command_that_signs_in() {
    let ground = Ground::new();
    let wire = Wire::answering(vec![Reply::stream(COPILOT_CHAT)]);

    let output = ground
        .copilot(&wire, "say hello")
        .output()
        .expect("the binary should run");

    assert_eq!(output.status.code(), Some(1), "a provider error (§15)");
    assert!(output.stdout.is_empty(), "{}", text(&output.stdout));
    assert!(
        text(&output.stderr).contains("sindri auth login copilot"),
        "the failure names the one command that fixes it: {}",
        text(&output.stderr)
    );
    assert!(
        wire.requests().is_empty(),
        "and nothing was sent without a session: {:?}",
        wire.requests()
    );
}
