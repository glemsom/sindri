//! A picture is a block and it keeps its words: the rows the store measured,
//! and the caption the alt text and the target make (ADR-0016 §2, §8).

use super::*;

use sindri_tui::graphics::Graphics;
use sindri_tui::markdown;
use sindri_tui::picture::Store;

/// The pictures a test draws from: committed beside the crate, so a test reads
/// a file and never builds one.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/pictures");

/// The store a test measures with: the made-up Kitty answer the layout tests
/// use, over the fixtures as the workspace (ADR-0016 §1).
fn store() -> Store {
    Store::new(Some(&Graphics::kitty()), FIXTURES).expect("a Kitty answer is a store")
}

#[test]
fn a_picture_that_opens_an_answer_takes_its_rows_then_its_caption() {
    let block = markdown::lines(
        "![a chart](wide.png)\n\nthe next block\n",
        60,
        Some(&store()),
    );
    let shown = texts(&block.rows);

    assert_eq!(
        shown.len(),
        12,
        "nine rows of picture, the caption, a gap, the next block: {shown:?}"
    );
    assert!(
        shown[..9].iter().all(String::is_empty),
        "the block reserves the nine rows the store measured, blank: {shown:?}"
    );
    assert_eq!(shown[9], "a chart ‹wide.png›", "the caption is below them");
    assert_eq!(shown[10], "", "and the next block starts one row after it");
    assert_eq!(shown[11], "the next block");
}

#[test]
fn an_image_in_the_middle_of_a_sentence_ends_the_row_and_the_rest_follows_the_caption() {
    let block = markdown::lines(
        "see ![a chart](wide.png) for the numbers",
        60,
        Some(&store()),
    );
    let shown = texts(&block.rows);

    assert_eq!(
        shown[0], "see",
        "the sentence stops where the picture is: {shown:?}"
    );
    assert!(
        shown[1..10].iter().all(String::is_empty),
        "the block takes its nine rows under that row: {shown:?}"
    );
    assert_eq!(shown[10], "a chart ‹wide.png›");
    assert_eq!(
        shown[11], "for the numbers",
        "the rest of the paragraph goes on below the caption: {shown:?}"
    );
}

#[test]
fn a_picture_with_no_words_is_its_target_alone() {
    let block = markdown::lines("![](wide.png)\n", 60, Some(&store()));
    let shown = texts(&block.rows);

    assert_eq!(
        shown[9], "‹wide.png›",
        "an absent alt text is no words: {shown:?}"
    );
    assert!(
        shown[..9].iter().all(String::is_empty),
        "the block is still reserved above it: {shown:?}"
    );
}

#[test]
fn a_picture_that_cannot_be_drawn_is_its_caption_and_takes_no_rows() {
    let block = markdown::lines("![a chart](plot.png)\n", 60, Some(&store()));
    let shown = texts(&block.rows);

    assert_eq!(
        shown,
        vec!["a chart ‹plot.png: no such file›"],
        "there is no file, so there is no block and no hole: {shown:?}"
    );
    assert_eq!(
        block.pictures[0].worth,
        sindri_tui::picture::Worth::Refused(sindri_tui::picture::Refusal::Missing),
        "the store's reason is kept, and it takes nothing"
    );
}

#[test]
fn every_refusal_is_its_own_reason_in_the_caption() {
    for (source, reason) in [
        ("![a chart](missing.png)", "missing.png: no such file"),
        ("![a chart](notes.txt)", "notes.txt: not an image"),
        ("![a chart](map.bmp)", "map.bmp: bmp: a format not drawn"),
        (
            "![a chart](https://example.com/plot.png)",
            "https://example.com/plot.png: https: is not fetched",
        ),
    ] {
        let block = markdown::lines(source, 100, Some(&store()));
        let shown = texts(&block.rows);
        assert_eq!(
            shown,
            vec![format!("a chart ‹{reason}›")],
            "{source} is a caption and the reason it is one: {shown:?}"
        );
    }
}

#[test]
fn a_terminal_that_draws_no_picture_says_so_in_the_caption() {
    // No store at all: the terminal was never asked, or answered that it speaks
    // no graphics, and the frame says that rather than pretending (ADR-0016 §1).
    let block = markdown::lines("![a chart](wide.png)\n", 60, None);
    let shown = texts(&block.rows);

    assert_eq!(
        shown,
        vec!["a chart ‹wide.png: this terminal draws no picture›"]
    );
    assert_eq!(
        block.pictures[0].worth,
        sindri_tui::picture::Worth::Refused(sindri_tui::picture::Refusal::NoGraphics),
        "nothing was measured, so there is nothing to draw"
    );
}

#[test]
fn a_picture_takes_the_rows_the_store_measured_and_not_one_of_its_own() {
    // tall.png is 10×5000: the store caps the block at MAX_ROWS while it keeps
    // its full column span, so the writer must take the store's row count and
    // never narrow the block itself (ADR-0016 §4).
    let block = markdown::lines("![a chart](tall.png)\n", 60, Some(&store()));
    let shown = texts(&block.rows);

    assert_eq!(
        shown.len(),
        usize::from(sindri_tui::picture::MAX_ROWS) + 1,
        "twenty rows of picture above the caption: {shown:?}"
    );
    assert_eq!(shown[20], "a chart ‹tall.png›");
}

#[test]
fn a_picture_in_a_table_cell_or_a_list_item_is_the_words_it_was_given() {
    let table = markdown::lines(
        "| note |\n| --- |\n| ![a chart](wide.png) |\n",
        60,
        Some(&store()),
    );
    let cell = texts(&table.rows)
        .into_iter()
        .find(|row| row.contains("a chart"))
        .expect("the cell says what the picture's words were");
    assert_eq!(cell.trim(), "a chart", "the cell is unchanged: {cell:?}");
    assert!(
        !cell.contains('‹'),
        "and it keeps no caption of its own: {cell:?}"
    );
    assert!(
        table.pictures.is_empty(),
        "a cell is not a place a graphic goes (ADR-0016 §2)"
    );

    let item = markdown::lines("- ![a chart](wide.png)\n", 60, Some(&store()));
    let shown = texts(&item.rows);
    assert_eq!(
        shown,
        vec!["• a chart"],
        "an item says what it said before: {shown:?}"
    );
    assert!(item.pictures.is_empty(), "an item is not a block either");

    let heading = markdown::lines("# ![a chart](wide.png)\n", 60, Some(&store()));
    let shown = texts(&heading.rows);
    assert_eq!(
        shown[0], "a chart",
        "and a heading keeps the words it was given: {shown:?}"
    );
    assert!(heading.pictures.is_empty(), "a title is not a block either");
}

#[test]
fn the_writer_says_where_a_pictures_block_starts_and_what_it_is_called() {
    let block = markdown::lines(
        "intro\n\n![a chart](wide.png)\n\ntail\n",
        60,
        Some(&store()),
    );
    let picture = block
        .pictures
        .first()
        .unwrap_or_else(|| panic!("the writer placed the picture"));

    assert_eq!(
        picture.at, 2,
        "the first row of its block, after intro and its gap"
    );
    assert_eq!(picture.src, "wide.png", "the source as the model wrote it");
    assert_eq!(picture.caption, "a chart ‹wide.png›");
    assert!(
        block.rows[picture.at]
            .spans
            .iter()
            .all(|span| span.content.trim().is_empty()),
        "the block starts on a blank row: {:?}",
        texts(&block.rows)
    );
    let caption = &block.rows[picture.at + 9];
    assert!(
        caption
            .spans
            .iter()
            .all(|span| span.style == sindri_tui::theme::faint()),
        "the caption is dimmed: {caption:?}"
    );
}

#[test]
fn a_picture_block_is_its_graphic_rows_and_its_caption_rows() {
    let block = markdown::lines(
        "![a chart of the thing that happened in the long run](wide.png)\n\nthe next block\n",
        30,
        Some(&store()),
    );
    let shown = texts(&block.rows);
    let picture = block
        .pictures
        .first()
        .unwrap_or_else(|| panic!("the writer placed the picture"));
    let end = picture.at + picture.rows;

    assert_eq!(
        picture.rows, 8,
        "five rows of picture and the three the caption wrapped into, and not \
         one of the next block's: {shown:?}"
    );
    assert!(
        shown[picture.at + 5].starts_with("a chart") && shown[end - 1].ends_with('›'),
        "the caption's rows are the block's last: {shown:?}"
    );
    assert_eq!(
        shown[end], "",
        "the block ends with its caption, before the gap: {shown:?}"
    );
    assert_eq!(shown[end + 1], "the next block", "{shown:?}");
}

#[test]
fn a_caption_wider_than_the_column_wraps_like_prose() {
    let block = markdown::lines(
        "![a chart of the thing that happened in the long run](wide.png)\n",
        30,
        Some(&store()),
    );
    let shown = texts(&block.rows);

    assert!(
        shown.last().is_some_and(|row| row.contains("‹wide.png›")),
        "the target is still there, on the caption's last row: {shown:?}"
    );
    assert!(
        shown[..shown.len() - 1]
            .iter()
            .all(|row| row.chars().count() <= 30),
        "no caption row is wider than the column: {shown:?}"
    );
}
