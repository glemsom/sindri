//! What the `bash` tool returns to the model: a structured outcome and the two
//! streams, head-and-tail truncated with the full text spilled to a file
//! (ADR-0007).

use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Why the command stopped — never a bare exit code, which in a pipeline can
/// belong to the wrong command (ADR-0007).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    /// The command ran and exited.
    Exited {
        /// The script's own exit status; when a signal killed it, 128 + the
        /// signal number, exactly as a shell would report it.
        exit_code: i32,
    },
    /// Our own deadline fired; the process group was signalled and killed.
    TimedOut,
    /// The sandbox never started, so no command ever ran.
    SandboxError {
        /// bwrap's own complaint, so the model can tell this from a failure.
        detail: String,
    },
}

/// One command's structured result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolResult {
    /// How the command ended.
    #[serde(flatten)]
    pub outcome: Outcome,
    /// The command's standard output, possibly truncated.
    pub stdout: String,
    /// The command's standard error, possibly truncated.
    pub stderr: String,
    /// Whether either stream was cut down, whatever became of the spill.
    pub truncated: bool,
    /// Where the untruncated streams were written, when truncation happened.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spill: Vec<PathBuf>,
    /// How long the command took.
    pub duration_ms: u64,
}

/// How much of each stream the model gets: this much head, this much tail.
#[derive(Debug, Clone, Copy)]
pub struct OutputLimit {
    /// Bytes kept from the start of the stream.
    pub head: usize,
    /// Bytes kept from the end of the stream.
    pub tail: usize,
}

impl Default for OutputLimit {
    fn default() -> Self {
        OutputLimit {
            head: 8192,
            tail: 8192,
        }
    }
}

/// One stream after truncation: the model-facing text, where the rest went, and
/// whether it went anywhere at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cut {
    /// What the model reads.
    pub text: String,
    /// The spill file, when the full text reached disk.
    pub spill: Option<PathBuf>,
    /// Whether the stream was cut down.
    pub truncated: bool,
}

/// Truncates one stream head-and-tail, spilling the full text when it does not
/// fit. A spill that fails is said in the text: losing output silently would
/// break the promise that nothing is lost.
pub fn truncate(stream: &[u8], limit: OutputLimit, spill_to: Option<&Path>) -> Cut {
    if stream.len() <= limit.head + limit.tail {
        return Cut {
            text: String::from_utf8_lossy(stream).to_string(),
            spill: None,
            truncated: false,
        };
    }

    let head_end = boundary_at_or_before(stream, limit.head);
    let tail_start = boundary_at_or_after(stream, stream.len() - limit.tail);
    let head = String::from_utf8_lossy(&stream[..head_end]);
    let tail = String::from_utf8_lossy(&stream[tail_start..]);
    let omitted = tail_start.saturating_sub(head_end);
    let mut text = format!("{head}\n… {omitted} bytes omitted …\n{tail}");

    let mut spill = None;
    match spill_to {
        Some(path) => match write_spill(path, stream) {
            Ok(path) => {
                text.push_str(&format!("\nfull output: {}", path.display()));
                spill = Some(path);
            }
            Err(error) => {
                text.push_str(&format!("\nfull output could not be saved: {error}"));
            }
        },
        None => text.push_str("\nfull output was not kept: no spill directory"),
    }

    Cut {
        text,
        spill,
        truncated: true,
    }
}

fn write_spill(path: &Path, stream: &[u8]) -> io::Result<PathBuf> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, stream)?;
    Ok(path.to_path_buf())
}

/// The nearest UTF-8 boundary at or before `index`, so a multi-byte character is
/// never split in half.
fn boundary_at_or_before(bytes: &[u8], index: usize) -> usize {
    let mut at = index.min(bytes.len());
    while at > 0 && !is_boundary(bytes, at) {
        at -= 1;
    }
    at
}

/// The nearest UTF-8 boundary at or after `index`.
fn boundary_at_or_after(bytes: &[u8], index: usize) -> usize {
    let mut at = index.min(bytes.len());
    while at < bytes.len() && !is_boundary(bytes, at) {
        at += 1;
    }
    at
}

fn is_boundary(bytes: &[u8], index: usize) -> bool {
    index == 0 || index >= bytes.len() || (bytes[index] & 0xC0) != 0x80
}
