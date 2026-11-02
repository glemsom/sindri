//! A `file:` URI is the `path` argument in another spelling — one more way to
//! write the string a model could have put in `path`, never a second door. What
//! these prove is the reduction (the authority and the percent escapes) and that
//! what it produces is guarded, and refused, in the words the path it spells is.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod support;

use support::open::{READABLE, Run};
use tempfile::TempDir;

#[test]
fn a_file_uri_naming_an_artefact_inside_the_writable_set_opens() {
    let run = Run::new();
    let report = run.report("notes/report.html");

    let outcome = run.via_url(&format!("file://{}", report.display()));

    assert_eq!(
        run.asked(),
        vec![report.display().to_string()],
        "the host is asked for the resolved file, exactly as it is for a path: {}",
        outcome.content
    );
    assert_eq!(outcome.facts.outcome.as_deref(), Some("started"));
    assert!(!outcome.is_error, "{}", outcome.content);
    assert!(
        outcome
            .content
            .contains(&format!("target: {}\n", report.display())),
        "the result echoes the path the host was given, never the URI: {}",
        outcome.content
    );
    assert!(
        !outcome.content.contains("file://"),
        "and the record cannot tell which spelling the model chose: {}",
        outcome.content
    );
}

#[test]
fn a_percent_encoded_file_uri_decodes_before_the_guard_sees_it() {
    let run = Run::new();
    let report = run.report("a report.html");
    let spelled = format!("file://{}", report.display()).replace(' ', "%20");

    let outcome = run.via_url(&spelled);

    assert_eq!(
        run.asked(),
        vec![report.display().to_string()],
        "`%20` is a space, and the guard reads the name it decodes to: {}",
        outcome.content
    );
    assert!(!outcome.is_error, "{}", outcome.content);
}

#[test]
fn file_localhost_and_an_empty_authority_name_the_same_path() {
    let run = Run::new();
    let report = run.report("chart.svg");
    let path = report.display().to_string();

    for spelling in [
        format!("file://{path}"),
        format!("file://localhost{path}"),
        format!("file://LOCALHOST{path}"),
    ] {
        let outcome = run.via_url(&spelling);

        assert!(!outcome.is_error, "`{spelling}`: {}", outcome.content);
        assert_eq!(
            run.asked().last(),
            Some(&path),
            "`{spelling}` is the same file as `file://{path}`, whatever case the authority is \
             written in: {}",
            outcome.content
        );
    }
}

#[test]
fn a_file_uri_naming_an_authority_that_is_not_localhost_is_refused_naming_it() {
    let run = Run::new();
    let report = run.report("report.html");
    let path = report.display().to_string();

    // Every authority but none at all and `localhost`: another machine, a
    // userinfo a `file:` URI may not carry, a port that names no file, an IPv6
    // loopback spelled as itself, and the backslash a model may reach for.
    for (authority, named) in [
        ("otherhost", "otherhost"),
        ("user@localhost", "user@localhost"),
        ("localhost:80", "localhost:80"),
        ("[::1]", "[::1]"),
        (r"\otherhost", r"\otherhost"),
    ] {
        let outcome = run.via_url(&format!("file://{authority}{path}"));

        assert!(
            run.asked().is_empty(),
            "`{authority}` names something other than this machine's own file: {:?}",
            run.asked()
        );
        assert!(outcome.is_error, "{}", outcome.content);
        assert!(
            outcome.content.contains(named),
            "the refusal names the authority it got, `{named}`: {}",
            outcome.content
        );
    }
}

#[test]
fn a_file_uri_naming_a_blocked_extension_is_refused_in_the_words_a_path_gets() {
    let run = Run::new();
    let payload = run.report("payload.desktop");
    let as_path = run.shown(&payload.display().to_string());

    // The dot encoded as well, so a check made on the spelling the URI carries
    // would see no extension at all: the decoded path is the one guarded.
    for uri in [
        format!("file://{}", payload.display()),
        format!("file://{}", payload.display()).replace(".desktop", "%2Edesktop"),
    ] {
        let outcome = run.via_url(&uri);

        assert!(
            run.asked().is_empty(),
            "`{uri}` must never reach the host's mimetype guesser"
        );
        assert!(outcome.is_error, "{}", outcome.content);
        assert!(
            outcome.content.contains(READABLE),
            "the refusal names every kind that would have opened: {}",
            outcome.content
        );
        assert_eq!(
            outcome.content, as_path.content,
            "`{uri}` is refused in exactly the words its path is refused in"
        );
    }
}

#[test]
fn a_file_uri_opens_nothing_a_path_could_not() {
    let run = Run::new();
    let workspace = run
        .workspace
        .path()
        .canonicalize()
        .expect("a canonical workspace");
    let outside = TempDir::new().expect("a tree the run may not write");
    let escaped = run.write(outside.path(), "report.html");
    let sibling = outside
        .path()
        .file_name()
        .expect("a named temp dir")
        .to_string_lossy()
        .into_owned();
    let missing = workspace.join("missing.html");
    let directory = workspace.join("pages.html");
    std::fs::create_dir(&directory).expect("a directory");
    let link = workspace.join("escape.html");
    std::os::unix::fs::symlink(&escaped, &link).expect("a link out of the writable set");
    // `..` hidden in a percent escape: a path out of the writable set once it is
    // decoded, and the decoded path is the one the guard reads.
    let through_parent = format!("{}/../{sibling}/report.html", workspace.display());

    for (uri, path) in [
        (
            format!("file://{}", escaped.display()),
            escaped.display().to_string(),
        ),
        (
            String::from("file:///etc/passwd"),
            String::from("/etc/passwd"),
        ),
        (String::from("file://localhost/"), String::from("/")),
        (
            format!("file://{}", missing.display()),
            missing.display().to_string(),
        ),
        (
            format!("file://{}", directory.display()),
            directory.display().to_string(),
        ),
        (
            format!("file://{}", link.display()),
            link.display().to_string(),
        ),
        (
            format!("file://{}", through_parent.replace("..", "%2e%2e")),
            through_parent.clone(),
        ),
    ] {
        let outcome = run.via_url(&uri);
        let as_path = run.shown(&path);

        assert!(
            run.asked().is_empty(),
            "`{uri}` must never reach the host: {:?}",
            run.asked()
        );
        assert!(outcome.is_error, "`{uri}` is no open: {}", outcome.content);
        assert_eq!(
            outcome.content, as_path.content,
            "`{uri}` is refused in exactly the words `{path}` is refused in"
        );
        assert!(
            !outcome.content.contains("file://"),
            "and the refusal does not say which spelling was chosen: {}",
            outcome.content
        );
    }
}

#[test]
fn a_percent_encoded_separator_is_a_separator_the_guard_reads() {
    let run = Run::new();
    let workspace = run
        .workspace
        .path()
        .canonicalize()
        .expect("a canonical workspace");
    let outside = TempDir::new().expect("a tree the run may not write");
    let escaped = run.write(outside.path(), "report.html");
    let spelled = escaped.display().to_string().replace('/', "%2F");
    // Every separator encoded, so the traversal out of the writable set is one
    // segment long to a reader that looks before decoding — and the decoded path
    // is the one guarded.
    let through_parent = format!("{}/../{}", workspace.display(), escaped.display());
    let hidden = through_parent.replace('/', "%2F");

    for (uri, path) in [
        (format!("file:{spelled}"), escaped.display().to_string()),
        (format!("file://{spelled}"), escaped.display().to_string()),
        (format!("file://{hidden}"), through_parent),
    ] {
        let outcome = run.via_url(&uri);
        let as_path = run.shown(&path);

        assert!(
            run.asked().is_empty(),
            "`{uri}` must never reach the host: {:?}",
            run.asked()
        );
        assert!(outcome.is_error, "`{uri}` is no open: {}", outcome.content);
        assert_eq!(
            outcome.content, as_path.content,
            "`%2F` is a separator, and `{uri}` is refused in exactly the words `{path}` is"
        );
    }
}

#[test]
fn a_file_uri_naming_no_path_at_all_is_refused() {
    let run = Run::new();

    for uri in ["file:", "file://", "file://localhost"] {
        let outcome = run.via_url(uri);

        assert!(run.asked().is_empty(), "`{uri}` names nothing to show");
        assert!(outcome.is_error, "{}", outcome.content);
        assert!(
            outcome.content.contains("names no path"),
            "the refusal says there is no path in it: {}",
            outcome.content
        );
    }
}
