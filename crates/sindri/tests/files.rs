//! The `@` picker's contract with a reader, walked rather than eyeballed: the keys that open and close the
//! screen, the glyphs that are the query and the ones still the prompt's, and the word a selection leaves in
//! the draft. Every workspace is the test's own, so nothing of this repository decides what a picker offers.

use std::path::Path;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use sindri::composer::{Composer, Key, Outcome};
use sindri::ink::{Ink, Look};
use sindri::keys::Mode;
use sindri::palette::Palette;
use sindri::picker::Row;
use sindri::view::View;

fn workspace(files: &[&str]) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a temp workspace");
    for file in files {
        let path = root.path().join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the directories a file sits in");
        }
        std::fs::write(&path, "").expect("a file");
    }
    root
}

fn standing(root: &Path) -> View {
    let mut view = View::boot();
    view.set_workspace(root.to_path_buf());
    view
}

fn draft(view: &View) -> &str {
    view.composer().text()
}

/// A frame standing in `root` with `draft` typed through the keys a reader's fingers send.
fn typing(root: &Path, draft: &str) -> View {
    let mut composer = Composer::new();
    composer.paste(draft);
    let mut view = View::typing(composer);
    view.set_workspace(root.to_path_buf());
    view
}

#[test]
fn an_at_opens_the_files_of_the_workspace() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = standing(root.path());

    assert_eq!(
        view.key(Key::Char('@')),
        Outcome::Edited,
        "`@` is a screen opening, so there is a frame to paint"
    );
    assert_eq!(
        view.mode(),
        Mode::Files,
        "and the picker is the mode on top of the stack"
    );
    assert_eq!(view.key(Key::Escape), Outcome::Edited, "Esc closes it");
    assert_eq!(
        view.mode(),
        Mode::Composer,
        "and the draft has the keys again"
    );
}

#[test]
fn an_at_inside_a_word_is_a_glyph_and_not_a_screen() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "why does user@host");
    view.key(Key::Char('@'));

    assert_eq!(
        view.mode(),
        Mode::Composer,
        "an address holds an `@` that is nobody's key: only an `@` that starts a word opens \
         the picker"
    );
    assert_eq!(draft(&view), "why does user@host@");
}

#[test]
fn an_at_after_a_space_walks_the_workspace_where_the_caret_is() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "read ");

    assert_eq!(view.key(Key::Char('@')), Outcome::Edited);
    assert_eq!(
        view.mode(),
        Mode::Files,
        "an `@` that opens a sentence opens the files: the draft is where a path is wanted"
    );
    assert_eq!(
        draft(&view),
        "read ",
        "and the `@` itself is the key, not a glyph"
    );
}

#[test]
fn a_picker_answers_its_own_keys_and_swallows_the_drafts() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "read ");
    view.key(Key::Char('@'));

    for key in [Key::Quit, Key::Cancel] {
        let before = draft(&view).to_owned();
        view.key(key);
        assert_eq!(
            draft(&view),
            before,
            "{key:?} at the picker leaves the draft under it alone"
        );
    }
    assert_eq!(view.mode(), Mode::Files, "and the screen is still on top");
}

#[test]
fn a_glyph_typed_at_the_picker_narrows_it_and_never_reaches_the_draft() {
    let root = workspace(&["rows.rs", "rail.rs"]);
    let mut view = typing(root.path(), "read ");
    view.key(Key::Char('@'));

    for glyph in "rail".chars() {
        assert_eq!(
            view.key(Key::Char(glyph)),
            Outcome::Edited,
            "each glyph of the query is a frame painted without it"
        );
    }

    assert_eq!(view.picker().query(), "rail");
    assert_eq!(
        view.picker().rows(),
        vec![Row {
            name: "rail.rs",
            directory: ""
        }],
        "the query narrows the rows as it is typed"
    );
    assert_eq!(
        draft(&view),
        "read ",
        "and not one glyph of it was the draft's"
    );
}

#[test]
fn selecting_a_file_puts_its_path_in_the_draft_and_closes_the_screen() {
    let root = workspace(&["rows.rs", "notes/a file.rs"]);
    let mut view = typing(root.path(), "read ");
    view.key(Key::Char('@'));

    assert_eq!(
        view.key(Key::Down),
        Outcome::Edited,
        "the second row is below the first"
    );
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Edited,
        "Enter puts the chosen path in the draft"
    );

    assert_eq!(
        draft(&view),
        "read 'notes/a file.rs' ",
        "the word a shell reads back as the one path, and the caret is left after it"
    );
    assert_eq!(
        view.mode(),
        Mode::Composer,
        "and the screen is gone: the reader is back in the draft it was written into"
    );
}

#[test]
fn a_path_chosen_in_the_middle_of_a_draft_lands_where_the_caret_is() {
    let root = workspace(&["rows.rs"]);
    let mut view = typing(root.path(), "read then say why");
    // The caret back to just after `read `, where a path goes in front of a word already written.
    for _ in 0.."then say why".len() {
        view.key(Key::Left);
    }

    view.key(Key::Char('@'));
    view.key(Key::Enter);

    assert_eq!(
        draft(&view),
        "read rows.rs then say why",
        "a picked path is inserted at the caret and never appended at the end"
    );
}

#[test]
fn a_query_that_matches_nothing_offers_nothing_and_selects_nothing() {
    let root = workspace(&["rows.rs"]);
    let mut view = typing(root.path(), "");
    view.key(Key::Char('@'));
    for glyph in "zzz".chars() {
        view.key(Key::Char(glyph));
    }

    assert!(view.picker().rows().is_empty());
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Unchanged,
        "there is no row to insert, so Enter changes nothing and the screen stays open to be \
         narrowed again"
    );
    assert_eq!(view.mode(), Mode::Files);
}

#[test]
fn a_frame_told_no_workspace_offers_no_file() {
    let mut view = View::boot();

    assert_eq!(
        view.key(Key::Char('@')),
        Outcome::Unchanged,
        "a frame that was told no directory has no files to offer, so nothing opens"
    );
    assert_eq!(view.mode(), Mode::Composer);
}

#[test]
fn escape_at_the_picker_leaves_the_draft_as_it_was() {
    let root = workspace(&["rows.rs"]);
    let mut view = typing(root.path(), "read ");
    view.key(Key::Char('@'));
    view.key(Key::Char('r'));

    assert_eq!(view.key(Key::Escape), Outcome::Edited);

    assert_eq!(
        draft(&view),
        "read ",
        "nothing of the picker reached the draft"
    );
    assert_eq!(view.mode(), Mode::Composer);
}

/// The frame `view` paints at `cols` columns and `rows` rows, read back cell by cell.
fn painted(view: &mut View, cols: u16, rows: u16) -> Buffer {
    let mut terminal =
        Terminal::new(TestBackend::new(cols, rows)).expect("a headless screen of that size");
    terminal
        .draw(|frame| view.paint(frame.area(), frame.buffer_mut()))
        .expect("the frame is painted");
    terminal.backend().buffer().clone()
}

fn look(buffer: &Buffer, palette: &Palette, x: u16, y: u16) -> Look {
    Look::of(buffer.cell((x, y)).expect("a cell of the frame"), palette)
        .expect("every cell of a frame is drawn in a look")
}

/// One row of the frame as the glyphs it draws, left to right from its first column.
fn row(buffer: &Buffer, y: u16) -> String {
    let area = buffer.area;
    (area.x..area.right())
        .map(|x| buffer.cell((x, y)).expect("a cell of the row").symbol())
        .collect()
}

/// The column of `said` where `needle` begins, counted in cells rather than bytes: a drawn row
/// holds glyphs wider than the one byte they are written in.
fn opens_at(said: &str, needle: &str) -> Option<usize> {
    let said: Vec<char> = said.chars().collect();
    let needle: Vec<char> = needle.chars().collect();
    (0..=said.len().saturating_sub(needle.len()))
        .find(|at| said[*at..*at + needle.len()] == needle[..])
}

/// Every row of the frame, top to bottom, as the glyphs each draws.
fn frame_rows(buffer: &Buffer) -> Vec<String> {
    (buffer.area.y..buffer.area.bottom())
        .map(|y| row(buffer, y))
        .collect()
}

/// The rows a screen's body is drawn in, each as the words it says: between a card's two borders,
/// or the whole of every row in a frame too narrow to be given one.
fn body_rows(buffer: &Buffer) -> Vec<String> {
    let rows = frame_rows(buffer);
    let (Some(top), Some(bottom)) = (
        rows.iter().position(|row| row.contains('╭')),
        rows.iter().rposition(|row| row.contains('╰')),
    ) else {
        return rows.into_iter().map(|row| row.trim().to_owned()).collect();
    };
    rows[top + 1..bottom]
        .iter()
        .map(|row| {
            let glyphs: Vec<char> = row.chars().collect();
            match (
                glyphs.iter().position(|glyph| *glyph == '│'),
                glyphs.iter().rposition(|glyph| *glyph == '│'),
            ) {
                (Some(left), Some(right)) => glyphs[left + 1..right].iter().collect(),
                _ => row.clone(),
            }
        })
        .map(|said| said.trim().to_owned())
        .collect()
}

#[test]
fn the_workspace_a_frame_walks_is_the_one_it_was_told() {
    let root = workspace(&["only-this.rs"]);
    let mut view = standing(root.path());
    view.key(Key::Char('@'));

    assert_eq!(
        view.picker().rows(),
        vec![Row {
            name: "only-this.rs",
            directory: ""
        }],
        "the walk is of the workspace the frame was handed, and of nothing else"
    );
}

/// The widths a card is drawn at: a card is at most seventy-two columns wide, so a wide frame gives
/// the body sixty-eight columns, fifty gives forty-six, and under fifty the card bleeds to the
/// frame and gives thirty-six.
const WIDTHS: [u16; 3] = [120, 50, 40];

#[test]
fn each_row_draws_the_files_name_and_the_directory_it_sits_in_at_every_width_that_holds_a_path() {
    let root = workspace(&["rows.rs", "src/view.rs", "docs/adr/note.md"]);
    let mut view = standing(root.path());
    view.key(Key::Char('@'));
    let palette = Palette::default();

    for cols in WIDTHS {
        let buffer = painted(&mut view, cols, 40);
        let rows = frame_rows(&buffer);
        for (name, directory) in [("rows.rs", ""), ("view.rs", "src"), ("note.md", "docs/adr")] {
            let y = buffer.area.y
                + u16::try_from(
                    rows.iter()
                        .position(|row| row.contains(name))
                        .unwrap_or_else(|| panic!("{name} is drawn on no row at {cols} columns")),
                )
                .expect("a row of the frame");
            let said = row(&buffer, y);
            let opens = opens_at(&said, name).expect("the name is drawn whole");
            let ends = opens + name.chars().count();
            for at in opens..ends {
                let x = buffer.area.x + u16::try_from(at).expect("a column of the frame");
                assert_ne!(
                    look(&buffer, &palette, x, y).ink(),
                    Ink::Dim,
                    "{name} is what a reader is reading, so it is drawn in the row's own ink \
                     at {cols} columns: {said:?}"
                );
            }
            let beside: String = said.chars().skip(ends).take(2 + directory.len()).collect();
            assert_eq!(
                beside,
                format!("  {directory}"),
                "and the directory it sits in follows it after two blanks, at {cols} \
                 columns: {said:?}"
            );
            for at in ends + 2..ends + 2 + directory.len() {
                let x = buffer.area.x + u16::try_from(at).expect("a column of the frame");
                assert_eq!(
                    look(&buffer, &palette, x, y).ink(),
                    Ink::Dim,
                    "the directory is the quieter half of the row, so it is dim at {cols} \
                     columns: {said:?}"
                );
            }
        }
    }
}

#[test]
fn a_name_too_wide_for_the_card_is_cut_before_its_directory_is_drawn() {
    let root = workspace(&[
        "a-file-name-that-is-wider-than-the-card-holds.md",
        "src/rows.rs",
    ]);
    let mut view = standing(root.path());
    view.key(Key::Char('@'));

    let buffer = painted(&mut view, 40, 40);
    let rows = frame_rows(&buffer);
    let said = rows
        .iter()
        .find(|row| row.contains("a-file-name"))
        .unwrap_or_else(|| panic!("the long name is drawn on no row: {rows:?}"))
        .trim()
        .to_owned();

    assert_eq!(
        said, "\u{203a} a-file-name-that-is-wider-than-the",
        "a row is cut at the card's right edge, and the name comes first, its own mark ahead of \
         it, so the name is what a narrow card keeps: half a directory with no name in front of \
         it names no file at all"
    );
}

#[test]
fn the_card_says_which_matches_it_shows_and_how_many_the_query_left() {
    let many: Vec<String> = (0..17).map(|at| format!("file{at:02}.rs")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    let root = workspace(&many);
    let mut view = standing(root.path());
    view.key(Key::Char('@'));

    let rows = body_rows(&painted(&mut view, 120, 40));
    assert!(
        rows.iter().any(|row| row == "1\u{2013}8 of 17"),
        "seventeen files answer an empty query and the card has the room to draw eight of \
         them, so the row under them says which eight and how many there are: {rows:?}"
    );

    let root = workspace(&["rows.rs", "src/view.rs", "docs/adr/note.md"]);
    let mut view = standing(root.path());
    view.key(Key::Char('@'));

    let rows = body_rows(&painted(&mut view, 120, 40));
    assert!(
        rows.iter().any(|row| row == "1\u{2013}3 of 3"),
        "three files answer, all three are drawn, and the count says so rather than leaving \
         a reader to wonder what the cap hid: {rows:?}"
    );
}

#[test]
fn a_query_that_matches_nothing_draws_one_line_saying_so() {
    let root = workspace(&["rows.rs", "rail.rs"]);
    let mut view = standing(root.path());
    view.key(Key::Char('@'));
    for glyph in "zzz".chars() {
        view.key(Key::Char(glyph));
    }

    let rows = body_rows(&painted(&mut view, 120, 40));
    assert_eq!(
        rows,
        vec![String::from("query: zzz"), String::from("no file answers")],
        "nothing holds `zzz`, so the body is the query and one line saying the list is empty: \
         no row of a file and no count of files there are none of"
    );
}

#[test]
fn a_band_too_short_for_the_picker_draws_the_rows_that_fit_and_keeps_the_rest_back() {
    let many: Vec<String> = (0..17).map(|at| format!("file{at:02}.rs")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    let root = workspace(&many);
    let mut view = standing(root.path());
    view.key(Key::Char('@'));

    let held = body_rows(&painted(&mut view, 120, 40));
    let short = painted(&mut view, 120, 11);
    let rows = frame_rows(&short);
    let top = rows
        .iter()
        .position(|row| row.contains('╭'))
        .expect("a card drawn with its top border");
    let bottom = rows
        .iter()
        .rposition(|row| row.contains('╰'))
        .expect("a card drawn with its bottom border");
    let border: Vec<char> = rows[top].chars().collect();
    let corner = |glyph: char| {
        border
            .iter()
            .position(|said| *said == glyph)
            .unwrap_or_else(|| panic!("the card's top border carries both its corners: {border:?}"))
    };
    let (left, right) = (corner('╭'), corner('╮'));
    for said in &rows[top + 1..bottom] {
        let held: Vec<char> = said.chars().collect();
        assert!(
            held[left] == '│' && held[right] == '│',
            "a row of the body is held between the card's own two sides, so no row of the picker \
             crosses the frame's edge: {said:?} in {rows:?}"
        );
    }

    let drawn = body_rows(&short);
    let (kept, count) = drawn.split_at(drawn.len().saturating_sub(1));
    assert!(
        kept.len() < held.len(),
        "a frame of eleven rows cannot hold the ten rows a frame of forty draws, and the card \
         takes one of the rows it has for the row that counts them: {kept:?} against {held:?}"
    );
    assert_eq!(
        kept,
        &held[..kept.len()],
        "the card too short for the picker draws the first rows of it in the same order and with \
         the same words, so a reader who can only see the head of the list sees the head of it: \
         {drawn:?}"
    );
    let told: usize = count
        .first()
        .and_then(|said| said.strip_prefix("\u{22ef} "))
        .and_then(|said| said.strip_suffix(" rows kept back"))
        .and_then(|told| told.parse().ok())
        .unwrap_or_else(|| {
            panic!(
                "the row above the bottom border is the card's own count of the rows it keeps \
                 back: {count:?}, {drawn:?}"
            )
        });
    assert_eq!(
        told,
        held.len() - kept.len(),
        "and the rows it kept back are the difference between the rows the list holds and the rows \
         the card draws: {count:?}"
    );
}
