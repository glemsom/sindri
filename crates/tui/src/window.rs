//! The window onto the transcript: where the reader is in the thread, what
//! moves it, and how much of the thread arrived below them while they were
//! reading. The place is a row of an entry and not an offset from the tail, so
//! neither a run that appends nor a call that opens above them moves the text
//! under a reader who has scrolled back; the tail is a place the window takes
//! again on `End`.

/// What the reader asked of the window, for the next frame to carry out. The
/// keys know the rows a page is worth and neither the entry they land on nor
/// how far the thread runs, so they ask and the window answers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Ask {
    #[default]
    None,
    /// Rows to move the window's top by, positive for back into the thread.
    Rows(isize),
    /// The first turn.
    Home,
    /// The tail, which is followed again.
    End,
    /// The entry the reader engaged with — `Enter` or a click on it — at its
    /// first row: a place taken rather than a page turned, and the frame that
    /// answers it is the one that stops following.
    Hold(usize),
}

/// The rows below a window's screen, and what they were counted from: the first
/// entry that was not laid out, and the thread's newest entry with its rows. A
/// run only ever appends at the tail, so what arrives is added to the count
/// rather than walked for again, which is what keeps a page a page on a long
/// thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Below {
    pub width: u16,
    pub first: usize,
    pub newest: usize,
    pub rows: usize,
}

/// Where the window is. `at` is the entry the window's top row belongs to and
/// how many rows into that entry's block it sits — its gap rows and its own;
/// `follow` says whether the tail drags the window with it.
#[derive(Debug, Clone)]
pub struct Window {
    pub(crate) at: (usize, usize),
    pub(crate) follow: bool,
    /// The rows between the window's bottom edge and the tail, as the last
    /// frame laid them out.
    pub(crate) scroll: usize,
    /// The rows that were below the window when the reader last went back into
    /// the thread: what arrived under them since is what is new.
    pub(crate) new_below: usize,
    /// The rows below the screen, and what they were counted from.
    pub(crate) below: Option<(Below, usize)>,
    /// What the reader asked of the window since the last frame.
    pub(crate) ask: Ask,
    /// The entry the reader engaged with, whose place is theirs until a key
    /// takes the tail. It is what tells a column the thread is too short to
    /// fill that the newest rows are not where the reader wants to be.
    hold: Option<usize>,
    /// Whether the thread changed under the window since the last frame, so
    /// what the badge counted no longer means anything.
    recount: bool,
    /// The rows the place has still to move, in `Ask::Rows`' sign: what a
    /// toggle that changed a block's height owes a reader whose top row sat
    /// inside it. Not an ask: an ask is the reader moving and is counted
    /// against the badge, while a row that left is not a row that arrived.
    moved: isize,
    /// The rows one page is worth to whatever has the screen. The transcript
    /// reports its own every frame; an open overlay reports what its box
    /// showed instead, so `PgUp`/`PgDn` move the box that covers the frame and
    /// never the frame beneath it.
    pub(crate) page: usize,
}

impl Default for Window {
    fn default() -> Self {
        Window {
            at: (0, 0),
            follow: true,
            scroll: 0,
            new_below: 0,
            below: None,
            ask: Ask::None,
            hold: None,
            recount: false,
            moved: 0,
            page: crate::app::DEFAULT_PAGE,
        }
    }
}

impl Window {
    /// The rows between the window's bottom edge and the tail: nothing while
    /// the reader is on it.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// The rows that arrived below the reader since they last looked there:
    /// nothing while they are on the tail, which is why the badge goes when
    /// they return to it.
    pub fn new_rows(&self) -> usize {
        self.scroll.saturating_sub(self.new_below.min(self.scroll))
    }

    /// Asks for something the next frame carries out.
    pub fn want(&mut self, ask: Ask) {
        self.ask = match (self.ask, ask) {
            (Ask::Rows(asked), Ask::Rows(rows)) => Ask::Rows(asked.saturating_add(rows)),
            _ => ask,
        };
    }

    /// Moves the place by `delta` rows, in `Ask::Rows`' sign: positive is back
    /// into the thread and negative is forward, across as few entries as it
    /// crosses. The window holds no entries and cannot say what a row is worth,
    /// so the move is `layout::back`'s and `layout::forward`'s, and the frame
    /// takes it on its way to the next layout.
    ///
    /// A second shift before that layout replaces the first rather than adding
    /// to it: both were worked out from the same place, so adding them would
    /// count the same rows twice.
    pub fn shift(&mut self, delta: isize) {
        self.moved = delta;
    }

    /// The rows the place owes the last frame's change, taken once.
    pub(crate) fn take_shift(&mut self) -> isize {
        std::mem::take(&mut self.moved)
    }

    /// Whether the frame answering `ask` is on a place the reader engaged with
    /// rather than one they scrolled back from: a hold already settled, or the
    /// one this frame was asked for.
    pub(crate) fn engaged(&self, ask: Ask) -> bool {
        self.hold.is_some() || matches!(ask, Ask::Hold(_))
    }

    /// The thread changed somewhere other than its tail — an entry opened, an
    /// entry taken out — so the rows below the screen are no longer the rows
    /// that were counted, and the badge's count starts again from them.
    pub fn forget(&mut self) {
        self.below = None;
        self.recount = true;
    }

    /// The first turn, and the tail followed again: the two ends of the thread.
    pub fn to_top(&mut self) {
        self.want(Ask::Home);
    }

    pub fn to_tail(&mut self) {
        self.want(Ask::End);
    }

    /// Takes what was asked for, once: a frame carries it out and the next one
    /// owes the reader nothing.
    pub fn take_ask(&mut self) -> Ask {
        std::mem::take(&mut self.ask)
    }

    /// What the frame laid out: the window's top, whether it follows the tail,
    /// the rows below it, and the rows one page is worth. The badge is counted
    /// here rather than in the renderer, because what is new is a fact about
    /// what arrived and not about where the window is.
    pub fn settled(
        &mut self,
        ask: Ask,
        at: (usize, usize),
        follow: bool,
        scroll: usize,
        page: usize,
    ) {
        self.at = at;
        self.follow = follow;
        self.scroll = scroll;
        self.page = page.max(1);
        // Going back into the thread takes the rows below the reader as read,
        // and reaching the top takes every row below it as read. Coming back
        // down reads them one for one, so the count falls with the window and
        // what is left is what is still new.
        self.new_below = match ask {
            // The first turn, and an engagement: the rows below the reader are
            // taken as read, so the count starts at nothing and only what
            // arrives after them is new.
            Ask::Home | Ask::Hold(_) => scroll,
            Ask::Rows(rows) if rows > 0 => self.new_below.saturating_add(rows.unsigned_abs()),
            Ask::Rows(rows) => self.new_below.saturating_sub(rows.unsigned_abs()),
            Ask::End | Ask::None => self.new_below,
        };
        if scroll == 0 {
            self.new_below = 0;
        }
        // The thread changed under the window: what is below it now was not
        // under it when the reader last looked, so it is neither new nor read,
        // and the count starts again from what is there.
        if std::mem::take(&mut self.recount) {
            self.new_below = scroll;
        }
        self.hold = match ask {
            Ask::Hold(entry) => Some(entry),
            _ if follow => None,
            _ => self.hold,
        };
    }
}

impl crate::app::App {
    /// Where the reader is in the thread: the rows between the window's bottom
    /// edge and its tail.
    pub fn scroll(&self) -> usize {
        self.window.scroll()
    }
}

#[cfg(test)]
mod tests {
    use super::Ask;
    use crate::Launch;
    use crate::app::App;
    use crate::render::transcript;

    /// A thread of twenty notes, one row each at the width they are laid out
    /// at, and a window short enough to be held somewhere in it.
    fn thread() -> App {
        let mut app = App::new(
            Launch {
                session: "4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77".to_string(),
                temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
                branch: None,
                sandbox: "bwrap · 1 writable".to_string(),
                provider: "anthropic".to_string(),
                model: "claude-sonnet-4-6".to_string(),
                workspace: "~/git/sindri".to_string(),
                max_turns: 250,
                context_limit: None,
            },
            None,
        );
        for note in 1..=20 {
            app.note_info(format!("note {note}"));
        }
        app
    }

    /// The tail, then a page back: a window that holds a place of its own.
    fn a_page_back() -> App {
        let mut app = thread();
        transcript::window(&mut app, 40, 5);
        app.window.want(Ask::Rows(6));
        transcript::window(&mut app, 40, 5);
        app
    }

    #[test]
    fn a_shift_moves_the_place_by_rows_and_is_not_a_reader_who_moved() {
        let mut app = a_page_back();
        let (entry, row) = app.window.at;
        let counted = app.window.new_below;
        assert!(entry > 2, "the window is off the tail: it holds {entry}");

        // Negative is forward, towards the tail: two one-row entries.
        app.window.shift(-2);
        transcript::window(&mut app, 40, 5);
        assert_eq!(app.window.at, (entry + 2, row), "the place moved forward");
        assert_eq!(
            app.window.new_below, counted,
            "and counted nothing: a shift is the thread moving, not the reader"
        );

        app.window.shift(2);
        transcript::window(&mut app, 40, 5);
        assert_eq!(
            app.window.at,
            (entry, row),
            "positive is back into the thread, and the place is where it was"
        );
    }

    #[test]
    fn a_shift_past_the_newest_row_leaves_the_place_alone() {
        let mut app = a_page_back();
        let at = app.window.at;

        app.window.shift(-400);
        transcript::window(&mut app, 40, 5);
        assert_eq!(
            app.window.at, at,
            "past the newest row is the tail, and the tail is `Ask::End`'s"
        );
        assert!(!app.window.follow, "so the window is not following");
    }
}
