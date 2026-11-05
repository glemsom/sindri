//! The dialects the provider seam sends, asserted where a reader learns them:
//! the glossary's **wire** entry and the paragraph that opens the seam itself.
//! The set is the code's — three dialects, two of them named by their API and
//! the third chosen by model name — so neither document may state a smaller one
//! and leave a reader believing every Copilot run is Chat Completions.

#![allow(
    clippy::panic,
    reason = "a panicking test is how a broken contract reports itself"
)]

/// The glossary, as committed: `include_str!` so the test cannot pass against a
/// file it read from the wrong directory.
const GLOSSARY: &str = include_str!("../../../GLOSSARY.md");

/// The provider seam's source, as committed: what the seam sends is the code's
/// fact, and the paragraph a reader meets first is read from here.
const SEAM: &str = include_str!("../../../crates/core/src/provider/rig_wire.rs");

/// The dialects the seam sends, as the provider library spells them: the two an
/// API names itself by, and Responses, the one a Copilot model is routed to.
const DIALECTS: [&str; 3] = ["Anthropic Messages", "Chat Completions", "Responses"];

/// One glossary entry, its lines joined so an assertion reads a claim rather
/// than a wrap.
fn entry(word: &str) -> String {
    GLOSSARY
        .split("\n\n")
        .find(|entry| entry.starts_with(&format!("**{word}**")))
        .map(|entry| entry.replace('\n', " "))
        .unwrap_or_else(|| panic!("GLOSSARY.md has no **{word}** entry"))
}

/// The paragraph the seam opens with, its lines joined so an assertion reads a
/// claim rather than a wrap — the same read the audit makes of every module.
fn seam_paragraph() -> String {
    SEAM.split("\n\n")
        .next()
        .unwrap_or_default()
        .lines()
        .map(|line| line.trim_start_matches("//!").trim())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn the_glossary_wire_entry_names_every_dialect_the_seam_sends() {
    let wire = entry("wire");
    for dialect in DIALECTS {
        assert!(
            wire.contains(dialect),
            "the seam sends {dialect:?}, and the entry that lists the dialects must \
             carry it: {wire:?}"
        );
    }
    assert!(
        wire.contains("Copilot"),
        "a Copilot model may be routed to Responses, and the entry must say so: \
         {wire:?}"
    );
    assert!(
        SEAM.contains("Route::Responses"),
        "the entry names a dialect the seam sends, so the seam must still select it"
    );
}

#[test]
fn the_provider_seam_paragraph_names_every_dialect_it_sends() {
    let paragraph = seam_paragraph();
    for dialect in DIALECTS {
        assert!(
            paragraph.contains(dialect),
            "the paragraph that opens the seam is where a reader learns what it \
             speaks, and {dialect:?} is missing from it: {paragraph:?}"
        );
    }
}
