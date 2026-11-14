//! The redaction walk: every `Entry` variant is written with a known secret in every field that can
//! hold one, and the transcript is read back against a golden only `SINDRI_BLESS=1` moves. The walk
//! destructures each variant without a `..`, so a field added later stops it building.

use std::time::Duration;

use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::records::{Entry, Facts, Record, read_transcript};
use sindri_core::redact::SecretSet;
use sindri_core::run::{Acted, Call, Exit, Limits};
use sindri_core::session::Session;
use sindri_core::wire::Wire;

const SECRET: &str = "hunter2-eval";

/// What a secret is written as, spelled out rather than imported: a test that read the constant
/// could not tell a mask from a mask that never changed.
const MASK: &str = "«redacted»";

const GOLDENS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/evals/redaction/entries.jsonl"
);

const BLESS: &str = "SINDRI_BLESS";

struct Ground {
    _root: tempfile::TempDir,
    workspace: std::path::PathBuf,
    state: std::path::PathBuf,
    limits: Limits,
    secrets: SecretSet,
}

impl Ground {
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

    fn session(&self) -> Session {
        Session::create(&self.state).expect("a session")
    }

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
        Entry::Prompt { text } => vec![("text", text.to_string())],
        Entry::Stop { stop } => vec![(
            "stop",
            stop.detail().unwrap_or_else(|| stop.word().to_owned()),
        )],
    }
}

fn planted(field: &str) -> bool {
    matches!(field, "name" | "arguments" | "text")
}

/// The transcript the walk reads, written through the record a run writes through, so redaction
/// happens on the production path and never in this test.
fn written(ground: &Ground) -> Vec<String> {
    let session = ground.session();
    let mut record = ground.record(&session);
    let call = Call::new(SECRET, format!(r#"{{"command": "echo {SECRET}"}}"#));

    record
        .asked(&format!("the key is {SECRET}, so triage this"))
        .expect("a prompt is recorded");
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
