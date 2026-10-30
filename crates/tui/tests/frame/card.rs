//! The card: the doctor's lines each on their own row, a block sized from the
//! rows they wrapped into and paged when the screen is shorter than they are,
//! the key that leaves and the command that checks again said in the card, and
//! a resize that lays it out again rather than dismissing it unread (issue #50).

use super::*;

use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};
use sindri_tui::Card;
use sindri_tui::card::{Answer, View};

/// A card the way `bwrap` gives one: three lines, the middle one long enough
/// that a fixed six-row block cut it in half.
fn card() -> Card {
    Card {
        title: "sindri cannot start: the sandbox does not work here".to_string(),
        check: "bwrap cannot build a namespace".to_string(),
        detail: "Permission denied\n\
                 bwrap: Creating new namespace failed: Operation not permitted\n\
                 bwrap: No permissions to creating new namespace: Operation not permitted\n\
                 the kernel refuses the user namespaces bubblewrap needs to build a sandbox with no root, so nothing inside it would have been isolated anyway\n\
                 allow them with sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0 and run sindri doctor to see every check pass again\n\
                 without them every command runs on your own machine with your own privileges, which is what the sandbox exists to prevent"
            .to_string(),
    }
}

fn screen(width: u16, height: u16) -> Screen {
    Screen {
        terminal: Terminal::new(TestBackend::new(width, height)).expect("a test terminal"),
        view: View::default(),
    }
}

/// One card drawn on one terminal, across however many sizes and frames a test
/// puts it through: the view outlives a resize, which is the point.
struct Screen {
    terminal: Terminal<TestBackend>,
    view: View,
}

impl Screen {
    fn draw(&mut self, card: &Card) {
        let Screen { terminal, view } = self;
        terminal
            .draw(|frame| render::draw_card(card, view, frame))
            .expect("the card draws");
    }

    fn resize(&mut self, width: u16, height: u16) {
        self.terminal.backend_mut().resize(width, height);
    }

    fn rows(&self) -> Vec<String> {
        let buffer = self.terminal.backend().buffer();
        let area = buffer.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    fn shown(&self) -> String {
        self.rows().join("\n")
    }

    /// Every word the card put inside its border, paged from wherever the
    /// reader is to the bottom: what a page does not show is on the next one.
    fn paged(&mut self, card: &Card) -> Vec<String> {
        let mut rows = Vec::new();
        let mut pages = 0;
        loop {
            rows.extend(inside(&self.rows()));
            if !self.view.below() {
                return rows;
            }
            self.view.page(false);
            self.draw(card);
            pages += 1;
            assert!(pages < 20, "the card reaches its last page: {rows:#?}");
        }
    }
}

/// The rows the card wrote inside its border, without the border itself: the
/// card's words, and nothing the box drew around them.
fn inside(rows: &[String]) -> Vec<String> {
    rows.iter()
        .filter(|row| row.starts_with('│'))
        .map(|row| {
            row.trim_matches(|glyph| glyph == '│' || glyph == ' ')
                .to_string()
        })
        .collect()
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_string).collect()
}

fn key(code: KeyCode) -> TerminalEvent {
    TerminalEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn the_doctors_three_lines_draw_as_three_lines() {
    let card = card();
    let mut screen = screen(100, 24);
    screen.draw(&card);
    let rows = screen.rows();
    let row_of = |needle: &str| rows.iter().position(|row| row.contains(needle));
    let denied = row_of("Permission denied").expect("the first of the doctor's lines is drawn");
    let failed = row_of("bwrap: Creating new namespace failed")
        .expect("the second of the doctor's lines is drawn");
    let lambda = row_of("the kernel refuses").expect("the third of the doctor's lines is drawn");
    assert!(
        denied != failed && failed != lambda && denied != lambda,
        "each line has a row of its own: {rows:#?}"
    );
}

#[test]
fn at_a_hundred_by_twentyfour_the_card_is_whole_and_marked_by_nothing() {
    let card = card();
    let mut screen = screen(100, 24);
    screen.draw(&card);
    assert!(
        screen.view.below().eq(&false) && !screen.view.above(),
        "nothing is hidden at 100×24: {:?}",
        screen.view
    );
    let shown = screen.shown();
    assert!(
        shown.contains("exists to prevent"),
        "the description's last words are on the screen: {shown}"
    );
    assert!(
        !shown.contains("more"),
        "and the card does not claim it runs past the screen: {shown}"
    );
}

#[test]
fn at_seventy_by_sixteen_the_description_is_whole_and_scrollable() {
    let card = card();
    let mut screen = screen(70, 16);
    screen.draw(&card);
    let first = screen.shown();
    assert!(
        first.contains("↓ more"),
        "the card says it runs past the bottom: {first}"
    );
    assert!(
        !first.contains("exists to prevent"),
        "and the description's last words are below the fold: {first}"
    );
    let whole = screen.paged(&card);
    // Every word the doctor wrote is on some page, in the order it was
    // written: a row that ran out of box wrapped, it did not lose the rest of
    // the sentence.
    let mut drawn = whole
        .iter()
        .flat_map(|row| words(row))
        .collect::<Vec<_>>()
        .into_iter();
    for wanted in words(&card.detail) {
        assert!(
            drawn.any(|word| word == wanted),
            "the card draws {wanted:?} whole, never cut: {whole:#?}"
        );
    }
    let last = screen.shown();
    assert!(
        last.contains("exists to prevent"),
        "the last page has the bottom of the report: {last}"
    );
    assert!(
        !last.contains("↓ more") && last.contains("↑ more"),
        "and says the way back up is there: {last}"
    );
}

#[test]
fn the_card_names_the_key_that_leaves_and_the_command_that_checks_again() {
    let mut card = card();
    card.detail = "bwrap is not installed".to_string();
    let mut screen = screen(100, 24);
    screen.draw(&card);
    let shown = screen.shown();
    assert!(
        shown.contains("any key leaves"),
        "the card says how to leave it: {shown}"
    );
    assert!(
        shown.contains("sindri doctor"),
        "and what to run to see the checks again: {shown}"
    );
}

#[test]
fn a_resize_lays_the_card_out_again_and_a_key_leaves_it() {
    let card = card();
    let mut screen = screen(100, 24);
    screen.draw(&card);
    assert!(!screen.shown().contains("↓ more"), "the card fits at first");
    // The same card on a smaller screen, as a `tmux` attach hands it one: laid
    // out again at 70×16, not dismissed before it was read.
    screen.resize(70, 16);
    screen.draw(&card);
    let narrow = screen.shown();
    assert!(
        narrow.contains("sindri cannot start"),
        "the card is still there: {narrow}"
    );
    assert!(
        narrow.contains("↓ more"),
        "and re-laid out at the size it now has: {narrow}"
    );
    assert!(
        narrow.contains("PgUp/PgDn scroll"),
        "the hint grows the page keys the card now needs: {narrow}"
    );
}

#[test]
fn only_a_key_leaves_the_card() {
    assert_eq!(
        sindri_tui::card::answer(&key(KeyCode::Char('q')), false),
        Answer::Leave
    );
    assert_eq!(
        sindri_tui::card::answer(&key(KeyCode::Esc), false),
        Answer::Leave
    );
    assert_eq!(
        sindri_tui::card::answer(&key(KeyCode::Enter), false),
        Answer::Leave
    );
    assert_eq!(
        sindri_tui::card::answer(&key(KeyCode::PageDown), true),
        Answer::Page { up: false },
        "a page key pages what is longer than the screen"
    );
    assert_eq!(
        sindri_tui::card::answer(&key(KeyCode::PageUp), true),
        Answer::Page { up: true }
    );
    assert_eq!(
        sindri_tui::card::answer(&key(KeyCode::PageDown), false),
        Answer::Leave,
        "a card with nothing below it is left by every key, not paged forever"
    );
    assert_eq!(
        sindri_tui::card::answer(&TerminalEvent::Resize(70, 16), true),
        Answer::Redraw,
        "a resize is not a key"
    );
    assert_eq!(
        sindri_tui::card::answer(&TerminalEvent::FocusGained, true),
        Answer::Redraw
    );
    assert_eq!(
        sindri_tui::card::answer(
            &TerminalEvent::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: ratatui::crossterm::event::MouseEventKind::Down(
                    ratatui::crossterm::event::MouseButton::Left
                ),
                column: 3,
                row: 3,
                modifiers: KeyModifiers::NONE,
            }),
            true
        ),
        Answer::Redraw,
        "the card has nothing to click"
    );
}
