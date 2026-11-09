//! A mermaid fence becomes a picture or is refused with a reason the caption can print. What is held is the
//! fence and never the drawing: a test says "it drew", by the ink of a raster and the count of raster passes
//! the seam keeps, and the corpus of real fences is an eval whose golden a reader approves.

use std::time::{Duration, Instant};

use sindri::graphics::CellSize;
use sindri::mermaid::{self, Block, Diagram, Fence, Refusal};

const FLOWCHART: &str =
    "flowchart TD\n  A[Start] --> B{Ready?}\n  B -->|yes| C[Ship]\n  B -->|no| A";

/// A fence the renderer will not read: a refusal a test can quote a reason from.
const PROSE: &str = "this is not a mermaid diagram";

fn inked(image: &image::RgbaImage) -> usize {
    image.pixels().filter(|pixel| pixel.0[3] > 0).count()
}

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

#[test]
fn a_fence_still_streaming_is_never_handed_to_the_renderer() {
    assert!(
        matches!(mermaid::block(PROSE, Fence::Streaming), Block::Streaming),
        "a fence still arriving renders nothing at all: had the renderer been asked, prose \
         would have come back as the refusal it is"
    );
}

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
