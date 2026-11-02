//! `sindri sessions`: the records a run left behind. `list` is one row per
//! record, newest first; `show` is one record whole. Reading only — nothing
//! here writes, deletes or prunes.

use sindri_core::clock;
use sindri_core::paths::state_dir;
use sindri_core::provider::Usage;
use sindri_core::record::{self, ShowError, Stored, Summary};
use sindri_tui::markdown::text::clip;
use sindri_types::ExitCode;

/// The one field a row can run long with. A subcommand printing a table does
/// not ask the terminal how wide it is: the prompt is held to a column of cells,
/// cut by `sindri_tui::markdown::text::clip` so that a wide glyph and a
/// combining mark are answered the one way for the whole tree, and so a prompt
/// written in wide glyphs cannot push the columns after it out of line.
/// `--json` and `sessions show` hold the whole of it.
const PROMPT_CELLS: usize = 60;

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
    // A text that survives the whole column is the text, and nothing was cut. A
    // text that does not stops one cell short of the column: the ellipsis is a
    // column of its own, and a glyph is whole or it is not there, so it may stop
    // further.
    let head = clip(&text, PROMPT_CELLS);
    if head == text {
        return text;
    }
    format!("{}…", clip(&head, PROMPT_CELLS - 1))
}

#[cfg(test)]
mod tests {
    use super::{PROMPT_CELLS, prompt};

    #[test]
    fn a_prompt_inside_its_column_is_unchanged() {
        let fits = "x".repeat(PROMPT_CELLS);
        assert_eq!(prompt(Some(&fits)), fits, "the last cell is still inside");
    }

    #[test]
    fn a_prompt_past_its_column_ends_in_an_ellipsis() {
        let long = "x".repeat(PROMPT_CELLS + 1);
        let shown = format!("{}…", "x".repeat(PROMPT_CELLS - 1));
        assert_eq!(prompt(Some(&long)), shown, "the ellipsis takes a cell");
    }

    #[test]
    fn a_wide_glyph_is_whole_or_it_is_absent() {
        // Two columns a glyph, so a column of 60 holds twenty-nine of them and
        // the ellipsis: the thirtieth is dropped rather than cut in half.
        let wide = "日".repeat(PROMPT_CELLS);
        let shown = format!("{}…", "日".repeat(PROMPT_CELLS / 2 - 1));
        assert_eq!(prompt(Some(&wide)), shown);
    }

    #[test]
    fn a_combining_mark_is_no_column_of_its_own() {
        // Sixty base glyphs of one cell each, each carrying a zero-width mark:
        // a count of characters would call this a hundred and twenty cells.
        let marked = "e\u{301}".repeat(PROMPT_CELLS);
        assert_eq!(prompt(Some(&marked)), marked);
    }

    #[test]
    fn a_prompt_lands_on_one_line_whatever_was_typed() {
        assert_eq!(prompt(Some("  how\nmany\t files? ")), "how many files?");
    }

    #[test]
    fn a_record_with_no_prompt_shows_a_dash() {
        assert_eq!(prompt(None), "—");
    }
}
