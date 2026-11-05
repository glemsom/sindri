//! The variables a test or an eval scrubs before it starts a run: every setting
//! a run resolves from its environment, because a host's shell that exports one
//! decides what such a run does and the test then proves the shell rather than
//! the tree.
//!
//! The names are read from the module that resolves the settings and not
//! written down a second time here: a setting added to `sindri_core::config` is
//! a test that fails until the harness scrubs it, so the list cannot drift the
//! way a comment does.

#![allow(
    clippy::expect_used,
    reason = "a scan that cannot read the source it walks panics, naming the file"
)]

use std::path::{Path, PathBuf};

use sindri_harness::SETTINGS;

/// The files `sindri_core::config` reads the environment from: its own, and
/// every module beside it, so a read that moves into a file of its own is
/// still a read this scan finds.
fn config_sources() -> Vec<PathBuf> {
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/src");
    let mut sources = vec![core.join("config.rs")];
    let mut siblings: Vec<PathBuf> = std::fs::read_dir(core.join("config"))
        .expect("the config module's directory")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "rs"))
        .collect();
    siblings.sort();
    assert!(
        !siblings.is_empty(),
        "the scan found the modules beside config.rs"
    );
    sources.extend(siblings);
    sources
}

/// Every `SINDRI_*` name a source carries, once each, in the order it first
/// appears: the read sites name one, the const a read is handed names one, and
/// so does a sentence telling a reader to set it — each of them a variable a
/// run resolves.
fn settings_in(source: &str) -> Vec<&str> {
    const PREFIX: &str = "SINDRI_";
    let mut names = Vec::new();
    let mut from = 0;
    while let Some(at) = source[from..].find(PREFIX) {
        let start = from + at;
        let body = &source[start + PREFIX.len()..];
        let end = body
            .find(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .unwrap_or(body.len());
        from = start + PREFIX.len() + end;
        // `SINDRI_*` is a prefix a paragraph may write; a setting has a name.
        if end == 0 {
            continue;
        }
        let name = source[start..from].trim_end_matches('_');
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

#[test]
fn every_setting_a_run_resolves_is_one_the_harness_scrubs() {
    let mut found = 0;
    for file in config_sources() {
        let source = std::fs::read_to_string(&file).expect("the config module's source");
        for name in settings_in(&source) {
            assert!(
                SETTINGS.contains(&name),
                "{} resolves {name} and the harness leaves it to the host's shell",
                file.display()
            );
            found += 1;
        }
    }
    // A scan that found nothing would satisfy every assertion above, so it is
    // held to the eleven settings the module resolves today.
    assert!(
        found >= 11,
        "the scan found the settings the config module resolves: {found}"
    );
}
