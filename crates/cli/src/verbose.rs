//! `-v`: what the run did and what it stood in, on stderr, after it ran. stdout
//! is left untouched, so a verbose run pipes exactly like a silent one.

use sindri_core::agent::Event;
use sindri_core::provider::Asked;

pub fn lines(events: Vec<Event>, session: &str, instructions: Option<String>) -> Vec<String> {
    let mut lines = Vec::new();
    // What the run was working to, said before the session it was working in:
    // the same sentence `sindri doctor` prints, so the two cannot drift.
    lines.extend(instructions.map(|line| format!("sindri: {line}")));
    for event in events {
        match event {
            Event::Reasoning { turn, text } => {
                for line in text.lines() {
                    lines.push(format!("sindri: reasoning (turn {turn}): {line}"));
                }
            }
            // A delta is a fragment of what `Reasoning` and the tool calls say
            // whole: a verbose run reports the turn, not every chunk.
            Event::TextDelta { .. } | Event::ReasoningDelta { .. } => {}
            // A load is said once, where the tool call that asked for it was
            // already said: the body is what the model reads, not what a
            // verbose run prints.
            Event::SkillLoaded { .. } => {}
            Event::ToolCall { turn, asked } => lines.push(match asked {
                Asked::Bash { command } => format!(
                    "sindri: bash (turn {turn}): {}",
                    command.lines().collect::<Vec<_>>().join(" ")
                ),
                Asked::Skill { name } => format!("sindri: skill (turn {turn}): {name}"),
                Asked::Open { target } => format!("sindri: open (turn {turn}): {target}"),
                Asked::Refused { named, problem } => {
                    format!("sindri: {named} (turn {turn}): not run, {problem}")
                }
            }),
        }
    }
    lines.push(format!("sindri: session {session}"));
    lines
}
