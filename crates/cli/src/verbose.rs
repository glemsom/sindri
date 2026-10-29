//! `-v`: what the run did, on stderr, after it ran. stdout is left untouched,
//! so a verbose run pipes exactly like a silent one (ADR-0004).

use sindri_core::agent::{Event, Report, Stopped};
use sindri_core::provider::Usage;

/// The lines `-v` prints: one per reasoning block, one per tool call, then the
/// footer naming the session and what the run cost.
pub fn lines(events: Vec<Event>, session: &str, report: &Report) -> Vec<String> {
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
    lines.push(format!(
        "sindri: session {session}: {} turn(s), {} tool call(s), {} stopped by {}",
        report.turns,
        report.tool_calls,
        usage(&report.usage),
        match report.stopped {
            Stopped::Final => "the model",
            Stopped::MaxTurns => "the turn cap",
        }
    ));
    lines
}

/// The counters worth showing, with unreported ones simply absent.
fn usage(usage: &Usage) -> String {
    let mut parts = Vec::new();
    if let (Some(input), Some(output)) = (usage.input_tokens, usage.output_tokens) {
        parts.push(format!("{input} input + {output} output tokens"));
    }
    if let Some(cached) = usage.cached_input_tokens {
        parts.push(format!("{cached} cached"));
    }
    if let Some(reasoning) = usage.reasoning_tokens {
        parts.push(format!("{reasoning} reasoning"));
    }
    if parts.is_empty() {
        return "usage unreported".to_string();
    }
    parts.join(", ")
}
