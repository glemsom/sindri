//! The skill catalog: what the two scopes hold, what was refused and why, and
//! which scope won when both carried the same name. Every fixture is a tree a
//! user could really have, and nothing here reaches the model.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken scan reports itself"
)]

mod content;
mod discovery;
mod fixtures;
mod frontmatter;
