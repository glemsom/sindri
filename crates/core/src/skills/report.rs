//! The catalog as `sindri doctor` says it: one line per skill, one per refusal,
//! one per shadowed name. A report of nothing found is a line and not a silence.

use unicode_width::UnicodeWidthStr;

use super::catalog::{Catalog, Skill};

impl Catalog {
    /// The catalog as text, ending with a newline so it composes with the
    /// description a front end prints beside it.
    pub fn report(&self) -> String {
        let mut out = self.found();
        out.push_str(&self.refused());
        out.push_str(&self.shadowed());
        out
    }

    /// Every skill, in columns wide enough for the widest value in each, so a
    /// reader compares scopes and invocation rights down the report.
    fn found(&self) -> String {
        if self.skills.is_empty() {
            return "skills: none found\n".to_string();
        }
        let rows: Vec<Row> = self.skills.iter().map(Row::of).collect();
        let (scope, name, invocation) = (
            widest(rows.iter().map(|row| row.scope.as_str())),
            widest(rows.iter().map(|row| row.name.as_str())),
            widest(rows.iter().map(|row| row.invocation.as_str())),
        );
        let mut out = format!("skills: {} found\n", self.skills.len());
        for row in &rows {
            out.push_str(&format!(
                "  {:<scope$}  {:<name$}  {:<invocation$}  {}\n",
                row.scope, row.name, row.invocation, row.at,
            ));
        }
        out
    }

    /// Every skill that would not load, with where it came from and the line
    /// that broke it where there is one: a file, or the scope that carries a
    /// built-in, which has none.
    fn refused(&self) -> String {
        let mut out = heading("refused", self.refused.len());
        for refusal in &self.refused {
            let at = match refusal.line {
                Some(line) => format!("{}:{line}", refusal.at),
                None => refusal.at.to_string(),
            };
            out.push_str(&format!("  {at}: {}\n", refusal.reason));
        }
        out
    }

    /// Every name a nearer scope took, and the two origins a reader needs: the
    /// skill that holds the name, and the one it was taken from. A built-in has
    /// no directory to print, so it is named by its scope's own word.
    fn shadowed(&self) -> String {
        let mut out = heading("shadowed", self.shadowed.len());
        for shadow in &self.shadowed {
            out.push_str(&format!(
                "  {}: {} shadows {}\n",
                shadow.winner.name, shadow.winner, shadow.loser
            ));
        }
        out
    }
}

/// The line that opens one of the sections after the skills themselves, and
/// nothing at all when there is nothing to open it over: a section that is
/// always there says nothing.
fn heading(section: &str, count: usize) -> String {
    match count {
        0 => String::new(),
        count => format!("skills: {count} {section}\n"),
    }
}

/// One skill as the report says it: the four values a line carries, held
/// together so the columns can be measured before any of them is printed.
struct Row {
    scope: String,
    name: String,
    invocation: String,
    at: String,
}

impl Row {
    fn of(skill: &Skill) -> Self {
        Row {
            scope: skill.scope.as_str().to_string(),
            name: skill.name.clone(),
            invocation: skill.invocation(),
            // A built-in is compiled in. A path in that column would be one a
            // reader went looking for, and there is no file to find.
            at: match skill.dir() {
                Some(dir) => dir.display().to_string(),
                None => "(compiled in)".to_string(),
            },
        }
    }
}

/// The width the column needs: the widest value in cells rather than in bytes,
/// because a name is a name and `café` is four columns wide.
fn widest<'a>(words: impl Iterator<Item = &'a str>) -> usize {
    words.map(UnicodeWidthStr::width).max().unwrap_or(0)
}
