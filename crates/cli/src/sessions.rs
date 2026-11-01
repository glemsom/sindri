//! `sindri sessions`: the records a run left behind. `list` is one row per
//! record, newest first; `show` is one record whole. Reading only — nothing
//! here writes, deletes or prunes.

use sindri_core::clock;
use sindri_core::paths::state_dir;
use sindri_core::provider::Usage;
use sindri_core::record::{self, ShowError, Stored, Summary};
use sindri_types::ExitCode;

/// The one field a row can run long with. A subcommand printing a table does
/// not ask the terminal how wide it is: the prompt is clipped here, and
/// `--json` and `sessions show` hold the whole of it.
const PROMPT_COLUMNS: usize = 60;

/// Prints the records, newest first, or nothing when the state directory holds
/// none: an empty list is not a failure.
pub fn list(limit: Option<usize>, json: bool) -> i32 {
    let root = match state_dir() {
        Ok(state) => state.join("sessions"),
        Err(error) => return unreadable(&error.to_string()),
    };
    let rows = match record::listing_limited(&root, limit) {
        Ok(rows) => rows,
        Err(error) => {
            return unreadable(&format!("{} cannot be read: {error}", root.display()));
        }
    };
    let mut out = String::new();
    for row in &rows {
        // A row serializes itself, in the order the text row prints, so `jq`
        // streams one object per line and neither shape can drift from the
        // other. Every field is a string or a number, so this cannot refuse;
        // `record::json` leans on the same.
        if json {
            out.push_str(&serde_json::to_string(row).unwrap_or_default());
        } else {
            out.push_str(&line(row));
        }
        out.push('\n');
    }
    print!("{out}");
    ExitCode::Success.code()
}

/// The state directory is the one thing a listing cannot do without, and a
/// caller that asked to read it is owed the reason rather than an empty list.
fn unreadable(what: &str) -> i32 {
    eprintln!("sindri: sessions: {what}");
    ExitCode::EnvironmentFailure.code()
}

/// Prints one session's transcript — the newest record when no id is named, the
/// one a whole id or a prefix names otherwise, as text or as the files' own
/// JSON. A wrong or ambiguous id is a usage error; a record that cannot be read
/// is the environment's failure, not the user's.
pub fn show(id: Option<&str>, json: bool) -> i32 {
    match Stored::resolve(id) {
        Ok(stored) => {
            if json {
                println!("{}", stored.json());
            } else {
                print!("{}", stored.render());
            }
            ExitCode::Success.code()
        }
        Err(error) => {
            eprintln!("sindri: sessions: {error}");
            match error {
                ShowError::BadId { .. }
                | ShowError::Ambiguous { .. }
                | ShowError::Missing { .. } => ExitCode::UsageError.code(),
                ShowError::Unreadable { .. } | ShowError::Corrupt { .. } => {
                    ExitCode::EnvironmentFailure.code()
                }
            }
        }
    }
}

/// The whole record as a row: the id a listing hands `show`, unfixed-width and
/// whole because a prefix would need uniqueness checked against the rows beside
/// it, then what a record is chosen by.
fn line(row: &Summary) -> String {
    format!(
        "{}  {}  {}  {}  {}  {}  {}",
        row.id,
        clock::at(row.started_ms),
        row.model,
        turns(row.turns),
        row.stopped,
        tokens(row.usage.as_ref()),
        prompt(row.prompt.as_deref()),
    )
}

fn turns(turns: Option<u32>) -> String {
    match turns {
        Some(1) => "1 turn".to_string(),
        Some(turns) => format!("{turns} turns"),
        // A run that never finished counted no turns: the record has none, and
        // a row that printed a zero would be inventing one.
        None => "— turns".to_string(),
    }
}

/// The run's own total. Only `total_tokens` states it — a sum of two counters
/// is arithmetic over what a provider reported, not the bill — so a run that
/// never reported one says so with a dash, and `--json` still carries every
/// counter the record holds.
fn tokens(usage: Option<&Usage>) -> String {
    match usage.and_then(|usage| usage.total_tokens) {
        Some(total) => format!("{total} tokens"),
        None => "— tokens".to_string(),
    }
}

/// The first thing the run was asked, on one line and in one column's width.
fn prompt(prompt: Option<&str>) -> String {
    let Some(prompt) = prompt else {
        return "—".to_string();
    };
    let text = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= PROMPT_COLUMNS {
        return text;
    }
    let mut clipped: String = text.chars().take(PROMPT_COLUMNS - 1).collect();
    clipped.push('…');
    clipped
}
