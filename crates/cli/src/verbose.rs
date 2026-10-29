//! `-v`: what the run did, on stderr, after it ran. stdout is left untouched,
//! so a verbose run pipes exactly like a silent one (ADR-0004).

use sindri_core::agent::Event;

/// The lines `-v` prints: one per reasoning block, one per tool call, then the
/// footer naming the session the run was recorded in.
pub fn lines(events: Vec<Event>, session: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for event in events {
        match event {
            Event::Reasoning { turn, text } => {
                for line in text.lines() {
                    lines.push(format!("sindri: reasoning (turn {turn}): {line}"));
                }
            }
            Event::ToolCall { turn, command, .. } => lines.push(match command {
                Some(command) => format!(
                    "sindri: bash (turn {turn}): {}",
                    command.lines().collect::<Vec<_>>().join(" ")
                ),
                None => format!("sindri: bash (turn {turn}): not run, the arguments were unusable"),
            }),
        }
    }
    lines.push(format!("sindri: session {session}"));
    lines
}
