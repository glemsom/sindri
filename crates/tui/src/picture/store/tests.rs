//! A fence's laws in the store: a diagram is measured from the svg the renderer
//! would write and rendered like any other source, and it is keyed by its own
//! text rather than by a file no one wrote.

use super::*;

use crate::picture::svgs::SVGS;

/// The store a test asks: the made-up Kitty answer a layout test uses. A fence
/// needs no workspace, so none is offered.
fn store() -> Store {
    Store::new(Some(&Graphics::kitty()), ".").expect("a Kitty answer is a store")
}

/// A diagram the crate draws: flat and wide, so the block is its own width.
const DIAGRAM: &str = "stateDiagram-v2\n    [*] --> Resting\n    Resting --> [*]\n";

/// Another diagram, of another shape, in a block of its own.
const OTHER: &str = "flowchart LR\n    A[fence] --> B[diagram]\n    B --> C[cells]\n";

/// A diagram no parser can read: an edge with nothing on the far side.
const BROKEN: &str = "flowchart TD\n    A --> \n";

/// The same fence after an agent rewrote it: a node renamed, so the body is not
/// the one the store already drew.
const REWRITTEN: &str = "stateDiagram-v2\n    [*] --> Running\n    Running --> [*]";

/// A fence body that names its own node, so `n` of them are `n` diagrams.
fn fence(n: usize) -> String {
    format!("stateDiagram-v2\n    [*] --> Node{n}\n    Node{n} --> [*]")
}

#[test]
fn a_fence_is_measured_from_the_svg_the_renderer_draws() {
    let worth = store().fence(DIAGRAM, 60);
    let Worth::Drawn {
        source,
        columns,
        rows,
    } = worth
    else {
        panic!("a state diagram is drawn: {worth:?}");
    };
    assert_eq!(
        source,
        Source::Fence(DIAGRAM.to_string()),
        "a fence is its own source"
    );
    assert!(rows >= 1, "it takes rows of the column");
    assert!(columns <= 60, "and never more columns than it was offered");
}

#[test]
fn a_fence_is_keyed_by_its_body_and_the_block_it_is_drawn_into() {
    let mut store = store();
    let worth = store.fence(DIAGRAM, 60);
    assert!(store.protocol(&worth).is_some(), "the diagram is drawable");
    assert_eq!(store.decodes, 1, "the first draw renders it");
    store.protocol(&worth);
    assert_eq!(
        store.decodes, 1,
        "the same body in the same block is the same pixels"
    );

    let other = store.fence(OTHER, 60);
    store.protocol(&other);
    assert_eq!(
        store.decodes, 2,
        "a different diagram is a render of its own"
    );

    // The same body offered a narrower column is drawn into another block, so
    // it is another picture rather than the one already held.
    let narrow = store.fence(DIAGRAM, 12);
    assert_ne!(narrow.rows(), 0, "the diagram still fits a narrow column");
    store.protocol(&narrow);
    assert_eq!(store.decodes, 3, "another block is another render");
    assert_eq!(
        store.renders(),
        2,
        "but a resized terminal re-rasters two diagrams and re-renders neither"
    );
}

#[test]
fn a_fence_that_will_not_parse_has_nothing_to_draw() {
    let mut store = store();
    let worth = store.fence(BROKEN, 60);
    assert!(
        matches!(worth, Worth::Refused(Refusal::Diagram(_))),
        "a broken fence is the parser's own typed reason: {worth:?}"
    );
    assert!(store.protocol(&worth).is_none(), "nothing was measured");
    assert_eq!(store.decodes, 0, "and nothing was rendered");
}

#[test]
fn a_repaint_of_a_frame_full_of_diagrams_renders_nothing() {
    let store = store();
    let bodies: Vec<String> = (0..4).map(fence).collect();
    for body in &bodies {
        assert!(
            matches!(store.fence(body, 60), Worth::Drawn { .. }),
            "the store draws a diagram of a fence"
        );
    }
    assert_eq!(
        store.renders(),
        4,
        "the first frame renders each diagram once"
    );
    // The reader scrolls past all four and back: the whole frame is measured
    // again, at the width it was measured at, and no diagram is drawn twice.
    for body in &bodies {
        store.fence(body, 60);
    }
    assert_eq!(
        store.renders(),
        4,
        "a repaint of the same frame renders nothing"
    );
}

#[test]
fn the_same_fence_body_at_two_column_widths_is_rendered_once_and_measured_twice() {
    let store = store();
    let (narrow, wide) = (store.fence(DIAGRAM, 8), store.fence(DIAGRAM, 80));
    assert_eq!(
        store.renders(),
        1,
        "the svg is keyed by source alone: the column mapping is the block's job"
    );
    assert!(
        wide.columns() > narrow.columns(),
        "each column width measures its own block: {} against {}",
        narrow.columns(),
        wide.columns()
    );
    assert_ne!(narrow.rows(), wide.rows(), "and its own rows");
}

#[test]
fn a_rewritten_fence_body_is_rendered_again() {
    let store = store();
    store.fence(DIAGRAM, 60);
    assert_eq!(store.renders(), 1, "a body is rendered once");
    store.fence(DIAGRAM, 60);
    assert_eq!(
        store.renders(),
        1,
        "and the same body is never rendered twice"
    );
    store.fence(REWRITTEN, 60);
    assert_eq!(
        store.renders(),
        2,
        "a cache that never misses is a cache that lies"
    );
}

#[test]
fn the_svg_cache_lets_go_the_least_recently_used_and_not_the_oldest() {
    let store = store();
    let bodies: Vec<String> = (0..SVGS).map(fence).collect();
    for body in &bodies {
        store.fence(body, 60);
    }
    assert_eq!(
        store.renders(),
        SVGS as u64,
        "a screenful of diagrams and the ones a scroll reaches next are held"
    );
    // The reader scrolls back to the top of the thread: a hit, which is what
    // makes that diagram the most recently used of the lot.
    store.fence(&bodies[0], 60);
    assert_eq!(
        store.renders(),
        SVGS as u64,
        "and costs nothing to draw again"
    );

    store.fence(&fence(SVGS), 60);
    assert_eq!(
        store.renders(),
        SVGS as u64 + 1,
        "one body past the bound makes room for itself"
    );
    store.fence(&bodies[0], 60);
    assert_eq!(
        store.renders(),
        SVGS as u64 + 1,
        "and the diagram the reader scrolled back to is still held"
    );
    store.fence(&bodies[1], 60);
    assert_eq!(
        store.renders(),
        SVGS as u64 + 2,
        "while the one above it, least recently used, was let go"
    );
}
