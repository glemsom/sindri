//! The count the band's edge hangs of the draft: the files its words name, the command it opens
//! with and the characters it holds, as a reader's own keys and a card's own rows change it.

use std::path::Path;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::facts::Facts;
use sindri::view::{Transcript, View};

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

/// A frame standing in `root` with `draft` typed through the keys a reader's fingers send.
fn typing(root: &Path, draft: &str) -> View {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked().working(root.to_path_buf(), None),
        Transcript::none(),
    );
    view.paste(draft);
    view
}

/// The frame `view` paints at `cols` columns and `rows` rows, read back cell by cell.
fn painted(view: &mut View, cols: u16, rows: u16) -> Buffer {
    let mut terminal =
        Terminal::new(TestBackend::new(cols, rows)).expect("a headless screen of that size");
    terminal
        .draw(|frame| {
            view.paint(frame.area(), frame.buffer_mut());
        })
        .expect("the frame is painted");
    terminal.backend().buffer().clone()
}

/// The band's edge row, as the glyphs it draws: the one row of the frame that opens a box.
fn edge_row(buffer: &Buffer) -> String {
    let area = buffer.area;
    (area.y..area.bottom())
        .map(|y| {
            (area.x..area.right())
                .map(|x| buffer.cell((x, y)).expect("a cell of the row").symbol())
                .collect::<String>()
        })
        .find(|row| row.starts_with('\u{250c}'))
        .unwrap_or_else(|| panic!("the frame draws no band edge row"))
}

fn draft(view: &View) -> &str {
    view.composer().text()
}

#[test]
fn a_paste_is_counted_the_moment_it_lands() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "read src/rows.rs");
    let buffer = painted(&mut view, 120, 40);
    let edge = edge_row(&buffer);

    assert!(
        edge.ends_with("1 file · 16 chars┐"),
        "the edge hangs no count of the draft the reader pasted: {edge:?}"
    );
}

#[test]
fn a_recalled_prompt_is_counted_as_the_draft_it_becomes() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "read src/rows.rs");
    let Outcome::Send(prompt) = view.key(Key::Enter) else {
        panic!("a draft naming no command is a prompt once it is sent");
    };
    view.enqueue(prompt);

    assert_eq!(
        draft(&view),
        "",
        "the draft a reader sent is the queue's and no longer theirs"
    );
    assert_eq!(
        view.key(Key::Up),
        Outcome::Edited,
        "`↑` recalls the prompt the queue holds"
    );
    let buffer = painted(&mut view, 120, 40);
    let edge = edge_row(&buffer);

    assert!(
        edge.ends_with("1 file · 16 chars┐"),
        "the edge counts the draft a recalled prompt becomes: {edge:?}"
    );
}

#[test]
fn a_row_a_card_writes_into_the_draft_is_counted_with_it() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "read ");
    view.key(Key::Char('@'));
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Edited,
        "the chosen path is written into the draft"
    );
    assert_eq!(draft(&view), "read src/rows.rs ");
    let buffer = painted(&mut view, 120, 40);
    let edge = edge_row(&buffer);

    assert!(
        edge.ends_with("1 file · 17 chars┐"),
        "the edge counts the file the picker's row wrote into the draft: {edge:?}"
    );

    let mut view = typing(root.path(), "/sk");
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Edited,
        "the row the commands card marks is written into the draft"
    );
    assert_eq!(
        draft(&view),
        "/skill",
        "a row whose line names no command yet is a word of the draft"
    );
    let buffer = painted(&mut view, 120, 40);
    let edge = edge_row(&buffer);

    assert!(
        edge.ends_with("6 chars┐"),
        "the edge counts the word the commands card wrote into the draft: {edge:?}"
    );
}
