//! A record as markdown: `/export` writes the current thread, and `sindri
//! sessions show` prints the same entries. Both walk the record's
//! own [`Entry`] values, so the two cannot disagree about what a session said —
//! only about how they say it.

use std::fmt::Write as _;

use super::entry::Entry;
use super::stored::Stored;

impl Stored {
    /// The thread as one markdown document: what the run was, then every entry
    /// in the order it happened, as headings and fenced blocks.
    pub fn markdown(&self) -> String {
        let mut out = format!("# session {}\n\n", self.meta.id);
        let _ = writeln!(
            out,
            "{}/{} · recorded by sindri {} · sandbox {}\n",
            self.meta.provider, self.meta.model, self.meta.version, self.meta.enforcement
        );
        let _ = write!(
            out,
            "started {} ms since the epoch · stopped {}",
            self.meta.started_ms, self.meta.stopped
        );
        if let (Some(turns), Some(tool_calls)) = (self.meta.turns, self.meta.tool_calls) {
            let _ = write!(out, " after {turns} turns and {tool_calls} tool calls");
        }
        out.push('\n');
        if let Some(error) = &self.meta.error {
            let _ = writeln!(out, "\nerror: {error}");
        }

        for entry in &self.entries {
            match entry {
                Entry::User { text } => {
                    let _ = write!(out, "\n## you\n\n{}", block(text));
                }
                Entry::Assistant {
                    turn,
                    text,
                    reasoning,
                } => {
                    let _ = write!(out, "\n## assistant (turn {turn})\n\n{}", block(text));
                    if let Some(reasoning) = reasoning {
                        let _ = write!(out, "\n> reasoned: {}\n", one_line(reasoning));
                    }
                }
                Entry::ToolCall {
                    turn,
                    call_id,
                    tool,
                    command,
                    problem,
                } => {
                    let _ = write!(out, "\n### {tool} `{call_id}` (turn {turn})\n\n");
                    match command {
                        Some(command) => {
                            let _ = write!(
                                out,
                                "{}sh\n{}\n{}\n",
                                fence(command),
                                command.trim_end(),
                                fence(command)
                            );
                        }
                        None => {
                            let _ = writeln!(
                                out,
                                "not run: {}",
                                one_line(
                                    problem.as_deref().unwrap_or("the arguments were unusable")
                                )
                            );
                        }
                    }
                }
                Entry::ToolResult {
                    turn,
                    call_id,
                    content,
                    is_error,
                } => {
                    let failed = if *is_error { " (failed)" } else { "" };
                    let _ = write!(
                        out,
                        "\n### result `{call_id}` (turn {turn}){failed}\n\n{}\n{}\n{}\n",
                        fence(content),
                        content.trim_end(),
                        fence(content)
                    );
                }
            }
        }
        out
    }
}

/// A fence longer than any run of backticks the content holds, so a tool result
/// that quotes markdown cannot close the block it is written into — the file
/// must read back as the same conversation it was made from.
fn fence(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for ch in text.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat((longest + 1).max(3))
}

/// A message's text as it is, ending in exactly one newline: a document that
/// closes a block must not depend on the run's luck with trailing whitespace.
fn block(text: &str) -> String {
    format!("{}\n", text.trim_end())
}

/// A one-line fact inside a blockquote: a newline would end the quote and
/// hand the rest of the sentence to the document.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
