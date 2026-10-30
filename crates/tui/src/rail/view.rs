//! The rows the rail last drew, kept so a click on one copies what was drawn
//! there: the rail's facts as a reader sees them, and where the session's own
//! row sits among them (ADR-0006, issue #51).

use ratatui::layout::Rect;

#[derive(Debug, Clone, Default)]
pub struct RailView {
    pub area: Rect,
    pub rows: Vec<String>,
    /// Where the session fact sits in `rows`.
    pub session: std::ops::Range<usize>,
}

impl RailView {
    pub fn row_at(&self, at: ratatui::layout::Position) -> Option<usize> {
        self.area
            .contains(at)
            .then(|| (at.y - self.area.y) as usize)
            .filter(|row| *row < self.rows.len())
    }
}
