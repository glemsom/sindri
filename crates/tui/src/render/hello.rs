//! The first frame: a thread with nothing in it yet is not a blank page. It
//! says what the run is — the wire, the model, the directory and the sandbox —
//! the one sentence of the contract that matters, and three things that can be
//! typed as they read, so the first keystroke is not a guess (issue #46).
//!
//! Every fact here comes from the launch, and nothing else does: an empty
//! frame can say what it was launched with, and cannot say what a run it has
//! not had has spent.

use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::markdown::text;
use crate::rail::SEPARATOR;
use crate::render::rail::chip;
use crate::render::transcript::plain;
use crate::theme;

/// The contract in one sentence: the tool there is, and the boundary it runs
/// behind — readable everywhere, writable only where the policy says
/// (ADR-0002, ADR-0010). It is a fact of the launch and not a promise: the
/// doctor proves the sandbox before the first frame, and a run whose bwrap
/// cannot start draws a card instead of this (ADR-0011 §15).
pub const CONTRACT: &str =
    "one tool, bash, sandboxed: the host is readable, only the chosen paths writable";

/// Three things to type, each one accepted exactly as it is written: two
/// prompts the loop sends whole, and a command the frame runs. `HELP` is
/// offered beside them, because the rest are on it.
pub const EXAMPLES: [&str; 3] = [
    "what changed here today?",
    "run the tests and fix what fails",
    "/sessions",
];

pub const HELP: &str = "/help";

/// The first frame's rows. An empty thread has no window over it, so what it
/// draws is what it says.
pub fn rows(app: &App, width: u16) -> Vec<Line<'static>> {
    let width = width.max(1) as usize;
    let mut rows = vec![head(app, width)];
    rows.push(Line::default());
    rows.extend(text::wrap(
        text::glyphs(&[Span::styled(CONTRACT, theme::faint())]),
        "",
        theme::faint(),
        width,
    ));
    rows.push(Line::default());
    for line in EXAMPLES.into_iter().chain([HELP]) {
        rows.push(typed(line));
    }
    rows
}

/// The first line: what you are talking to, where, and what it is contained
/// by. The sandbox is named by its posture's head — the count of what it may
/// write is the rail's — because the row has three other facts to hold. The
/// directory is the one fact shortened as the row runs out: a wire or a model
/// half-said names something else, and the rail states the path whole.
fn head(app: &App, width: usize) -> Line<'static> {
    let launch = app.rail().launch();
    let posture = format!("⬢ {}", contains(&launch.sandbox));
    let fixed =
        "◆ sindri".width() + launch.provider.width() + launch.model.width() + 2 * GAP.width();
    let room = width.saturating_sub(fixed + 2 * GAP.width() + posture.width());
    let mut spans = vec![
        Span::styled("◆ sindri", theme::prompt()),
        Span::raw(GAP),
        Span::raw(launch.provider.clone()),
        Span::raw(GAP),
        Span::raw(launch.model.clone()),
        Span::raw(GAP),
    ];
    if room > 0 {
        spans.push(Span::styled(
            text::middle(&launch.workspace, room),
            theme::faint(),
        ));
        spans.push(Span::raw(GAP));
    }
    spans.push(Span::styled(posture, chip()));
    Line::from(spans)
}

/// What separates one fact from the next on the first line.
const GAP: &str = "  ";

/// A posture's head: `bwrap · 1 writable` is `bwrap` where the row has three
/// other facts to hold. The separator is the rail's, so the two readings of a
/// posture cannot drift apart.
fn contains(sandbox: &str) -> &str {
    sandbox.split(SEPARATOR).next().unwrap_or(sandbox)
}

/// One thing that can be typed, marked so it reads as an offer rather than as
/// prose the run wrote.
fn typed(offer: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("  ›  ", theme::faint()),
        Span::raw(offer.to_string()),
    ])
}

/// The rows as characters, for the frame's own copy and click bookkeeping.
pub fn plain_rows(rows: &[Line<'static>]) -> Vec<String> {
    rows.iter().map(plain).collect()
}
