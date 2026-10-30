//! The window onto the transcript: where the reader is in the thread, what
//! moves it, and how much of the thread arrived below them while they were
//! reading. The place is a row of an entry and not an offset from the tail, so
//! a run that appends never moves the text under a reader who has scrolled
//! back; the tail is a place the window takes again on `End`.

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

    /// The thread changed somewhere other than its tail — an entry opened, an
    /// entry taken out — so the rows below the screen are no longer the rows
    /// that were counted.
    pub fn forget(&mut self) {
        self.below = None;
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
            Ask::Home => scroll,
            Ask::Rows(rows) if rows > 0 => self.new_below.saturating_add(rows.unsigned_abs()),
            Ask::Rows(rows) => self.new_below.saturating_sub(rows.unsigned_abs()),
            Ask::End | Ask::None => self.new_below,
        };
        if scroll == 0 {
            self.new_below = 0;
        }
    }
}

impl crate::app::App {
    /// Where the reader is in the thread: the rows between the window's bottom
    /// edge and its tail.
    pub fn scroll(&self) -> usize {
        self.window.scroll()
    }
}
