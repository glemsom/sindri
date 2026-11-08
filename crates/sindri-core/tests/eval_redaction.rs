//! The redaction walk of ADR-0011: every `Entry` variant is written with a known
//! secret planted in every field that can hold one, the transcript is read back,
//! and no field of any variant carries the secret out (§13, §16).
//!
//! The walk destructures each variant without a `..`, so a field added later stops
//! this test from building rather than riding out unwatched: the promise is that
//! every field a record writes is covered, and a walk that quietly skipped the new
//! field would be the hole it exists to close.
//!
//! The lines are held to a golden as well as to the secret's absence: the mask is
//! the only thing a reader may find where a secret was, and a line that stopped
//! writing a field is a fact a diff should name. `SINDRI_BLESS=1` rewrites the
//! golden, and it is the only way one moves.
//!
//! An eval is judged by `make evals` and never by `make test`, which is why the
//! test here is ignored: the stage that owns a failure is the stage that prints it.

use std::time::Duration;

use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::records::{Entry, Facts, Record, read_transcript};
use sindri_core::redact::SecretSet;
use sindri_core::run::{Acted, Call, Exit, Limits};
use sindri_core::session::Session;
use sindri_core::wire::Wire;

/// The literal planted in every field. Its shape says what it is, so a reader of a
/// failure knows the string is the harness's and not a line the code invented.
const SECRET: &str = "hunter2-eval";

/// What a secret is written as, spelled out here rather than imported: a test that
/// read the constant could not tell a mask from a mask that never changed.
const MASK: &str = "«redacted»";

/// Where the accepted lines live: beside the test that compares them.
const GOLDENS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/evals/redaction/entries.jsonl"
);

/// The variable that rewrites the golden instead of comparing it.
const BLESS: &str = "SINDRI_BLESS";

/// A workspace, a state home and the dials the record is opened with, all the
/// test's own: no real `HOME` or `XDG_STATE_HOME` is read or written.
struct Ground {
    /// Held so the directories outlive the paths that name them.
    _root: tempfile::TempDir,
    /// The one directory the run stands in.
    workspace: std::path::PathBuf,
    /// The state home the session directory is made under.
    state: std::path::PathBuf,
    /// The dials the run obeys.
    limits: Limits,
    /// Every literal the record must never write down.
    secrets: SecretSet,
}

impl Ground {
    /// A ground with an empty workspace, an empty state home, and `SECRET` among
    /// the literals the record must not carry.
    fn new() -> Ground {
        let root = tempfile::tempdir().expect("a temp ground");
        let workspace = root.path().join("workspace");
        let state = root.path().join("state");
        for path in [&workspace, &state] {
            std::fs::create_dir_all(path).expect("a directory");
        }
        Ground {
            _root: root,
            workspace,
            state,
            limits: Limits::default(),
            secrets: SecretSet::of([SECRET]),
        }
    }

    /// A session directory of this ground's state home.
    fn session(&self) -> Session {
        Session::create(&self.state).expect("a session")
    }

    /// A record opened on `session`, held to the unsafe boundary because nothing a
    /// transcript says is a fact about a sandbox.
    fn record(&self, session: &Session) -> Record {
        let boundary =
            Boundary::from_config(&Config::default(), &self.workspace, Enforcement::Unsafe)
                .expect("the boundary holds");
        Record::open(
            session,
            Facts {
                wire: Wire::Anthropic,
                model: "claude-sonnet-4",
                route: None,
                thinking: false,
                boundary: &boundary,
                limits: &self.limits,
                workspace: &self.workspace,
                secrets: &self.secrets,
            },
        )
        .expect("a record")
    }
}

/// Every field of `entry`, named as the transcript names it: the destructuring
/// leaves no `..`, so a variant that gains a field stops this from building.
fn walk(entry: &Entry) -> Vec<(&'static str, String)> {
    match entry {
        Entry::Turn { turn } => vec![("turn", turn.to_string())],
        Entry::Call {
            turn,
            name,
            arguments,
        } => vec![
            ("turn", turn.to_string()),
            ("name", name.to_string()),
            ("arguments", arguments.to_string()),
        ],
        Entry::Result {
            turn,
            name,
            text,
            bytes,
            elapsed_ms,
            exit,
        } => vec![
            ("turn", turn.to_string()),
            ("name", name.to_string()),
            ("text", text.to_string()),
            ("bytes", bytes.to_string()),
            ("elapsed_ms", elapsed_ms.to_string()),
            ("exit", format!("{exit:?}")),
        ],
        Entry::Reasoning { turn, text } => {
            vec![("turn", turn.to_string()), ("text", text.to_string())]
        }
        Entry::Assistant { turn, text } => {
            vec![("turn", turn.to_string()), ("text", text.to_string())]
        }
    }
}

/// Whether a field is one the secret was planted in, which is every field that
/// carries words the run was handed and none of the numbers or the verdict.
fn planted(field: &str) -> bool {
    matches!(field, "name" | "arguments" | "text")
}

/// The transcript the walk reads, written through the record a run writes through so
/// the redaction happens where the production path applies it and never in this test.
fn written(ground: &Ground) -> Vec<String> {
    let session = ground.session();
    let mut record = ground.record(&session);
    let call = Call::new(SECRET, format!(r#"{{"command": "echo {SECRET}"}}"#));

    record.turn_started(1).expect("a turn start is recorded");
    record.called(1, &call).expect("a call is recorded");
    record
        .resulted(
            1,
            &call,
            &Acted {
                text: format!("{SECRET}\n"),
                bytes: 7,
                elapsed: Duration::from_millis(3),
                exit: Exit::Code(0),
            },
        )
        .expect("a result is recorded");
    record
        .reasoned(1, &format!("the key is {SECRET}, so read the file"))
        .expect("a thought is recorded");
    record
        .answered(1, &format!("the key is {SECRET}"))
        .expect("an answer is recorded");

    assert_eq!(
        read_transcript(session.directory())
            .expect("the transcript reads")
            .skipped(),
        0,
        "every line this walk wrote is a line this build knows"
    );
    std::fs::read_to_string(session.directory().join("transcript.jsonl"))
        .expect("the transcript")
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Holds the walk's lines to the golden, or rewrites it when `bless` says so.
fn hold(lines: &[String]) {
    let wrote = format!("{}\n", lines.join("\n"));
    if std::env::var_os(BLESS).is_some() {
        std::fs::write(GOLDENS, wrote)
            .unwrap_or_else(|error| panic!("{GOLDENS} is written: {error}"));
        return;
    }
    let accepted = std::fs::read_to_string(GOLDENS).unwrap_or_else(|error| {
        panic!("the golden {GOLDENS} is readable: {error}; run `SINDRI_BLESS=1 make evals`")
    });
    assert!(
        accepted == wrote,
        "the transcript lines: {}\nrun `SINDRI_BLESS=1 make evals` to move the golden deliberately",
        named_diff(&accepted, &wrote)
    );
}

/// The first line two transcripts disagree on, with its number and both sides.
fn named_diff(accepted: &str, wrote: &str) -> String {
    for (at, (left, right)) in accepted
        .lines()
        .zip(wrote.lines())
        .enumerate()
        .map(|(at, pair)| (at + 1, pair))
    {
        if left != right {
            return format!("line {at} differs:\n  accepted: {left}\n  written:  {right}");
        }
    }
    format!(
        "the accepted transcript has {} lines and the written one has {}",
        accepted.lines().count(),
        wrote.lines().count()
    )
}

#[test]
#[ignore = "an eval is judged by `make evals`, not by `make test`"]
fn no_field_of_any_entry_lets_a_secret_out() {
    let ground = Ground::new();
    let lines = written(&ground);
    let mut leaked = Vec::new();
    let mut unmasked = Vec::new();

    for (at, line) in lines.iter().enumerate() {
        assert!(
            !line.contains(SECRET),
            "line {} of {GOLDENS} carries the secret whole: {line}",
            at + 1
        );
        let entry: Entry = serde_json::from_str(line).unwrap_or_else(|error| {
            panic!("line {} reads back as an entry: {error}\n{line}", at + 1)
        });
        for (field, value) in walk(&entry) {
            if value.contains(SECRET) {
                leaked.push(format!("line {} field `{field}`: {value}", at + 1));
            }
            if planted(field) && !value.contains(MASK) {
                unmasked.push(format!(
                    "line {} field `{field}`: {value} — the secret was planted here",
                    at + 1
                ));
            }
        }
    }

    assert!(
        leaked.is_empty(),
        "no field of any Entry variant lets a secret out (ADR-0011):\n{}",
        leaked.join("\n")
    );
    assert!(
        unmasked.is_empty(),
        "every field a secret was planted in writes the mask and nothing else:\n{}",
        unmasked.join("\n")
    );
    hold(&lines);
    println!(
        "\ncheck: eval redaction-walk ... ok ({} lines)",
        lines.len()
    );
}
