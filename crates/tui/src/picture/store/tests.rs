//! A fence's laws in the store: a diagram is measured from the svg the renderer
//! would write and rendered like any other source, and it is keyed by its own
//! text rather than by a file no one wrote.

use super::*;

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
