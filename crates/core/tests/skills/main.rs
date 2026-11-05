//! The skill catalog: what the three scopes hold, what was refused and why, and
//! which scope won when both carried the same name. Every fixture is a tree a
//! user could really have, and nothing here reaches the model.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a panicking test is how a broken scan reports itself"
)]

mod builtin;
mod content;
mod discovery;
mod fixtures;
mod frontmatter;
