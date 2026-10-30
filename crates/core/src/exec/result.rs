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
    Exited {
        /// The script's own exit status; when a signal killed it, 128 + the
        /// signal number, exactly as a shell would report it.
        exit_code: i32,
    },
    TimedOut,
    SandboxError {
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolResult {
    #[serde(flatten)]
    pub outcome: Outcome,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spill: Vec<PathBuf>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct OutputLimit {
    pub head: usize,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cut {
    pub text: String,
    pub spill: Option<PathBuf>,
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
