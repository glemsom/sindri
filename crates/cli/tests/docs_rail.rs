//! The rail's list of facts, in the two documents that state it: every fact the
//! rail draws is a fact the glossary's **rail** entry and the README's rail
//! bullet name, and the figure the README's pulse bullet promises moves is one
//! the rail moves. A fact the rail grows that a document has not learned is a
//! reader told the rail holds less than it does.

#![allow(
    clippy::panic,
    reason = "a panicking test is how a broken contract reports itself"
)]

use std::time::Duration;

use sindri_core::provider::Usage;
use sindri_tui::Launch;
use sindri_tui::driver::Stop;
use sindri_tui::rail::{Fact, Rail};

/// The glossary, as committed: `include_str!` so the test cannot pass against a
/// file it read from the wrong directory.
const GLOSSARY: &str = include_str!("../../../GLOSSARY.md");

/// The README, as committed: the rail's bullet is the one thing a reader of the
/// terminal UI section is promised, so it is asserted rather than trusted.
const README: &str = include_str!("../../../README.md");

/// One glossary entry, its lines joined so an assertion reads a claim rather
/// than a wrap.
fn entry(word: &str) -> String {
    GLOSSARY
        .split("\n\n")
        .find(|entry| entry.starts_with(&format!("**{word}**")))
        .map(|entry| entry.replace('\n', " "))
        .unwrap_or_else(|| panic!("GLOSSARY.md has no **{word}** entry"))
}

/// One README bullet by the bold word it opens with, its wrap undone: a claim
/// that crosses a line break is one claim.
fn bullet(word: &str) -> String {
    README
        .split("\n- **")
        .find(|bullet| bullet.starts_with(&format!("{word}:**")))
        .map(|bullet| bullet.replace('\n', " "))
        .unwrap_or_else(|| panic!("README.md has no **{word}** bullet"))
}

/// A launch with a value under every label the rail states.
fn launch() -> Launch {
    Launch {
        session: "4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77".to_string(),
        temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
        branch: Some("522-rail-facts-list".to_string()),
        sandbox: "bwrap · 1 writable".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        base_url: None,
        workspace: "~/git/sindri".to_string(),
        skills: vec!["house-style".to_string()],
        max_turns: 250,
        context_limit: Some(200_000),
        status: true,
    }
}

/// A rail holding one of everything: a settled run gives the stop word, and a
/// skill in force gives a chip a rail with nothing loaded has not got.
fn drawn() -> Rail {
    let mut rail = Rail::new(launch());
    rail.skill_loaded("house-style");
    rail.settled(1, &Usage::default(), Stop::Final, Duration::ZERO);
    rail
}

#[test]
fn the_glossary_and_the_readme_name_every_fact_the_rail_draws() {
    let rail = entry("rail");
    let readme = bullet("Rail");
    for fact in drawn().facts(Duration::ZERO) {
        let label = fact.label();
        for placed in [&rail, &readme] {
            assert!(
                placed.contains(label),
                "the rail draws {label:?}, so the entry and the bullet must name it: \
                 {placed:?}"
            );
        }
    }
}

#[test]
fn the_readme_promises_motion_only_for_a_figure_the_rail_moves() {
    // What the pulse promises is what the rail does while a run streams, so the
    // figures that move there are read off the rail: it accents the one it has
    // just grown, and a figure nobody drew cannot be one of them.
    let mut streaming = Rail::new(launch());
    streaming.begun();
    streaming.streamed(400, Duration::ZERO);
    let facts: Vec<Fact> = streaming.facts(Duration::ZERO);
    let moving: Vec<&str> = facts
        .iter()
        .filter(|fact| fact.fresh())
        .map(Fact::label)
        .collect();
    assert_eq!(
        moving,
        ["context"],
        "the context meter is the one figure a stream moves: {facts:?}"
    );

    let pulse = bullet("Pulse");
    for label in moving {
        assert!(
            pulse.contains(label),
            "the pulse bullet promises what moves, and the rail moves {label:?}: {pulse:?}"
        );
    }
    assert!(
        !pulse.contains("output"),
        "the rail draws no output figure, so the pulse bullet cannot say one moves: \
         {pulse:?}"
    );
}
