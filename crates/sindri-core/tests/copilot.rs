//! Proves the Copilot sign-in with no socket and no GitHub account: the code a
//! person is shown, the wait between polls, the owner-only file the session is
//! kept in, a session refreshed when it expires, and the wording a revoked one
//! comes back with. Every one of them is driven through a transport, a store and
//! a callback this test owns, and the replies are the shapes GitHub was recorded
//! answering with rather than anything invented about the flow.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sindri_core::config::config_path;
use sindri_core::provider::copilot::{self, DeviceCode, SignIn, Store, store_dir};
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Ask, Client, Verdict};
use sindri_core::redact::{MASK, SecretSet};
use sindri_core::wire::Wire;
use tempfile::TempDir;

/// A Copilot session in a Responses-shaped reply, which is the route the dialect
/// takes for a model identifier that asks for it.
const RESPONSES: &str = include_str!("fixtures/providers/openai_responses_stream.sse");

/// The device code GitHub hands out, as the flow was recorded asking for one.
const DEVICE_CODE: &str = r#"{"device_code":"3584cd8a","user_code":"ABCD-1234","verification_uri":"https://github.com/login/device","interval":1,"expires_in":900}"#;

/// GitHub's own answer while the code has not been entered yet.
const PENDING: &str = r#"{"error":"authorization_pending"}"#;

/// The GitHub token the device flow ends with.
const GITHUB_TOKEN: &str = "gho_recorded_github_token";

/// The answer that carries it.
const ACCESS_TOKEN: &str = r#"{"access_token":"gho_recorded_github_token"}"#;

/// The Copilot session the token is exchanged for, good until 2096.
const LIVE_SESSION: &str = "tid=recorded_copilot_session";

/// The exchange, as Copilot Answers it.
const LIVE: &str = r#"{"token":"tid=recorded_copilot_session","expires_at":4000000000,"endpoints":{"api":"https://api.githubcopilot.com"}}"#;

/// The session that same exchange answered with in 1970: a token on file that a
/// run has to refresh before it can ask.
const SPENT: &str = r#"{"token":"tid=spent_copilot_session","expires_at":1,"endpoints":{"api":"https://api.githubcopilot.com"}}"#;

/// The permission bits a path carries, without the file-type bits beside them.
fn mode(path: &Path) -> u32 {
    std::fs::metadata(path)
        .expect("a path to stat")
        .permissions()
        .mode()
        & 0o777
}

/// An environment a test can hold as data, pointed at `home`.
fn environment(home: &TempDir) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();
    vars.insert(
        String::from("XDG_CONFIG_HOME"),
        home.path().display().to_string(),
    );
    vars
}

/// A store in the directory the environment names, whose requests `replay`
/// answers: the seam the front end builds, with the network taken out.
fn store(home: &TempDir, replay: &Replay) -> Store {
    let dir = store_dir(&environment(home)).expect("a config home is named");
    Store::over(replay.clone(), dir)
}

/// A sign-in's three answers as GitHub was recorded giving them, plus whatever
/// comes after them.
fn sign_in_replies(exchange: &'static str) -> Vec<Canned> {
    vec![
        Canned::reply(200, DEVICE_CODE),
        Canned::reply(200, ACCESS_TOKEN),
        Canned::reply(200, exchange),
    ]
}

/// How many times the flow asked GitHub whether the code had been entered.
fn polls(replay: &Replay) -> usize {
    replay
        .requests()
        .iter()
        .filter(|seen| seen.url.ends_with("/login/oauth/access_token"))
        .count()
}

#[test]
fn a_sign_in_shows_the_code_and_keeps_the_session_owner_only() {
    let home = tempfile::tempdir().expect("a temp config home");
    let replay = Replay::new(sign_in_replies(LIVE));
    let signed_in = store(&home, &replay);
    let shown = Arc::new(Mutex::new(Vec::new()));
    let told = Arc::clone(&shown);

    signed_in
        .login(move |code| told.lock().expect("the code log").push(code))
        .expect("the sign-in finishes");

    assert_eq!(
        shown.lock().expect("the code log").as_slice(),
        [DeviceCode {
            verification_uri: String::from("https://github.com/login/device"),
            user_code: String::from("ABCD-1234"),
        }],
        "the person is shown the code and where to type it"
    );

    let kept = signed_in.path();
    assert_eq!(mode(&kept), copilot::MODE, "the session is kept owner-only");
    let apart = config_path(&environment(&home)).expect("a config home is named");
    assert_ne!(
        kept, apart,
        "a sign-in is never the file an api key is read from (§10)"
    );
    assert_eq!(
        kept.parent().map(mode),
        Some(0o700),
        "the directory that holds it is the owner's alone"
    );
    let record = std::fs::read_to_string(&kept).expect("the record was written");
    assert!(
        record.contains(LIVE_SESSION),
        "the session is what a later run reads"
    );
}

#[test]
fn a_sign_in_waits_the_poll_interval_before_asking_github_again() {
    let home = tempfile::tempdir().expect("a temp config home");
    let mut replies = sign_in_replies(LIVE);
    replies.insert(1, Canned::reply(200, PENDING));
    let replay = Replay::new(replies);

    let started = Instant::now();
    store(&home, &replay)
        .login(|_| {})
        .expect("the sign-in finishes once the code is entered");
    let waited = started.elapsed();

    assert_eq!(
        polls(&replay),
        2,
        "an authorization still pending is asked about again"
    );
    assert!(
        waited >= Duration::from_secs(1),
        "the interval GitHub named is waited out before asking: {waited:?}"
    );
}

#[test]
fn an_expired_session_is_refreshed_without_a_second_sign_in() {
    let home = tempfile::tempdir().expect("a temp config home");
    let replay = Replay::new(
        sign_in_replies(SPENT)
            .into_iter()
            .chain([Canned::reply(200, LIVE)])
            .collect::<Vec<_>>(),
    );
    let signed_in = store(&home, &replay);
    signed_in.login(|_| {}).expect("the sign-in finishes");
    let before = replay.requests().len();

    let session = signed_in
        .session()
        .expect("a session that has expired is refreshed on the way out");

    assert_eq!(format!("{session:?}"), MASK, "a session is never printed");
    let asked = &replay.requests()[before..];
    assert_eq!(
        asked.len(),
        1,
        "one exchange, and no device code: {:?}",
        asked.iter().map(|seen| &seen.url).collect::<Vec<_>>()
    );
    assert!(
        asked[0].url.ends_with("/copilot_internal/v2/token"),
        "the refresh is the exchange and nothing else: {}",
        asked[0].url
    );
    assert!(
        std::fs::read_to_string(signed_in.path())
            .expect("the record")
            .contains(LIVE_SESSION),
        "the refreshed session is kept, so the next ask needs no request"
    );
}

#[test]
fn a_revoked_session_asks_for_a_new_sign_in_rather_than_failing_obscurely() {
    let home = tempfile::tempdir().expect("a temp config home");
    let replay = Replay::new(
        sign_in_replies(SPENT)
            .into_iter()
            .chain([Canned::reply(401, r#"{"message":"Bad credentials"}"#)])
            .collect::<Vec<_>>(),
    );
    let signed_in = store(&home, &replay);
    signed_in.login(|_| {}).expect("the sign-in finishes");

    let refused = signed_in
        .session()
        .expect_err("a session the wire will not take is not a session");

    assert_eq!(refused, SignIn::Revoked, "a revoked token is its own fact");
    let said = refused.message().to_string();
    assert!(
        said.contains(copilot::LOGIN),
        "the message names the command that fixes it: {said}"
    );
    assert!(
        !said.contains(GITHUB_TOKEN),
        "and repeats nothing the store held: {said}"
    );
}

#[test]
fn nothing_the_store_holds_can_be_printed() {
    let home = tempfile::tempdir().expect("a temp config home");
    let replay = Replay::new(sign_in_replies(LIVE));
    let signed_in = store(&home, &replay);
    signed_in.login(|_| {}).expect("the sign-in finishes");

    let secrets = signed_in.secrets();
    assert_eq!(
        secrets.len(),
        2,
        "the session and the GitHub token behind it are both secrets"
    );
    let quoted = secrets
        .redact(format!("refused {LIVE_SESSION} and {GITHUB_TOKEN}"))
        .to_string();
    assert!(
        quoted.contains(MASK),
        "what it prints is the mask: {quoted}"
    );
    assert!(
        !quoted.contains(LIVE_SESSION) && !quoted.contains(GITHUB_TOKEN),
        "no window prints what the store holds: {quoted}"
    );
    let described = format!("{signed_in:?}");
    assert!(
        !described.contains(LIVE_SESSION) && !described.contains(GITHUB_TOKEN),
        "the seam's own description carries no credential: {described}"
    );
}

#[test]
fn the_row_a_sign_in_ends_with_names_the_file_and_its_mode() {
    let home = tempfile::tempdir().expect("a temp config home");
    let signed_in = store(&home, &Replay::default());

    let row = copilot::store_row(&signed_in.path());

    assert!(
        row.contains(&signed_in.path().display().to_string()),
        "the row names the file the session was written to: {row}"
    );
    assert!(row.contains("mode 0600"), "and its mode: {row}");
    assert!(
        row.contains("never shown again"),
        "and that nothing ever shows it: {row}"
    );
}

#[test]
fn a_copilot_ask_is_sent_with_the_session_the_store_holds() {
    let home = tempfile::tempdir().expect("a temp config home");
    let replay = Replay::new(
        sign_in_replies(LIVE)
            .into_iter()
            .chain([Canned::stream(RESPONSES)])
            .collect::<Vec<_>>(),
    );
    let signed_in = store(&home, &replay);
    signed_in.login(|_| {}).expect("the sign-in finishes");
    let client = Client::over(replay.clone(), SecretSet::default()).store(signed_in);

    let answer = client
        .ask(
            &Ask::new(Wire::Copilot, "gpt-5.3-codex").prompt("Say hello"),
            &mut (),
        )
        .expect("an ask on the wire that signs in answers");

    assert_eq!(answer.text, "Hi there");
    let sent = replay.requests().pop().expect("the ask was sent");
    assert!(
        sent.headers.contains(&(
            String::from("authorization"),
            format!("Bearer {LIVE_SESSION}")
        )),
        "the session the store holds is what the wire is asked with: {:?}",
        sent.headers
    );
}

#[test]
fn a_copilot_ask_with_no_sign_in_asks_for_a_login_instead_of_sending() {
    let home = tempfile::tempdir().expect("a temp config home");
    let replay = Replay::default();
    let client = Client::over(replay.clone(), SecretSet::default()).store(store(&home, &replay));

    let failure = client
        .ask(
            &Ask::new(Wire::Copilot, "gpt-4o").prompt("Say hello"),
            &mut (),
        )
        .expect_err("nothing is signed in");

    assert_eq!(failure.verdict, Verdict::Unconfigured);
    assert!(
        failure.message.to_string().contains(copilot::LOGIN),
        "the failure names the command that signs in: {failure}"
    );
    assert!(
        replay.requests().is_empty(),
        "nothing was sent without a session: {:?}",
        replay.requests()
    );
}
