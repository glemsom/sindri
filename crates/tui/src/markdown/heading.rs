//! The heading scale: the shallower the level, the heavier the heading. H1 and
//! H2 are `HEADING` in bold over a rule beneath them, and the rule spans the
//! column divided by the level, so a heading's rank is a function of its level
//! and not a case somebody remembered to add.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme;

/// The levels heavy enough to be bold and to rule. Deeper headings are
/// `HEADING` alone: past H2 there is nothing left to take away.
const RULED: u8 = 2;

pub fn style(level: u8) -> Style {
    let style = Style::new().fg(theme::HEADING);
    if level <= RULED {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

/// The row under a heading, or nothing for a level that rules not at all. The
/// rule starts where the heading's own words do, so a heading in a quote rules
/// inside the quote and not across it.
pub fn rule(level: u8, prefix: &str, width: usize) -> Option<Line<'static>> {
    if level == 0 || level > RULED {
        return None;
    }
    let columns = (width / level as usize).saturating_sub(prefix.chars().count());
    (columns > 0).then(|| {
        Line::from(Span::styled(
            format!("{prefix}{}", "─".repeat(columns)),
            theme::faint(),
        ))
    })
}
