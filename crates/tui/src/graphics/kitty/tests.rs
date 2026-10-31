//! What the frame's raw placements put on the wire: the sequences, held to the
//! shapes the protocol asks for rather than to a terminal's answer.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui_image::picker::Picker;

use super::*;
use crate::graphics::Graphics;

/// A frame forty columns wide and ten rows tall, of which an image takes a part.
fn screen() -> Buffer {
    Buffer::empty(Rect::new(0, 0, 40, 10))
}

/// A placement with a made-up Kitty answer, and the frame it draws into.
fn placed() -> (Placement, Buffer) {
    (
        Placement::new(&Graphics::kitty()).expect("a made-up Kitty answer places"),
        screen(),
    )
}

/// The rect most of these tests place an image in: four columns by three rows, at
/// an offset nothing else shares.
fn at() -> Rect {
    Rect::new(2, 3, 4, 3)
}

/// Pixels, as a caller hands them over: how they look is the caller's business
/// and this module never looks.
fn image() -> RgbaImage {
    RgbaImage::from_pixel(32, 54, image::Rgba([11, 22, 33, 255]))
}

/// A terminal that answers something other than Kitty: the fallback the frame
/// draws instead of an image.
fn not_kitty() -> Graphics {
    Graphics::from_picker(Picker::from_fontsize((8, 18)))
}

/// The escapes `buf` carries, as the one symbol a cell holds each: a placement is
/// written into a single cell and the cells it covers are marked, never painted.
fn escapes(buf: &Buffer) -> Vec<String> {
    buf.content
        .iter()
        .map(|cell| cell.symbol().to_string())
        .filter(|symbol| symbol.contains('\u{1b}'))
        .collect()
}

/// Whether a sequence transmits the image's bytes, rather than only placing what
/// the terminal already holds.
fn transmits(sequence: &str) -> bool {
    sequence.contains("a=T")
}

/// Whether a sequence places an image the terminal already holds.
fn places(sequence: &str) -> bool {
    sequence.contains("a=p")
}

/// The image id a sequence names.
fn id(sequence: &str) -> String {
    sequence
        .split(',')
        .find_map(|field| field.strip_prefix("i="))
        .unwrap_or_else(|| panic!("a sequence names its image: {sequence:?}"))
        .to_string()
}

#[test]
fn a_made_up_kitty_answer_transmits_the_pixels_and_places_them_without_moving_the_cursor() {
    let (mut placement, mut buf) = placed();
    placement.place(&image(), at(), -1, &mut buf);
    let sent = escapes(&buf);
    assert_eq!(sent.len(), 1, "one escape, in one cell: {sent:?}");
    let sequence = &sent[0];
    assert!(
        transmits(sequence),
        "the first frame sends the pixels: {sequence:?}"
    );
    assert!(places(sequence), "and places them: {sequence:?}");
    assert!(
        sequence.contains("a=T,f=100,"),
        "the payload is a PNG, which a flat gradient keeps tiny: {sequence:?}"
    );
    assert!(
        sequence.contains("C=1"),
        "the frame positions its own cursor, so a placement may not move it: {sequence:?}"
    );
    assert!(
        sequence.contains("q=2"),
        "nothing answers an escape this side never reads: {sequence:?}"
    );
}

#[test]
fn a_terminal_that_answers_something_else_writes_no_escape_at_all() {
    assert!(
        Placement::new(&not_kitty()).is_none(),
        "the caller falls back to its glyphs, and not one byte reaches the buffer"
    );
}

#[test]
fn the_first_frame_transmits_the_image_and_the_next_places_it_for_nothing() {
    let (mut placement, mut first) = placed();
    placement.place(&image(), at(), -1, &mut first);
    assert!(
        transmits(&escapes(&first)[0]),
        "the first frame sends the pixels"
    );

    let mut next = screen();
    placement.place(&image(), at(), -1, &mut next);
    let sent = escapes(&next);
    assert_eq!(sent.len(), 1, "the second frame still places it: {sent:?}");
    assert!(
        !transmits(&sent[0]),
        "and sends no pixel of it again: {:?}",
        sent[0]
    );
    assert!(
        places(&sent[0]),
        "it places what the terminal holds: {:?}",
        sent[0]
    );
}

#[test]
fn a_new_rect_transmits_the_image_again_under_an_id_no_other_placement_has_used() {
    let (mut placement, mut first) = placed();
    placement.place(&image(), at(), -1, &mut first);
    let before = escapes(&first);

    let wider = Rect { width: 6, ..at() };
    let mut resized = screen();
    placement.place(&image(), wider, -1, &mut resized);
    let after = escapes(&resized);
    assert!(
        transmits(&after[0]),
        "a rect the terminal has no image for is an image sent again: {after:?}"
    );
    assert_ne!(
        id(&before[0]),
        id(&after[0]),
        "an id is never reused across a resize: {:?} then {:?}",
        before[0],
        after[0]
    );
}

#[test]
fn a_new_layer_places_the_image_again_on_the_layer_it_is_given() {
    let (mut placement, mut first) = placed();
    placement.place(&image(), at(), -1, &mut first);
    assert!(
        escapes(&first)[0].contains("z=-1"),
        "under the glyphs: {:?}",
        escapes(&first)[0]
    );

    let mut above = screen();
    placement.place(&image(), at(), 0, &mut above);
    let sent = escapes(&above);
    assert!(
        !transmits(&sent[0]),
        "the pixels are unchanged, so only their place is said again: {sent:?}"
    );
    assert!(
        sent[0].contains("z=0"),
        "and it is said on the layer it was given: {:?}",
        sent[0]
    );
}

#[test]
fn the_image_covers_its_rect_and_the_cells_beside_it_are_not_its() {
    let (mut placement, mut buf) = placed();
    placement.place(&image(), at(), -1, &mut buf);
    for y in at().top()..at().bottom() {
        for x in at().left()..at().right() {
            let cell = &buf[(x, y)];
            if (x, y) == (at().x, at().y) {
                assert!(!cell.skip, "the cell that carries the escape is painted");
                assert!(cell.symbol().contains('\u{1b}'), "and carries it");
            } else {
                assert!(cell.skip, "({x}, {y}) is covered by the image");
                assert_eq!(cell.symbol(), " ", "({x}, {y}) paints no glyph of its own");
            }
        }
    }
    let beside = (at().right(), at().y);
    assert!(
        !buf[beside].skip,
        "the cell beside the rect is not the image's"
    );
}

#[test]
fn a_rect_one_cell_wide_costs_a_cell_it_does_not_have_so_nothing_is_drawn() {
    // Every frame drops the cell after a symbol wider than its column, and an
    // escape is as wide as the characters it prints. The seam buys that cell with
    // a margin two columns wide (`render::GAP_COLUMNS`); a rect with no second
    // column would have to buy it off the frame beside it, so it is not drawn at
    // all and the frame keeps the glyphs it had there.
    let (mut placement, mut buf) = placed();
    placement.place(&image(), Rect::new(5, 4, 1, 2), 0, &mut buf);
    assert!(
        escapes(&buf).is_empty(),
        "a rect with no cell to spend writes no escape: {:?}",
        escapes(&buf)
    );
}

#[test]
fn an_image_too_big_for_one_escape_is_sent_in_chunks_and_only_the_last_one_ends_it() {
    // A gradient is a few kilobytes of PNG, so the protocol's own 4096-character
    // ceiling is what splits it: every escape but the last says `m=1`, and the
    // last says `m=0` — a terminal still waiting on an image draws none of it.
    let noise = RgbaImage::from_fn(160, 160, |x, y| {
        image::Rgba([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8, 255])
    });
    let (mut placement, mut buf) = placed();
    placement.place(&noise, at(), 0, &mut buf);
    let sequence = &escapes(&buf)[0];
    let (sent, placed) = sequence
        .split_once("\u{1b}_Ga=p,")
        .expect("an image is placed once it is sent");
    assert!(
        placed.contains("C=1"),
        "and its place never moves the cursor"
    );
    let chunks: Vec<&str> = sent.split("\u{1b}_G").skip(1).collect();
    assert!(
        chunks.len() > 1,
        "an image past one escape's worth of base64 is several: {} chunks",
        chunks.len()
    );
    assert!(
        chunks[0].contains("a=T,f=100,") && chunks[0].contains("m=1;"),
        "the first escape names the image and promises more: {:?}",
        chunks[0]
    );
    for chunk in &chunks[1..chunks.len() - 1] {
        assert!(
            chunk.starts_with("m=1;"),
            "a middle escape says only that more is coming: {chunk:?}"
        );
    }
    let last = chunks[chunks.len() - 1];
    assert!(last.starts_with("m=0;"), "and the last ends it: {last:?}");
    for chunk in &chunks {
        let payload = chunk
            .split_once(';')
            .expect("every escape carries a payload")
            .1
            .trim_end_matches("\u{1b}\\");
        assert!(
            payload.chars().count() <= CHUNK,
            "no escape carries more than {CHUNK} characters of base64: {}",
            payload.chars().count()
        );
    }
}

#[test]
fn tmux_believes_no_terminal_that_it_can_draw_directly() {
    assert!(
        !speaks(&Graphics::kitty(), true),
        "a Kitty terminal inside tmux reads as one and is not: an escape is swallowed there, \
         so graphics go off rather than wrong"
    );
    assert!(
        speaks(&Graphics::kitty(), false),
        "and outside tmux the answer is the answer"
    );
    assert!(
        !speaks(&not_kitty(), false),
        "which is still a Kitty question"
    );
}
