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
use sindri::picture::{self, Surface};

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
