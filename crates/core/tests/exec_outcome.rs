//! What the model is handed of a command's output: `truncate` decides the head,
//! the tail, the marker naming the bytes between them and the spill file, so
//! each shape is driven here directly rather than through a sandbox.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken cut reports itself"
)]

use sindri_core::exec::{OutputLimit, truncate};
use tempfile::TempDir;

#[test]
fn a_stream_past_head_and_tail_keeps_its_ends_and_names_the_bytes_between() {
    let dir = TempDir::new().expect("a temp directory");
    let path = dir.path().join("call-stdout.txt");
    let limit = OutputLimit::default();
    let stream = vec![b'x'; 20_000];

    let cut = truncate(&stream, limit, Some(&path));

    assert!(cut.truncated);
    let omitted = stream.len() - limit.head - limit.tail;
    let expected = format!(
        "{}\n… {omitted} bytes omitted …\n{}\nfull output: {}",
        String::from_utf8_lossy(&stream[..limit.head]),
        String::from_utf8_lossy(&stream[stream.len() - limit.tail..]),
        path.display()
    );
    assert_eq!(cut.text, expected);
    assert_eq!(cut.spill.as_deref(), Some(path.as_path()));
    assert_eq!(
        std::fs::read(&path).expect("the spilled file"),
        stream,
        "the whole stream is spilled, not the truncated text"
    );
}

#[test]
fn a_cut_that_would_split_a_glyph_moves_to_the_boundary_and_spills_the_whole() {
    let dir = TempDir::new().expect("a temp directory");
    let path = dir.path().join("cut.txt");
    // `€` is three bytes wide and one straddles each cut: `head: 4` lands on its
    // third byte, `len - tail` on the second byte of the trailing one.
    let stream = format!("ab€{}€ef", "x".repeat(10)).into_bytes();
    let limit = OutputLimit { head: 4, tail: 4 };

    let cut = truncate(&stream, limit, Some(&path));

    assert!(cut.truncated);
    assert_eq!(
        cut.text,
        format!(
            "ab\n… 16 bytes omitted …\nef\nfull output: {}",
            path.display()
        ),
        "the head stops before the glyph and the tail starts after it"
    );
    assert_eq!(cut.spill.as_deref(), Some(path.as_path()));
    assert_eq!(std::fs::read(&path).expect("the spilled file"), stream);
}

#[test]
fn a_missing_spill_directory_is_said_and_keeps_no_file() {
    let stream = vec![b'x'; 20_000];

    let cut = truncate(&stream, OutputLimit::default(), None);

    assert!(cut.truncated);
    assert!(cut.spill.is_none(), "nothing is written without a path");
    let last = cut.text.rsplit('\n').next().expect("a last line");
    assert_eq!(last, "full output was not kept: no spill directory");
}

#[test]
fn a_stream_that_fits_comes_back_whole_and_writes_nothing() {
    let dir = TempDir::new().expect("a temp directory");
    let path = dir.path().join("short.txt");
    let limit = OutputLimit::default();
    // Exactly head plus tail, ending on a glyph: the widest stream that still fits.
    let whole = format!("{}€", "x".repeat(limit.head + limit.tail - 3));

    let cut = truncate(whole.as_bytes(), limit, Some(&path));

    assert!(!cut.truncated);
    assert!(cut.spill.is_none());
    assert_eq!(cut.text, whole);
    assert!(!path.exists(), "a stream that fits spills nothing");
}
