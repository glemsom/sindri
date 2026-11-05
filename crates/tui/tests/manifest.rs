//! The dependencies the frame is built on, as the workspace manifest declares
//! them: crossterm is asked for a Linux terminal's events and the paste it sends
//! once it has been asked, and never for its `default` set, whose `windows`
//! member pins Win32 bindings nothing here can compile, and `image` is asked for
//! exactly the formats the picture store draws. The manifest is the only place
//! those requests are written down, so they are asserted rather than trusted —
//! the reader who wants a feature back reads the line with the comment above it,
//! and is meant to be stopped there.

#![allow(
    clippy::panic,
    reason = "a panicking test is how a broken contract reports itself"
)]

/// The manifest, as committed: `include_str!` so the test cannot pass against a
/// file it read from the wrong directory.
const MANIFEST: &str = include_str!("../../../Cargo.toml");

/// The picture source, as committed, for the formats it says may be drawn.
const PICTURE_SOURCE: &str = include_str!("../src/picture/source.rs");

/// The workspace's line for `name`, and the comment lines directly above it —
/// the two a reader takes together.
fn declaration(name: &str) -> (String, String) {
    let lines: Vec<&str> = MANIFEST.lines().collect();
    let index = lines
        .iter()
        .position(|line| line.starts_with(&format!("{name} = ")))
        .unwrap_or_else(|| panic!("the manifest declares no `{name}`"));
    let mut above: Vec<&str> = lines[..index]
        .iter()
        .rev()
        .take_while(|line| line.trim_start().starts_with('#'))
        .copied()
        .collect();
    above.reverse();
    (above.join(" "), lines[index].to_string())
}

#[test]
fn the_manifest_asks_crossterm_for_events_and_paste_and_never_for_windows() {
    let (comment, line) = declaration("crossterm");

    // What this test refuses is `windows`, the feature that names the Win32
    // bindings — `crossterm_winapi` and `winapi` — because no crate here holds
    // a Windows conditional. Paste is not refused: a pasted prompt arrives as
    // one event and must not arrive as a keystroke per line break, so the
    // feature is asked for beside `events`.
    assert!(
        !line.contains("windows"),
        "the `windows` feature asks for Win32 bindings nothing here compiles: {line:?}"
    );
    // Named rather than defaulted: crossterm's `default` set is `bracketed-paste`,
    // `windows` and `events`, so taking it would ask for the feature just refused.
    assert!(
        line.contains("default-features = false"),
        "crossterm's defaults are not the set the frame wants: {line:?}"
    );
    // Keys, mouse and resize reach the frame through `events`, and a paste
    // through `bracketed-paste`: both are what the frame's tests would miss.
    assert_eq!(
        features(&line),
        ["events", "bracketed-paste"],
        "the frame is fed by `events` and by the paste the terminal sends once it \
         is asked, and by no other feature: {line:?}"
    );
    // The reason, in the words the next reader needs before they add it back.
    for word in ["linux", "windows", "paste"] {
        assert!(
            comment.to_lowercase().contains(word),
            "the comment above the dependency says what the feature set is, what \
             `windows` would fetch and why paste is in it: {comment:?}"
        );
    }
}

/// What `text` holds between `opener` and the first `closer` after it — the one
/// way this file reads a list out of a file, whether the list is `DRAWS` or a
/// feature set.
fn between<'a>(text: &'a str, opener: &str, closer: &str) -> &'a str {
    let start = text
        .find(opener)
        .unwrap_or_else(|| panic!("no {opener:?} to read"));
    let rest = &text[start + opener.len()..];
    let end = rest
        .find(closer)
        .unwrap_or_else(|| panic!("{opener:?} never closes with {closer:?}"));
    &rest[..end]
}

/// The formats the store draws, as words the manifest could ask for: `DRAWS`
/// in the picture source, whose `ImageFormat` variants are the crate's own names
/// for them, lowercased because a cargo feature is spelled that way.
fn drawn_formats() -> Vec<String> {
    let block = between(PICTURE_SOURCE, "const DRAWS:", "];");
    block
        .match_indices("ImageFormat::")
        .map(|(at, marker)| {
            let variant: String = block[at + marker.len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            assert!(
                !variant.is_empty(),
                "`DRAWS` names no format after `ImageFormat::`"
            );
            variant.to_ascii_lowercase()
        })
        .collect()
}

/// The features a manifest line asks for, as it writes them.
fn features(line: &str) -> Vec<String> {
    between(line, "features = [", "]")
        .split(',')
        .map(|feature| feature.trim().trim_matches('"').to_owned())
        .filter(|feature| !feature.is_empty())
        .collect()
}

#[test]
fn the_image_crate_is_asked_for_every_format_the_store_draws() {
    let (_, line) = declaration("image");
    let drawn = drawn_formats();
    assert_eq!(
        drawn.len(),
        4,
        "the store draws four formats, and the manifest is read against them: {drawn:?}"
    );
    // Defaults off, as the crossterm line's are: `image`'s default set is every
    // format it can decode, which is more than the store draws and more than the
    // store's refusals name.
    assert!(
        line.contains("default-features = false"),
        "`image`'s defaults are not the set the store draws: {line:?}"
    );
    let asked = features(&line);
    for format in &drawn {
        assert!(
            asked.contains(format),
            "the store draws {format:?} and the manifest never asks for it: {line:?}"
        );
    }
    for feature in &asked {
        assert!(
            drawn.contains(feature),
            "the manifest asks for {feature:?}, which the store does not draw: {line:?}"
        );
    }
}
