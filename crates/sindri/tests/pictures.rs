//! What a picture an answer points at comes to: a source resolved inside the workspace is
//! measured into the cells it takes and placed over them through the Kitty protocol, and one
//! that leaves the workspace — or is not there at all — is refused with the reason the
//! caption prints in its place (§12, #55).
//!
//! Nothing here has a terminal. A placement is asserted as the bytes it puts in a cell of a
//! buffer ratatui's headless backend hands back, and a cost — a raster made, a picture sent —
//! is read off the counter the placement keeps, so the three rules ADR-0008 laid down are
//! numbers a test can hold rather than a reading of the code.

use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};

use sindri::graphics::CellSize;
use sindri::markdown;
use sindri::picture::{self, Surface};
use sindri::rows::Row;

/// A directory with a workspace inside it, and one picture in that workspace: the file a
/// run's own commands could have read, and so the file an answer may point the reader at.
/// What is outside it — the directory above — is what a refusal is about.
fn workspace(width: u32, height: u32) -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().expect("a directory to stand in");
    let inside = root.path().join("ws");
    std::fs::create_dir(&inside).expect("a workspace inside it");
    shoot(&inside.join("shot.png"), width, height);
    (root, inside)
}

/// One picture written at `path`: a plain field of colour, because nothing here reads its
/// pixels — what a picture looks like is `graphics`' business.
fn shoot(path: &Path, width: u32, height: u32) {
    let picture = RgbaImage::from_pixel(width, height, Rgba([120, 80, 200, 255]));
    picture.save(path).expect("a picture on disk");
}

/// The surface a picture is measured on: the workspace a source is resolved inside, the cell
/// it is drawn at, and whether the terminal places what it is handed.
fn surface(workspace: &Path) -> Surface {
    Surface::of(Some(workspace), CellSize::new(10, 20), true)
}

/// §12's first picture: a source inside the workspace is found, and measured into the cells
/// the band's width leaves it. The source as it was written is what the picture is kept by,
/// so the same words in two answers are one picture (ADR-0008).
#[test]
fn a_picture_inside_the_workspace_is_measured_as_the_cells_it_takes() {
    let (_root, workspace) = workspace(40, 40);

    let measured = picture::measure("shot.png", 80, &surface(&workspace))
        .expect("a picture inside the workspace is one the reader may point at");

    assert_eq!(
        (measured.cols(), measured.rows()),
        (4, 2),
        "40 by 40 pixels at a 10 by 20 cell is four cells by two"
    );
    assert_eq!(
        measured.source(),
        "shot.png",
        "and it is kept by the words the answer wrote, not by the path they resolved to"
    );
    assert_eq!(
        measured.pixels(CellSize::new(10, 20)),
        (40, 40),
        "the pixels it is drawn at are the cells it takes times the cell"
    );
}

/// A source wider than the band is the ordinary case, and it is measured into the cells the
/// band has rather than refused: a block wider than its row would be clipped by the paint
/// and lost from whatever copies it (§12).
#[test]
fn a_picture_wider_than_the_band_is_measured_into_the_columns_the_band_has() {
    let (_root, workspace) = workspace(400, 200);

    let measured = picture::measure("shot.png", 20, &surface(&workspace))
        .expect("a wide picture is fitted, not refused");

    assert_eq!(
        (measured.cols(), measured.rows()),
        (20, 5),
        "40 by 10 cells fitted into 20 columns is 20 by 5, and keeps its shape"
    );
}

/// An SVG is measured by the size it declares, with no renderer asked for it: the same
/// promise ADR-0003 makes of a mermaid fence holds for a drawing an answer points at.
#[test]
fn a_drawing_inside_the_workspace_is_measured_by_the_size_it_declares() {
    let (_root, workspace) = workspace(40, 40);
    std::fs::write(
        workspace.join("plan.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
             <rect x="0" y="0" width="200" height="100" fill="#ffffff"/>
           </svg>"##,
    )
    .expect("a drawing on disk");

    let measured = picture::measure("plan.svg", 80, &surface(&workspace))
        .expect("a drawing inside the workspace is a picture");

    assert_eq!(
        (measured.cols(), measured.rows()),
        (20, 5),
        "200 by 100 pixels at a 10 by 20 cell is twenty cells by five"
    );
}

/// The refusal §12 asks for by name: a picture is drawn from a file the run could read, and
/// a file outside the workspace is not one. It is said as a reason and not as a silence,
/// because a picture that vanished is the failure the caption exists for.
#[test]
fn a_picture_outside_the_workspace_is_refused_with_the_reason_the_caption_prints() {
    let (root, workspace) = workspace(40, 40);
    shoot(&root.path().join("outside.png"), 40, 40);
    std::os::unix::fs::symlink(root.path().join("outside.png"), workspace.join("link.png"))
        .expect("a link out of the workspace");

    for source in ["/etc/hostname", "../outside.png", "link.png"] {
        let refusal = picture::measure(source, 80, &surface(&workspace))
            .expect_err("a picture outside the workspace is not drawn");

        assert!(
            refusal.message().contains("outside the workspace"),
            "{source} is refused as outside the workspace, and the caption says so: {}",
            refusal.message()
        );
    }
}

/// And a source that resolves nowhere is refused with what the read said, because a mistyped
/// path and a file the run may not read are two things a reader acts on differently.
#[test]
fn a_picture_that_is_not_there_is_refused_with_the_reason_the_read_gave() {
    let (_root, workspace) = workspace(40, 40);

    let refusal = picture::measure("absent.png", 80, &surface(&workspace))
        .expect_err("a picture that is not there is not drawn");

    assert!(
        refusal.message().contains("could not be read"),
        "the caption says the picture could not be read, and what was wrong: {}",
        refusal.message()
    );
}

/// A frame nobody told where it stands draws no picture rather than one from a directory
/// nobody named: the workspace is the whole of what makes a source a file the run could
/// read (§12).
#[test]
fn a_frame_with_no_workspace_refuses_the_picture_and_says_what_it_lacks() {
    let refusal = picture::measure("shot.png", 80, &Surface::none())
        .expect_err("a frame with no workspace has nowhere to look");

    assert!(
        refusal.message().contains("no workspace"),
        "the caption says there is nowhere to look: {}",
        refusal.message()
    );
}

/// §12's block: a picture is spoken of as the words an answer wrote over it, and as its
/// source when it wrote none — a picture with no words would copy as nothing at all.
#[test]
fn a_picture_is_spoken_of_as_its_alternate_text_or_as_its_source() {
    assert_eq!(
        picture::words("shot.png", "a purple field"),
        "a purple field",
        "the words the answer wrote over the picture are what it is spoken of as"
    );
    assert_eq!(
        picture::words("shot.png", "  "),
        "shot.png",
        "and a picture written with no words is spoken of as its source"
    );
}

/// §12's block: a picture takes the cells it was measured into, and the words it is spoken of
/// as stand at the head of them — so a reader of a terminal that places the picture sees the
/// picture, and a reader of one that does not sees what it was of.
///
/// The words are laid out as prose and not one run, so an alt text longer than the band wraps
/// onto the block's own rows instead of running over its edge: a row is whole or it is not
/// written (§12).
#[test]
fn a_picture_is_laid_out_as_the_rows_it_takes_with_its_words_at_the_head() {
    let (_root, workspace) = workspace(40, 40);

    let rows = markdown::rows_on(
        "before ![a purple field](shot.png) after",
        80,
        0,
        &surface(&workspace),
    );
    let at = rows
        .iter()
        .position(|row| row.picture().is_some())
        .expect("the picture is a row of the answer");

    assert_eq!(
        rows[at].glyphs(),
        "a purple field",
        "the row the picture stands on is its words, which is what a copy of it takes"
    );
    let measured = rows[at].picture().expect("the picture of that row");
    assert_eq!(
        (measured.cols(), measured.rows()),
        (4, 2),
        "and the cells of it are the cells it was measured into"
    );
    assert_eq!(
        rows[at + 1].glyphs(),
        "",
        "the second of its two rows is the picture's own and holds no words"
    );
    assert!(
        rows.iter().any(|row| row.glyphs().contains("after")),
        "and what the answer said after the picture is still in the answer: {:?}",
        rows.iter().map(Row::glyphs).collect::<Vec<_>>()
    );
    assert!(
        rows.iter().all(|row| row.width() <= 80),
        "no row of the answer is wider than the band it was laid out at"
    );
}

/// A picture's words are its own even when the answer wrote none: the source is what it is
/// spoken of as, because a picture with no words at all would copy as nothing.
#[test]
fn a_picture_written_with_no_alternate_text_is_spoken_of_as_its_source() {
    let (_root, workspace) = workspace(40, 40);

    let rows = markdown::rows_on("![](shot.png)", 80, 0, &surface(&workspace));

    assert_eq!(
        rows.iter()
            .find(|row| row.picture().is_some())
            .expect("the picture is a row")
            .glyphs(),
        "shot.png"
    );
}

/// A terminal that places no picture is given the words and no rows reserved for a picture it
/// cannot draw: a block of rows nothing fills is a hole in the transcript, and a hole is not
/// the honest way to say that a picture is not drawn (§12).
#[test]
fn a_terminal_that_places_no_picture_is_given_the_words_and_no_rows_for_it() {
    let (_root, workspace) = workspace(40, 40);
    let words_only = Surface::of(Some(&workspace), CellSize::new(10, 20), false);

    let rows = markdown::rows_on(
        "before ![a purple field](shot.png) after",
        80,
        0,
        &words_only,
    );

    assert!(
        rows.iter().all(|row| row.picture().is_none()),
        "nothing here claims a picture will be drawn"
    );
    assert_eq!(
        rows.iter().map(Row::glyphs).collect::<Vec<_>>().join("\n"),
        "before\n\na purple field\n\nafter",
        "the words the picture was pointed at with stand in one row of their own"
    );
}

/// And a picture that is refused says so where it would have stood, in the words the refusal
/// was measured in: a picture that vanished is the failure the caption exists for (§12).
#[test]
fn a_refused_picture_is_a_row_saying_why_it_was_not_drawn() {
    let (root, workspace) = workspace(40, 40);
    shoot(&root.path().join("outside.png"), 40, 40);

    let rows = markdown::rows_on("![a field](../outside.png)", 80, 0, &surface(&workspace));
    let said = rows.iter().map(Row::glyphs).collect::<Vec<_>>().join("\n");

    assert!(
        said.contains("a field") && said.contains("outside the workspace"),
        "the caption says what the picture was of and why it was not drawn: {said:?}"
    );
    assert!(
        rows.iter().all(|row| row.picture().is_none()),
        "and nothing claims a picture will be drawn"
    );
}

/// A picture's words are held to the band like any other words: an alt text wider than it
/// wraps instead of running over the edge, and a refusal's caption — which is longer than any
/// alt text a writer would type — does the same, so a copy of the block never takes half a
/// word (§12).
#[test]
fn a_picture_whose_words_are_wider_than_the_band_wraps_and_is_never_cut() {
    let (_root, taken) = workspace(40, 40);
    let alt = "a purple field photographed at dusk from the far side of the car park with a \
               long lens and no tripod at all";

    for draws in [true, false] {
        let on = Surface::of(Some(&taken), CellSize::new(10, 20), draws);
        let rows = markdown::rows_on(&format!("![{alt}](shot.png)"), 30, 0, &on);
        let said = rows.iter().map(Row::glyphs).collect::<Vec<_>>().join(" ");

        assert!(
            rows.iter().all(|row| row.width() <= 30),
            "every row of the block is inside the band it was laid out at: {said:?}"
        );
        assert!(
            said.contains("no tripod at all"),
            "and every word of the alt text is on the screen, draws the picture or not: \
             {said:?}"
        );
    }

    let (above, band) = workspace(40, 40);
    shoot(&above.path().join("outside.png"), 40, 40);
    let rows = markdown::rows_on("![a field](../outside.png)", 24, 0, &surface(&band));
    let said = rows.iter().map(Row::glyphs).collect::<Vec<_>>().join(" ");

    assert!(
        rows.iter().all(|row| row.width() <= 24),
        "no row of the caption runs over its band: {said:?}"
    );
    assert!(
        said.contains("outside the workspace"),
        "and the reason the caption exists to print is printed: {said:?}"
    );
}
