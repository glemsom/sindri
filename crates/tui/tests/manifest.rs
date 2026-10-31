//! The dependency the frame is built on, as the workspace manifest declares it:
//! crossterm is asked for a Linux terminal's events and never for its `default`
//! set, whose `windows` member pins Win32 bindings nothing here can compile.
//! The manifest is the only place that request is written down, so it is
//! asserted rather than trusted — the reader who wants a feature back reads the
//! line with the comment above it, and is meant to be stopped there.

/// The manifest, as committed: `include_str!` so the test cannot pass against a
/// file it read from the wrong directory.
const MANIFEST: &str = include_str!("../../../Cargo.toml");

/// The workspace's line for crossterm, and the comment lines directly above it —
/// the two a reader takes together.
fn crossterm_declaration() -> (String, String) {
    let lines: Vec<&str> = MANIFEST.lines().collect();
    let index = lines
        .iter()
        .position(|line| line.starts_with("crossterm = "))
        .unwrap_or_else(|| panic!("the manifest declares no `crossterm`"));
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
fn the_manifest_asks_crossterm_for_events_and_never_for_windows() {
    let (comment, line) = crossterm_declaration();

    // `windows` is the feature that names the Win32 bindings — `crossterm_winapi`
    // and `winapi` — and no crate here holds a Windows conditional.
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
    // Keys, mouse and resize reach the frame through `events`; it is what the
    // frame's key and mouse tests would miss.
    assert!(
        line.contains(r#"features = ["events"]"#),
        "the frame is fed by `events` and by no other feature: {line:?}"
    );
    // The reason, in the words the next reader needs before they add it back.
    for word in ["linux", "windows"] {
        assert!(
            comment.to_lowercase().contains(word),
            "the comment above the dependency says what the feature set is and \
             what `windows` would fetch: {comment:?}"
        );
    }
}
