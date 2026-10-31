//! Selecting rendered rows and getting them out: the drag, the refusal that
//! says it refused, and the keyboard's own way to do both — the mark's
//! lifetime is `marks.rs`'s.

use super::*;

use sindri_tui::graphics::Graphics;

#[test]
fn a_drag_copies_the_rendered_rows_and_nothing_else() {
    let clipboard = Fake::taking();
    let mut app = answered(clipboard.clone());
    let rows = screen(&mut app, 100, 20);
    let row = rows
        .iter()
        .position(|row| row.contains("the answer"))
        .expect("the answer is on screen");

    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        0,
        row as u16,
    );
    mouse(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        40,
        row as u16 + 1,
    );
    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        40,
        row as u16 + 1,
    );

    let copied = clipboard.saw();
    assert_eq!(copied.len(), 1, "one drag is one copy: {copied:?}");
    // The answer hangs two columns off its turn's gutter, and a copy keeps the
    // first row's indentation.
    assert_eq!(
        copied[0], "  the answer, in full",
        "the copy is exactly the rows the drag covered"
    );
    assert!(
        !copied[0].contains('\u{1b}'),
        "and no escape sequence reaches it"
    );
}

#[test]
fn a_refused_copy_is_reported_as_a_refusal_and_never_as_success() {
    let mut app = answered(Fake::refusing());
    let rows = screen(&mut app, 100, 20);
    let row = rows
        .iter()
        .position(|row| row.contains("the answer"))
        .expect("the answer is on screen");
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        0,
        row as u16,
    );
    mouse(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        30,
        row as u16,
    );
    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        30,
        row as u16,
    );

    let said = notes(&app);
    assert_eq!(
        said, "a question\ncopy refused: no tool on PATH took the text",
        "a refusal says it refused, and nothing else is said of the copy"
    );
}

#[test]
fn ctrl_y_copies_the_last_answer_without_a_mouse() {
    let clipboard = Fake::taking();
    let mut app = answered(clipboard.clone());
    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    assert_eq!(
        clipboard.saw(),
        vec!["the answer, in full".to_string()],
        "Ctrl-Y is the drag's keyboard twin"
    );
    assert_eq!(
        notes(&app),
        "a question",
        "and a copy the terminal cannot confirm says nothing of itself: {:?}",
        notes(&app)
    );
}

#[test]
fn a_copy_through_a_shell_tool_names_the_tool() {
    let clipboard = Fake {
        seen: Arc::new(Mutex::new(Vec::new())),
        answer: Copied::Tool("wl-copy"),
    };
    let mut app = answered(clipboard);
    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    assert!(
        notes(&app).contains("via wl-copy"),
        "the note names the channel that took it: {:?}",
        notes(&app)
    );
}

/// One answer whose whole body is a picture: the rows it is drawn on, and the
/// caption under them.
fn pictured(clipboard: Fake) -> App {
    let mut app = App::with_clipboard(
        launch("4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77"),
        None,
        Box::new(clipboard),
    )
    .with_graphics(Some(Graphics::kitty()))
    .with_workspace(PICTURES);
    app.note_info("a question");
    app.event(Event::TextDelta {
        turn: 1,
        text: "![a chart](narrow.png)\n".to_string(),
    });
    app
}

/// The pictures a test draws from: committed beside the crate, so a test reads a
/// file and never builds one.
const PICTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/pictures");

#[test]
fn ctrl_y_on_an_answer_holding_a_picture_copies_its_caption_once_and_no_blank_rows() {
    let clipboard = Fake::taking();
    let mut app = pictured(clipboard.clone());
    assert!(
        screen(&mut app, 100, 20)
            .iter()
            .any(|row| row.contains("a chart \u{2039}narrow.png\u{203A}")),
        "the picture's caption is in the frame"
    );

    // The whole answer, selected from the keyboard: its marker, the three rows
    // of picture and the caption under them.
    for _ in 0..5 {
        press(&mut app, KeyCode::Up, KeyModifiers::SHIFT);
    }
    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);

    let copied = clipboard.saw();
    assert_eq!(copied.len(), 1, "one copy: {copied:?}");
    assert_eq!(
        copied[0], "  \u{280B} 0.0s\na chart \u{2039}narrow.png\u{203A}",
        "the block is its caption once, and not the run of blank rows it is \
         drawn on: {copied:?}"
    );
}

/// The rail's own columns of a 100-column frame.
const RAIL: usize = 32;
/// The label field the rail draws: nine columns, `workspace` whole.
const LABEL: usize = 9;

/// The rail's text of one row, at 100 columns.
fn rail_of(row: &str) -> String {
    row.chars().skip(row.chars().count() - RAIL).collect()
}

/// The row of the rail that states `label`, found by the label's own column.
fn rail_row(rows: &[String], label: &str) -> usize {
    rows.iter()
        .position(|row| {
            rail_of(row)
                .chars()
                .skip(render::RAIL_INSET)
                .take(LABEL)
                .collect::<String>()
                .trim_end()
                == label
        })
        .unwrap_or_else(|| panic!("the rail states {label}: {rows:#?}"))
}

#[test]
fn clicking_a_rail_row_copies_the_fact_and_none_of_its_typography() {
    let clipboard = Fake::taking();
    let mut app = app_with(clipboard.clone());
    let rows = screen(&mut app, 100, 20);

    // The rows are drawn as a ledger — a label at the edge, a leader across
    // the gap — and neither typography is something a copy should carry.
    let cache = rail_row(&rows, "cache");
    let opened: String = rail_of(&rows[cache]).chars().take(LABEL).collect();
    assert_eq!(
        opened.trim_end(),
        format!("{}{}", " ".repeat(render::RAIL_INSET), "cache"),
        "the label opens the row inside the rail's inset: {:?}",
        rows[cache]
    );
    assert!(
        rail_of(&rows[cache]).contains('·'),
        "the leader bridges the field to the value: {:?}",
        rows[cache]
    );

    for (label, said) in [("cache", "cache —"), ("context", "context ░░░░░░░░ 0/200k")]
    {
        let row = rail_row(&rows, label);
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            100 - RAIL as u16 + 5,
            row as u16,
        );
        let copied = clipboard.saw();
        assert_eq!(
            copied.last(),
            Some(&said.to_string()),
            "a click copies the fact, label and value, and nothing drawn around \
             them: {copied:?}"
        );
    }
    assert!(
        clipboard.saw().iter().all(|copy| !copy.contains('·')),
        "no leader reaches the clipboard: {:?}",
        clipboard.saw()
    );
}

#[test]
fn a_rail_row_copies_the_same_fact_with_a_glow_in_the_margin() {
    // The seam is additive: what a row copies is the fact, and a terminal that
    // draws the light strip changes nothing about the text.
    let mut copies = Vec::new();
    for glowing in [false, true] {
        let clipboard = Fake::taking();
        let mut app = app_with(clipboard.clone());
        if glowing {
            app = app.with_graphics(Some(Graphics::kitty()));
        }
        let rows = screen(&mut app, 100, 20);
        let row = rail_row(&rows, "cache");
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            100 - RAIL as u16 + 5,
            row as u16,
        );
        copies.push(clipboard.saw());
    }
    assert_eq!(copies[0], copies[1], "the same fact, lit or not");
    assert_eq!(copies[0], vec!["cache —".to_string()]);
}

#[test]
fn ctrl_g_copies_the_session_id_alone() {
    let clipboard = Fake::taking();
    let mut app = app_with(clipboard.clone());
    screen(&mut app, 100, 20);
    press(&mut app, KeyCode::Char('g'), KeyModifiers::CONTROL);
    assert_eq!(
        clipboard.saw(),
        vec!["4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77".to_string()],
        "Ctrl-G is the id alone, pasted into `sindri sessions show`"
    );
}

/// A diagram no file backs: a mermaid fence's body is its own source.
const DIAGRAM: &str = "stateDiagram-v2\n    [*] --> Resting\n    Resting --> [*]\n";

/// One answer whose whole body is a diagram: a block of rows and its caption.
fn diagrammed(clipboard: Fake) -> App {
    let mut app = App::with_clipboard(
        launch("4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77"),
        None,
        Box::new(clipboard),
    )
    .with_graphics(Some(Graphics::kitty()));
    app.note_info("a question");
    app.event(Event::TextDelta {
        turn: 1,
        text: format!("```mermaid\n{DIAGRAM}```\n"),
    });
    app
}

#[test]
fn a_diagrams_block_copies_as_its_mermaid_source_and_never_as_its_caption() {
    let clipboard = Fake::taking();
    let mut app = diagrammed(clipboard.clone());
    let rows = screen(&mut app, 100, 30);
    assert!(
        rows.iter()
            .any(|row| row.contains("\u{2039}mermaid\u{203A}")),
        "the fence is a block with its caption: {rows:#?}"
    );

    // The whole block, dragged over as a reader takes a diagram: only its rows
    // leave with the selection, and what leaves is the source.
    let block = app
        .viewport()
        .pictures
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("the diagram was placed: {rows:#?}"));
    let area = app.viewport().area;
    let (top, bottom) = (
        area.y + block.at as u16,
        area.y + (block.at + block.rows - 1) as u16,
    );
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 0, top);
    mouse(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        60,
        bottom,
    );
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 60, bottom);

    let copied = clipboard.saw();
    assert_eq!(
        copied,
        vec![DIAGRAM.trim_end_matches('\n').to_string()],
        "a diagram pasted somewhere is mermaid, not a pair of guillemets: {copied:?}"
    );
}
