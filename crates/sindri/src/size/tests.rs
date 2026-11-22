use super::*;

use crate::ink::Ink;

#[test]
fn a_run_is_written_as_its_words_and_measured_as_the_cells_they_leave() {
    let run = Sized::new(2, SCALE, "a日", Look::plain(Ink::Plain));

    assert_eq!(
        run.width(),
        6,
        "one cell and two cells at twice the cell are six columns"
    );
    assert_eq!(
        run.spelling(),
        "a 日  ",
        "every glyph is followed by the columns the terminal draws it over, so the run is as \
             wide as it is drawn"
    );
}

fn row_of(buffer: &Buffer, y: u16, width: u16) -> String {
    (0..width)
        .map(|x| buffer.cell((x, y)).expect("a cell of the row").symbol())
        .collect()
}

#[test]
fn a_run_with_no_row_of_the_band_under_it_is_drawn_as_the_words_it_says() {
    let run = Sized::new(0, SCALE, "The rule", Look::plain(Ink::Plain));
    let palette = Palette::TRUECOLOR;

    let band = Rect::new(0, 0, 20, 4);
    let mut sizes = Sizes::none();
    sizes.lay(3, 0, std::slice::from_ref(&run));
    let mut buffer = Buffer::empty(band);
    sizes.settle(band, &mut buffer, &palette);

    assert!(
        sizes.laid().is_empty(),
        "a block with no row of the band under it is no block at all"
    );
    assert_eq!(
        row_of(&buffer, 3, 8),
        "        ",
        "and the row keeps the words it was written as, which the frame spelled"
    );

    let band = Rect::new(0, 0, 20, 5);
    let mut sizes = Sizes::none();
    sizes.lay(3, 0, std::slice::from_ref(&run));
    let mut buffer = Buffer::empty(band);
    sizes.settle(band, &mut buffer, &palette);

    assert_eq!(sizes.laid().len(), 1, "a block with a row under it stands");
    assert_eq!(
        row_of(&buffer, 3, 8),
        "T h e   ",
        "and the row is written as the cells the sizing leaves"
    );
}

#[test]
fn a_terminal_that_sizes_text_is_one_that_names_itself_after_one_that_does() {
    assert!(
        speaks(Some("xterm-kitty"), None),
        "a terminal that names itself after one that sizes text draws a heading large"
    );
    assert!(
        speaks(Some("xterm-256color"), Some("ghostty")),
        "and one named by the program that runs it"
    );
    assert!(
        !speaks(Some("xterm-256color"), None),
        "a terminal that says nothing keeps today's spelling"
    );
    assert!(!speaks(None, None), "and so does one that names nothing");
    assert!(
        !speaks(Some("screen-256color"), Some("tmux")),
        "a multiplexer is not the terminal it draws on"
    );
    assert!(
        !speaks(Some("alacritty"), None),
        "a terminal that places a hyperlink and draws no large text is handed the small one"
    );
}
