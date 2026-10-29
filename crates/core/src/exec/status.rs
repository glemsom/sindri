//! Reading bwrap's status channel. bwrap writes a child-pid document before the
//! mounts and an exit-code document once the command finished, so the *absence*
//! of the exit-code document means the sandbox never started — never that the
//! command failed (ADR-0007).

/// The exit code bwrap reported, when the mounts succeeded and the command ran.
pub fn exit_code(docs: &[u8]) -> Option<i32> {
    let text = String::from_utf8_lossy(docs);
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|doc| doc.get("exit-code")?.as_i64().map(|code| code as i32))
}

/// The shell's own reading of a status: the code, or 128 + the signal.
pub fn exit_status_code(status: &std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// The first non-empty line of an error stream, for the one-line detail the
/// model needs to tell a broken sandbox from a failed command.
pub fn first_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    if line.is_empty() {
        "the sandbox produced no status and no error".to_string()
    } else {
        line.to_string()
    }
}
