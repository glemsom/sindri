//! Reading bwrap's status channel. bwrap writes a child-pid document before the
//! mounts and an exit-code document once the command finished, so the *absence*
//! of the exit-code document means the sandbox never started — never that the
//! command failed.

pub fn exit_code(docs: &[u8]) -> Option<i32> {
    let text = String::from_utf8_lossy(docs);
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|doc| i32::try_from(doc.get("exit-code")?.as_i64()?).ok())
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

#[cfg(test)]
mod tests {
    use super::exit_code;

    #[test]
    fn an_exit_code_too_large_to_hold_is_no_exit_code() {
        // 4294967297 wraps to 1 as an i32, so a document that never named a real
        // code would be reported as a command that exited 1.
        assert_eq!(exit_code(b"{\"exit-code\":4294967297}\n"), None);
    }

    #[test]
    fn an_exit_code_small_enough_to_hold_is_read_as_itself() {
        assert_eq!(exit_code(b"{\"exit-code\":7}\n"), Some(7));
    }
}
