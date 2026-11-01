//! A fence's laws in the store: a diagram is measured from the svg the renderer
//! writes, rendered like any other source once it has, and keyed by its own text
//! rather than by a file no one wrote. No frame lays one out: the store is asked
//! for a body it has not been handed, answers that it is pending, and is handed
//! the drawing by the loop — which is what `drawn` below stands in for.

use super::*;

use crate::picture::svgs::SVGS;
use crate::picture::{DIAGRAM_ROWS, MAX_ROWS, draw as drawn};

/// The store a test asks: the made-up Kitty answer a layout test uses. A fence
/// needs no workspace, so none is offered.
fn store() -> Store {
    Store::new(Some(&Graphics::kitty()), ".").expect("a Kitty answer is a store")
}

/// A diagram the crate draws: flat and wide, so the block is its own width.
const DIAGRAM: &str = "stateDiagram-v2\n    [*] --> Resting\n    Resting --> [*]\n";

/// Another diagram, of another shape, in a block of its own.
const OTHER: &str = "flowchart LR\n    A[fence] --> B[diagram]\n    B --> C[cells]\n";

/// A diagram drawn top to bottom and long with it: ten nodes, one under the
/// next, whose height is the dimension that carries its words. Committed beside
/// the crate so the store, the writer and the frame all measure one diagram.
const PORTRAIT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/diagrams/ten-nodes.mmd"
));

/// A diagram no parser can read: an edge with nothing on the far side.
const BROKEN: &str = "flowchart TD\n    A --> \n";

/// The same fence after an agent rewrote it: a node renamed, so the body is not
/// the one the store already drew.
const REWRITTEN: &str = "stateDiagram-v2\n    [*] --> Running\n    Running --> [*]";

/// A fence body that names its own node, so `n` of them are `n` diagrams.
fn fence(n: usize) -> String {
    format!("stateDiagram-v2\n    [*] --> Node{n}\n    Node{n} --> [*]")
}

/// The drawings the store has asked for, made here as the loop makes them — on a
/// thread of its own there, on this one here — and handed back. A test has no
/// loop, so it is the loop, and a body drawn once is never asked for again.
fn draw(store: &Store) -> Vec<String> {
    let asked = store.take_wanted();
    for body in &asked {
        store.keep(body, drawn(body));
    }
    asked
}

/// What a fence is worth once its drawing has been made: the measure that asks,
/// the drawing, and the measure that has it.
fn measured(store: &Store, body: &str, columns: u16) -> Worth {
    let worth = store.fence(body, columns);
    draw(store);
    match worth {
        Worth::Pending => store.fence(body, columns),
        drawn => drawn,
    }
}

#[test]
fn a_fence_is_measured_from_the_svg_the_renderer_draws() {
    let worth = measured(&store(), DIAGRAM, 60);
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
    let first = measured(&store, DIAGRAM, 60);
    assert!(store.protocol(&first).is_some(), "the diagram is drawable");
    assert_eq!(store.decodes, 1, "the first draw renders it");
    store.protocol(&first);
    assert_eq!(
        store.decodes, 1,
        "the same body in the same block is the same pixels"
    );

    let other = measured(&store, OTHER, 60);
    store.protocol(&other);
    assert_eq!(
        store.decodes, 2,
        "a different diagram is a render of its own"
    );

    // The same body offered a narrower column is drawn into another block, so
    // it is another picture rather than the one already held.
    let narrow = measured(&store, DIAGRAM, 12);
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
    // The frame that first lays the fence out asks and is handed nothing: the
    // parser has not spoken yet, and silence is not a refusal.
    assert_eq!(
        store.fence(BROKEN, 60),
        Worth::Pending,
        "a body no thread has drawn yet is pending"
    );
    let worth = measured(&store, BROKEN, 60);
    assert!(
        matches!(worth, Worth::Refused(Refusal::Diagram(_))),
        "a broken fence is the parser's own typed reason: {worth:?}"
    );
    assert!(store.protocol(&worth).is_none(), "nothing was measured");
    assert_eq!(store.decodes, 0, "and nothing was rendered");
    assert_eq!(
        store.renders(),
        1,
        "the reason is held, so the frame that asks again renders nothing"
    );
    measured(&store, BROKEN, 60);
    assert_eq!(
        store.renders(),
        1,
        "and a repaint asks the renderer nothing"
    );
}

#[test]
fn a_repaint_of_a_frame_full_of_diagrams_renders_nothing() {
    let store = store();
    let bodies: Vec<String> = (0..4).map(fence).collect();
    for body in &bodies {
        assert_eq!(
            store.fence(body, 60),
            Worth::Pending,
            "the first frame asks and lays nothing out"
        );
    }
    assert_eq!(
        store.renders(),
        0,
        "and no diagram was put through the renderer to ask"
    );
    assert_eq!(draw(&store).len(), 4, "the loop draws the four bodies");
    assert_eq!(store.renders(), 4, "each diagram once, and never twice");
    for body in &bodies {
        assert!(
            matches!(store.fence(body, 60), Worth::Drawn { .. }),
            "a body the store holds is a diagram"
        );
    }
    // The reader scrolls past all four and back: the whole frame is measured
    // again, at the width it was measured at, and no diagram is drawn twice.
    for body in &bodies {
        measured(&store, body, 60);
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
    let narrow = measured(&store, DIAGRAM, 8);
    let wide = measured(&store, DIAGRAM, 80);
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
    measured(&store, DIAGRAM, 60);
    assert_eq!(store.renders(), 1, "a body is rendered once");
    measured(&store, DIAGRAM, 60);
    assert_eq!(
        store.renders(),
        1,
        "and the same body is never rendered twice"
    );
    measured(&store, REWRITTEN, 60);
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
        measured(&store, body, 60);
    }
    assert_eq!(
        store.renders(),
        SVGS as u64,
        "a screenful of diagrams and the ones a scroll reaches next are held"
    );
    // The reader scrolls back to the top of the thread: a hit, which is what
    // makes that diagram the most recently used of the lot.
    measured(&store, &bodies[0], 60);
    assert_eq!(
        store.renders(),
        SVGS as u64,
        "and costs nothing to draw again"
    );

    measured(&store, &fence(SVGS), 60);
    assert_eq!(
        store.renders(),
        SVGS as u64 + 1,
        "one body past the bound makes room for itself"
    );
    measured(&store, &bodies[0], 60);
    assert_eq!(
        store.renders(),
        SVGS as u64 + 1,
        "and the diagram the reader scrolled back to is still held"
    );

    // The one above it was the least recently used of the lot, so it was let go:
    // the frame that lays it out again has no drawing for it and asks afresh,
    // and the panel stands until the answer lands.
    assert_eq!(
        store.fence(&bodies[1], 60),
        Worth::Pending,
        "a diagram the store let go is not one it still holds"
    );
    measured(&store, &bodies[1], 60);
    assert_eq!(
        store.renders(),
        SVGS as u64 + 2,
        "and it is put through the renderer once more"
    );
}

#[test]
fn a_portrait_diagram_takes_the_rows_a_diagram_may_and_a_photograph_does_not() {
    // The same question asked of the two: a photograph is landscape and shrinks
    // gracefully, a flowchart drawn top to bottom does not. At sixty columns
    // the ten-node diagram is 585×984 pixels of its own, so 480 pixels wide is
    // 808 tall: forty-five rows of eighteen. It takes the diagram's allowance
    // and stops there, where tall.png — ten pixels by five thousand — takes a
    // picture's twenty.
    let worth = measured(&store(), PORTRAIT.trim_end_matches('\n'), 60);
    assert_eq!(
        worth.rows(),
        DIAGRAM_ROWS,
        "a portrait diagram takes the taller allowance: {worth:?}"
    );
    assert!(
        worth.rows() > MAX_ROWS,
        "and the allowance is what a picture may not take"
    );
    assert_eq!(
        Source::File("a chart".into()).allowance(),
        MAX_ROWS,
        "and a file's allowance did not move"
    );
}

#[test]
fn a_portrait_diagram_in_a_narrow_body_takes_the_rows_the_column_gives_it() {
    // The acceptance of #120 is a forty-column body. There the column binds and
    // the allowance does not: 320 pixels wide is 0.55 of the diagram's own
    // width, so 538 pixels tall — thirty rows of eighteen, at 7.7 pixels of
    // text where the twenty the old cap allowed drew 5.1. The allowance is a
    // ceiling and never a target.
    let worth = measured(&store(), PORTRAIT.trim_end_matches('\n'), 40);
    assert_eq!(
        worth.rows(),
        30,
        "the column, not the allowance, is what drew this one: {worth:?}"
    );
    assert!(
        worth.rows() < DIAGRAM_ROWS,
        "so it never reached the ceiling"
    );
}
