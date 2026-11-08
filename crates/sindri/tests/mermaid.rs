//! A mermaid fence becomes a picture or is refused with a reason the caption can print:
//! `mermaid-rs-renderer` draws the fence's source into an SVG in this process, the graphics
//! seam turns that SVG into pixels, and a fence that will not draw is the source it was
//! written as with one sentence under it saying why (§12, ADR-0003).
//!
//! What is held here is the fence, never the drawing: a picture is proved by the ink of a
//! raster and by the count of raster passes the seam keeps, so a test says "it drew" and
//! never "it drew this thousand pixels". The corpus of real fences, whose drawing is a
//! golden a reader approves, is an eval and not a test (§16).

use std::time::{Duration, Instant};

use sindri::graphics::CellSize;
use sindri::mermaid::{self, Block, Diagram, Fence, Refusal};

/// A fence a reader really writes, and the one every picture below is drawn from.
const FLOWCHART: &str =
    "flowchart TD\n  A[Start] --> B{Ready?}\n  B -->|yes| C[Ship]\n  B -->|no| A";

/// A fence the renderer will not read, in its own words: a test needs a refusal it can
/// quote a reason from, not just any failure.
const PROSE: &str = "this is not a mermaid diagram";

/// Every pixel of a raster that carries any ink at all.
fn inked(image: &image::RgbaImage) -> usize {
    image.pixels().filter(|pixel| pixel.0[3] > 0).count()
}

/// The first of #54's five: a fence a reader writes is drawn — the SVG is there, the cells
/// it takes are known before a pixel is spent, and spending them leaves a picture with ink
/// in it at the size the footprint bought (ADR-0003).
#[test]
fn a_real_fence_renders_to_a_picture_measured_before_a_pixel_is_spent() {
    let diagram = Diagram::of(FLOWCHART).expect("a fence a reader writes is drawn");
    assert_eq!(
        diagram.source(),
        FLOWCHART,
        "the diagram keeps the source it was drawn from, because that is what its block \
         copies as and what a reader sees when the picture cannot be placed"
    );
    assert!(
        diagram.svg().contains("<svg"),
        "the renderer answered with an SVG: {}",
        &diagram.svg()[..diagram.svg().len().min(120)]
    );

    let cell = CellSize::NOMINAL;
    let footprint = diagram
        .footprint(cell)
        .expect("a flowchart of four nodes fits a transcript");
    assert!(
        footprint.cols() > 1 && footprint.rows() > 1,
        "the diagram takes cells: {}x{}",
        footprint.cols(),
        footprint.rows()
    );
    assert_eq!(
        diagram.picture().rasterises(),
        0,
        "measuring a picture spends no pixel: a footprint that rasterised would cost the \
         transcript a raster pass per layout"
    );

    let image = diagram
        .rasterise(cell)
        .expect("the diagram draws at the size it was measured at");
    assert_eq!(
        image.dimensions(),
        footprint.pixels(cell),
        "the raster is drawn at the pixels the footprint bought, so the placement and the \
         picture cannot disagree about how big the diagram is"
    );
    assert!(
        inked(&image) > 1000,
        "the diagram's ink is in the picture: {} pixels of it",
        inked(&image)
    );
}

/// #54's second line at the renderer: a fence that will not parse is refused with the
/// renderer's own words, because a caption that says "it failed" is a caption no reader can
/// act on.
#[test]
fn a_fence_the_renderer_will_not_read_is_refused_in_the_renderers_own_words() {
    let refusal = Diagram::of(PROSE).expect_err("prose is not a diagram");
    assert!(
        matches!(refusal, Refusal::Unreadable { .. }),
        "a fence the parser rejects is unreadable rather than too slow or broken: {refusal:?}"
    );

    let message = refusal.message();
    assert!(
        message.contains("could not be drawn"),
        "the caption says what happened to the diagram: {message}"
    );
    assert!(
        message.to_lowercase().contains("diagram header"),
        "the caption carries the renderer's own reason, which is the only thing that tells \
         a writer which line was wrong: {message}"
    );
}

/// #54's fourth line: a render is given a wall clock and refused when it overruns, because a
/// pathological fence that hangs a single render hangs the whole transcript with it.
#[test]
fn a_render_that_outlasts_its_bound_is_refused_rather_than_hanging_the_transcript() {
    let bound = Duration::from_millis(20);
    let started = Instant::now();
    let outcome: Result<(), Refusal> = mermaid::within(bound, || {
        std::thread::sleep(Duration::from_millis(400));
    });

    assert_eq!(
        outcome.err(),
        Some(Refusal::TooSlow { bound }),
        "a render still running when its bound passes is refused, and the refusal carries \
         the bound it was given"
    );
    assert!(
        started.elapsed() < Duration::from_millis(300),
        "the caller waited {:?} and not the render's 400 ms: a bound that waits for the \
         work it is bounding is no bound at all",
        started.elapsed()
    );
}

/// A caller who names a wall clock gets its own: the eval draws a corpus on a shared runner
/// and needs more patience than a reader watching a frame, and the number it was given is the
/// number its refusal carries.
#[test]
fn a_render_is_refused_with_the_bound_its_caller_gave_it() {
    let bound = Duration::from_millis(1);
    let refusal = Diagram::of_within(bound, FLOWCHART)
        .expect_err("a fence is not drawn in the one millisecond it was given");

    assert_eq!(
        refusal,
        Refusal::TooSlow { bound },
        "the refusal is the caller's own bound and not the product's"
    );
}

/// The other half of the fourth line: a renderer that dies is a refusal too. A renderer that
/// panics on the thread the frame is painted on would take the frame with it.
#[test]
fn a_renderer_that_panics_is_a_refusal_and_not_a_crash() {
    let outcome: Result<(), Refusal> = mermaid::within(Duration::from_millis(500), || {
        panic!("the renderer gave up");
    });

    assert_eq!(
        outcome.err(),
        Some(Refusal::Broken),
        "a renderer that left without answering is a refusal, because a fence that cannot \
         be drawn is a caption and never a dead terminal"
    );
    assert!(
        !Refusal::Broken.message().is_empty(),
        "a refusal with no words is a diagram that silently disappeared"
    );
}

/// #54's third line: a fence whose closing line has not arrived is not a diagram. The
/// evidence is the source itself — prose the closed fence would refuse comes back as
/// `Streaming` and not as a refusal, which is only possible if the renderer was never asked.
#[test]
fn a_fence_still_streaming_is_never_handed_to_the_renderer() {
    assert!(
        matches!(mermaid::block(PROSE, Fence::Streaming), Block::Streaming),
        "a fence still arriving renders nothing at all: had the renderer been asked, prose \
         would have come back as the refusal it is"
    );
}

/// #54's first line at the seam a fence crosses: a closed fence that draws hands over the
/// picture, and one that does not hands over the one sentence its caption prints.
#[test]
fn a_closed_fence_hands_over_the_picture_or_the_reason_it_did_not_draw() {
    match mermaid::block(FLOWCHART, Fence::Closed) {
        Block::Picture(diagram) => assert_eq!(diagram.source(), FLOWCHART),
        other => panic!("a closed fence that parses is a picture, not {other:?}"),
    }

    match mermaid::block(PROSE, Fence::Closed) {
        Block::Refused { caption } => assert!(
            caption.contains("could not be drawn"),
            "the caption names the reason: {caption}"
        ),
        other => panic!("a closed fence that does not parse is a refusal, not {other:?}"),
    }
}
