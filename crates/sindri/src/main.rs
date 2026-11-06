//! The Sindri front ends: the terminal UI and batch mode, both driving one core
//! that owns every decision either of them makes.

mod batch; // a prompt in, the answer and one newline out
mod osc; // sequences that carry words out of the terminal
mod tui; // the frame, the rail, the composer and the queue

fn main() {
    // The front ends themselves land in later tickets. This crate exists so that
    // a front end owning a decision the core could make is a compile error.
}
