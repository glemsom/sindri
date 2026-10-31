//! A record as text: `sindri sessions show` printing one run whole, for a human
//! reading a transcript. Every fact the record holds is said here — the
//! boundary and dials it ran under, what each turn cost, what each call did,
//! what a crash took and what bubblewrap said — because a reader who has to
//! open the files to learn one of them has been handed a second job.

use super::entry::Entry;
use super::stored::Stored;
use crate::provider::Usage;

impl Stored {
    pub fn render(&self) -> String {
        let mut out = format!(
            "session {} ({}/{}); recorded by sindri {}\n",
            self.meta.id, self.meta.provider, self.meta.model, self.meta.version
        );
        out.push_str(&format!("sandbox: {}\n", self.meta.enforcement));
        // Only what the record holds: an old record names no tree, and a run
        // outside a repository has no git facts to state.
        if !self.meta.workspace.is_empty() {
            out.push_str(&format!("workspace: {}", self.meta.workspace));
            if let Some(git) = &self.meta.git {
                let dirty = if git.dirty { ", dirty" } else { "" };
                out.push_str(&format!(" ({} @ {}{dirty})", git.branch, git.commit));
            }
            out.push('\n');
        }
        // `max_turns` is zero only in a record written before the dials were
        // kept, and no run obeyed no cap.
        if self.meta.max_turns > 0 {
            let thinking = if self.meta.thinking { "on" } else { "off" };
            out.push_str(&format!(
                "dials: max turns {}, thinking {thinking}\n",
                self.meta.max_turns
            ));
        }
        out.push_str(&format!(
            "started: {} ms since the epoch\n",
            self.meta.started_ms
        ));
        match (self.meta.ended_ms, self.meta.duration_ms) {
            (Some(ended), Some(duration)) => {
                out.push_str(&format!("ended: {ended} after {duration} ms\n"));
            }
            (Some(ended), None) => out.push_str(&format!("ended: {ended}\n")),
            (None, _) => out.push_str("ended: never — the run did not finish\n"),
        }
        out.push_str(&format!("stopped: {}", self.meta.stopped));
        if let (Some(turns), Some(tool_calls)) = (self.meta.turns, self.meta.tool_calls) {
            out.push_str(&format!(
                ", after {turns} turns and {tool_calls} tool calls"
            ));
        }
        out.push('\n');
        if let Some(answered) = &self.meta.answered_model {
            out.push_str(&format!("answered by: {answered}\n"));
        }
        out.push_str(&format!("usage: {}\n", usage(self.meta.usage.as_ref())));
        if let Some(error) = &self.meta.error {
            out.push_str(&format!("error: {error}\n"));
        }
        // What the crash took is a fact of the header, not of a turn: a line
        // that did not parse has no place in the sequence to be printed at.
        if self.unread > 0 {
            let line = if self.unread == 1 { "line" } else { "lines" };
            out.push_str(&format!("{} {line} could not be read\n", self.unread));
        }

        for entry in &self.entries {
            match entry {
                Entry::User { text } => {
                    out.push_str("\n--- user ---\n");
                    out.push_str(text);
                    end_line(&mut out, text);
                }
                Entry::Assistant {
                    turn,
                    text,
                    reasoning,
                    usage,
                    duration_ms,
                } => {
                    out.push_str(&format!("\n--- assistant (turn {turn}) ---\n"));
                    out.push_str(text);
                    end_line(&mut out, text);
                    if let Some(cost) = cost(usage.as_ref(), *duration_ms) {
                        out.push_str(&format!("turn cost: {cost}\n"));
                    }
                    if let Some(reasoning) = reasoning {
                        out.push_str(&format!("reasoning: {reasoning}\n"));
                    }
                }
                Entry::ToolCall {
                    turn,
                    call_id,
                    tool,
                    command,
                    problem,
                } => {
                    out.push_str(&format!("\n--- {tool} (turn {turn}) {call_id} ---\n"));
                    match command {
                        Some(command) => {
                            out.push_str(&format!("$ {command}\n"));
                        }
                        None => out.push_str(&format!(
                            "not run: {}\n",
                            problem.as_deref().unwrap_or("the arguments were unusable")
                        )),
                    }
                }
                Entry::Policy { description } => {
                    out.push_str("\n--- policy (from here on) ---\n");
                    out.push_str(description);
                    end_line(&mut out, description);
                }
                Entry::ToolResult {
                    turn,
                    call_id,
                    content,
                    is_error,
                    outcome,
                    exit_code,
                    duration_ms,
                    truncated,
                    spill,
                } => {
                    let failed = if *is_error { " (failed)" } else { "" };
                    out.push_str(&format!(
                        "\n--- result (turn {turn}) {call_id}{failed} ---\n"
                    ));
                    // The same facts the model read, on the line above the text
                    // it read them in: a reader must not parse a prompt to learn
                    // an exit code either. A record written before these were
                    // kept states none of them, and the line it never had is
                    // not invented for it.
                    let facts = result_facts(
                        outcome.as_deref(),
                        *exit_code,
                        *duration_ms,
                        *truncated,
                        spill,
                    );
                    if !facts.is_empty() {
                        out.push_str(&facts);
                        out.push('\n');
                    }
                    for line in content.lines() {
                        out.push_str(&format!("  {line}\n"));
                    }
                }
            }
        }

        if !self.status.is_empty() {
            out.push_str("\n--- sandbox status ---\n");
            for line in &self.status {
                out.push_str(line);
                out.push('\n');
            }
        }
        out
    }
}

/// A turn's cost, in the one line the turn has room for: the span the loop
/// measured and the counters the provider reported, each only when the record
/// holds it. A turn nobody measured says nothing, because zero is a
/// measurement and an absent one is not a zero.
fn cost(usage: Option<&Usage>, duration_ms: Option<u64>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(ms) = duration_ms {
        parts.push(format!("{ms} ms"));
    }
    if let Some(usage) = usage {
        for (name, value) in [
            ("input", usage.input_tokens),
            ("output", usage.output_tokens),
            ("cached", usage.cached_input_tokens),
            ("reasoning", usage.reasoning_tokens),
            ("total", usage.total_tokens),
        ] {
            if let Some(value) = value {
                parts.push(format!("{name} {value}"));
            }
        }
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// A call's outcome, exit status, span and what became of its text, as the
/// line above the text it was read in: what the record states, never a word
/// invented for a fact it lacks. An entry that states none of them — one
/// written before they were kept — says nothing at all.
fn result_facts(
    outcome: Option<&str>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    truncated: bool,
    spill: &[String],
) -> String {
    let mut parts = Vec::new();
    match (outcome, exit_code) {
        (Some(word), Some(code)) => parts.push(format!("{word} ({code})")),
        (Some(word), None) => parts.push(word.to_string()),
        (None, _) => {}
    }
    if let Some(ms) = duration_ms {
        parts.push(format!("{ms} ms"));
    }
    if truncated {
        parts.push("truncated".to_string());
    }
    for path in spill {
        parts.push(format!("spill {path}"));
    }
    parts.join(", ")
}

fn end_line(out: &mut String, text: &str) {
    if !text.ends_with('\n') {
        out.push('\n');
    }
}

fn usage(usage: Option<&crate::provider::Usage>) -> String {
    let Some(usage) = usage else {
        return "none reported".to_string();
    };
    let counter = |name: &str, value: Option<u64>| match value {
        Some(value) => format!("{name} {value}"),
        None => format!("{name} unreported"),
    };
    [
        counter("input", usage.input_tokens),
        counter("output", usage.output_tokens),
        counter("cached", usage.cached_input_tokens),
        counter("reasoning", usage.reasoning_tokens),
        counter("total", usage.total_tokens),
    ]
    .join(", ")
}
