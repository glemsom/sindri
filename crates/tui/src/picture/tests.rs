//! The store's own tests: every one of them asks a made-up Kitty answer and a
//! file on disk, so none of them reaches a terminal (ADR-0016 §1).

use super::*;

/// The fixtures this module measures, committed beside the crate so a test
/// reads a picture and never builds one.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/pictures");

/// The cell the made-up answer draws in: twice as tall as it is wide, as a
/// terminal's is.
const CELL: (u16, u16) = (8, 18);

/// The store a test asks: the made-up answer a layout test uses, over the
/// fixtures as the workspace.
fn store() -> Store {
    store_here(FIXTURES)
}

/// The same store over a workspace a test made for itself.
fn store_here(workspace: impl Into<PathBuf>) -> Store {
    Store::new(Some(&Graphics::kitty()), workspace).expect("a Kitty answer is a store")
}

/// The block a source takes at a column width, or a panic that says why it took
/// none.
fn shown(src: &str, columns: u16) -> (u16, u16) {
    let worth = store().measure(src, columns);
    match worth {
        Worth::Drawn { columns, rows, .. } => (columns, rows),
        Worth::Refused(reason) => panic!("{src} was refused: {reason:?}"),
    }
}

/// Why a source takes nothing, in the words the caption would print.
fn refused(src: &str) -> (Refusal, u16) {
    let worth = store().measure(src, 60);
    let rows = worth.rows();
    match worth {
        Worth::Refused(reason) => (reason, rows),
        Worth::Drawn { rows, .. } => panic!("{src} was drawn in {rows} rows"),
    }
}

#[test]
fn a_wide_picture_at_sixty_columns_takes_the_rows_its_aspect_asks_for() {
    // 600×200 in eight-by-eighteen cells: the block is 60 columns of them, so
    // 480 pixels wide and 160 tall — which is nine rows of eighteen pixels.
    assert_eq!(shown("wide.png", 60), (60, 9));
}

#[test]
fn a_square_is_square_in_a_cell_twice_as_tall_as_it_is_wide() {
    // 200×200, narrower than 60 columns, so it keeps its width: 25 columns of
    // eight pixels beside 12 rows of eighteen.
    let (columns, rows) = shown("square.png", 60);
    assert_eq!((columns, rows), (25, 12));
    let (wide, tall) = (
        i32::from(columns) * i32::from(CELL.0),
        i32::from(rows) * i32::from(CELL.1),
    );
    assert!(
        (wide - tall).abs() < i32::from(CELL.1),
        "a square stays square to within a row: {wide}×{tall} pixels"
    );
}

#[test]
fn a_picture_narrower_than_the_column_keeps_its_width() {
    // 100 pixels across is 13 columns, not the 60 it was offered.
    let (columns, rows) = shown("narrow.png", 60);
    assert_eq!((columns, rows), (13, 3));
    assert!(columns < 60, "a picture is never upscaled past itself");
}

#[test]
fn a_picture_thousands_of_rows_tall_is_capped_at_the_face_allowance() {
    // 10×5000 would ask for 278 rows of eighteen pixels; the block takes the
    // twenty a face may take, and the renderer fits the picture inside them.
    assert_eq!(shown("tall.png", 60), (2, MAX_ROWS));
    assert_eq!(
        MAX_ROWS,
        crate::face::ROWS,
        "a picture's allowance is the face's, and the two do not drift"
    );
}

#[test]
fn measuring_reads_the_header_and_never_the_pixels() {
    // A header that says 100×50 over a body no decoder could read: laying out
    // a long thread measures this file without spending a decode on it.
    assert!(
        image::open(format!("{FIXTURES}/corrupt.png")).is_err(),
        "the fixture must really be undecodable, or this proves nothing"
    );
    assert_eq!(shown("corrupt.png", 60), (13, 3));
}

#[test]
fn a_missing_file_is_a_reason_and_no_rows() {
    assert_eq!(refused("nothing.png"), (Refusal::Missing, 0));
}

#[test]
fn a_directory_is_a_reason_and_no_rows() {
    assert_eq!(refused("."), (Refusal::Directory, 0));
}

#[test]
fn bytes_that_are_not_an_image_are_a_reason_and_no_rows() {
    assert_eq!(refused("notes.txt"), (Refusal::NotAnImage, 0));
}

#[test]
fn a_picture_cut_off_inside_its_header_is_a_reason_and_no_rows() {
    assert_eq!(refused("truncated.png"), (Refusal::Damaged, 0));
}

#[test]
fn a_format_the_decoders_do_not_link_is_a_reason_and_names_itself() {
    assert_eq!(
        refused("map.bmp"),
        (Refusal::Unsupported("bmp".to_string()), 0)
    );
}

#[test]
fn a_file_that_claims_a_hundred_billion_pixels_is_refused_before_the_decode() {
    assert_eq!(
        refused("bomb.png"),
        (
            Refusal::TooManyPixels {
                width: 100_000,
                height: 100_000
            },
            0
        )
    );
}

#[test]
fn a_source_with_a_scheme_is_refused_by_name_and_never_fetched() {
    assert_eq!(
        refused("https://example.com/plot.png"),
        (Refusal::Scheme("https".to_string()), 0)
    );
    assert_eq!(
        refused("data:image/png;base64,AAAA"),
        (Refusal::Scheme("data".to_string()), 0)
    );
}

#[test]
fn a_source_over_the_byte_cap_is_refused_without_reading_it() {
    let over = tempfile::tempdir().expect("a scratch directory");
    let path = over.path().join("huge.png");
    std::fs::write(&path, vec![0u8; (BYTES + 1) as usize]).expect("a file past the cap");
    let worth = store_here(over.path()).measure("huge.png", 60);
    assert_eq!(worth, Worth::Refused(Refusal::TooBig(BYTES + 1)));
}

#[test]
fn an_absolute_path_is_taken_as_it_is_and_a_tilde_is_not_expanded() {
    let absolute = format!("{FIXTURES}/wide.png");
    assert!(matches!(
        store().measure(&absolute, 60),
        Worth::Drawn { .. }
    ));
    assert_eq!(refused("~/wide.png"), (Refusal::Missing, 0));
}

#[test]
fn every_reason_is_a_sentence_the_caption_can_print() {
    assert_eq!(Refusal::Missing.said(), "no such file");
    assert_eq!(Refusal::Directory.said(), "a directory");
    assert_eq!(Refusal::NotAnImage.said(), "not an image");
    assert_eq!(Refusal::Damaged.said(), "its header is not whole");
    assert_eq!(Refusal::NoGraphics.said(), "this terminal draws no picture");
    assert_eq!(
        Refusal::Scheme("https".to_string()).said(),
        "https: is not fetched"
    );
    assert_eq!(
        Refusal::Unsupported("bmp".to_string()).said(),
        "bmp: a format not drawn"
    );
    assert_eq!(
        Refusal::TooBig(BYTES + 1).said(),
        "16.0 MiB, over the 16 MiB cap"
    );
    assert_eq!(
        Refusal::TooManyPixels {
            width: 100_000,
            height: 100_000
        }
        .said(),
        "100000×100000, over the 40 megapixel cap"
    );
}

#[test]
fn the_same_key_is_decoded_once_and_drawn_from_cache() {
    let mut store = store();
    let worth = store.measure("wide.png", 60);
    assert!(store.protocol(&worth).is_some(), "the picture is drawable");
    assert_eq!(store.decodes, 1, "the first draw decodes it");
    assert!(
        store.protocol(&worth).is_some(),
        "a redraw of the same answer"
    );
    assert_eq!(store.decodes, 1, "and costs no second decode");
}

#[test]
fn a_picture_rewritten_under_a_new_answer_is_read_again() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = scratch.path().join("plot.png");
    std::fs::copy(format!("{FIXTURES}/wide.png"), &path).expect("a picture an agent wrote");

    let mut store = store_here(scratch.path());
    let worth = store.measure("plot.png", 60);
    store.protocol(&worth);
    assert_eq!(store.decodes, 1);

    // A second, later answer under the same name and length: only the mtime
    // says the file is not the one already held.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(1);
    File::options()
        .write(true)
        .open(&path)
        .and_then(|file| file.set_modified(later))
        .expect("a rewritten file is re-dated");
    let worth = store.measure("plot.png", 60);
    store.protocol(&worth);
    assert_eq!(store.decodes, 2, "a new answer is fetched, not reused");
}

#[test]
fn the_ninth_picture_evicts_the_first() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    for plot in 0..9 {
        std::fs::copy(
            format!("{FIXTURES}/wide.png"),
            scratch.path().join(format!("plot{plot}.png")),
        )
        .expect("nine pictures");
    }
    let mut store = store_here(scratch.path());
    let plots: Vec<Worth> = (0..9)
        .map(|plot| store.measure(&format!("plot{plot}.png"), 60))
        .collect();
    for worth in &plots {
        store.protocol(worth);
    }
    assert_eq!(store.decodes, 9);
    assert_eq!(
        store.live.len(),
        LIVE,
        "eight are live, each holding pixels"
    );

    store.protocol(&plots[0]);
    assert_eq!(store.decodes, 10, "the oldest was let go to make room");
    assert_eq!(store.live.len(), LIVE);
    store.protocol(&plots[8]);
    assert_eq!(store.decodes, 10, "and the newest is still held");
}

#[test]
fn a_refused_source_has_nothing_to_draw() {
    let mut store = store();
    let worth = store.measure("nothing.png", 60);
    assert!(store.protocol(&worth).is_none(), "nothing was measured");
    assert_eq!(store.decodes, 0, "and nothing was decoded");
}

#[test]
fn a_picture_in_any_format_the_decoders_link_is_measured() {
    // Written here rather than committed: what is under test is the store's
    // reading of a format, not the codec's own arithmetic.
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let picture = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        40,
        30,
        image::Rgb([10, 200, 30]),
    ));
    for (format, name) in [
        (ImageFormat::Jpeg, "photo.jpg"),
        (ImageFormat::Gif, "plot.gif"),
        (ImageFormat::WebP, "shot.webp"),
    ] {
        let path = scratch.path().join(name);
        picture
            .write_to(&mut File::create(&path).expect("a file to write"), format)
            .expect("an encoder the decoders link");
        assert!(
            matches!(
                store_here(scratch.path()).measure(name, 60),
                Worth::Drawn { .. }
            ),
            "{name} is a picture the store draws"
        );
    }
}

#[test]
fn there_is_no_store_without_a_kitty_answer() {
    assert!(
        Store::new(None, FIXTURES).is_none(),
        "a frame that asked nothing has no picture to hold"
    );
    let mut coarse = Picker::from_fontsize(CELL);
    coarse.set_protocol_type(ProtocolType::Halfblocks);
    assert!(
        Store::new(Some(&Graphics::from_picker(coarse)), FIXTURES).is_none(),
        "no sixel, no half-blocks, no 256-colour fallback"
    );
}
