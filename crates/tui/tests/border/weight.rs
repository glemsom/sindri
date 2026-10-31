//! The weight the stroke spends: a hairline at the quiet end, a taper to the
//! lamp over the last third, and never a pixel heavier than it needs.

use super::*;

#[test]
fn the_rule_is_a_hairline_with_a_faint_neighbour_and_not_a_block() {
    let frame = frame();
    let quiet = frame.width() / 3;
    assert_eq!(
        alpha(&frame, quiet, 0),
        255,
        "a pixel on the frame's own edge is the stroke, at full opacity"
    );
    assert_eq!(
        [1, 2, 3].map(|y| alpha(&frame, quiet, y)),
        [0, 0, 0],
        "the quiet end is one pixel and nothing behind it, at 8 x 18 cells"
    );
    let w = frame.width();
    let lamp = frame.height() / 2;
    assert_eq!(
        [0, 1, 2].map(|back| alpha(&frame, w - 1 - back, lamp)),
        [255, 255, 255],
        "the lamp is three whole pixels"
    );
    // The fourth is what the lamp has over three whole ones: the 40 per cent
    // neighbour that makes the stroke read as a taper and not as a block.
    let neighbour = f32::from(alpha(&frame, w - 4, lamp)) / 255.0;
    assert!(
        (neighbour + 3.0 - border::LAMP).abs() < 0.06,
        "three pixels and a neighbour of {neighbour:.2} make the lamp's \
         {LAMP}, no more and no less",
        LAMP = border::LAMP
    );
    assert_eq!(
        alpha(&frame, w - 5, lamp),
        0,
        "and that is where the stroke stops"
    );
}

#[test]
fn the_rule_only_ever_thickens_toward_the_lamp() {
    let frame = frame();
    let cut = border::CHAMFER;
    // The label's cells are air, so the rule is read either side of the cut.
    let gap =
        (u32::from(LABEL.start) * u32::from(CELL.0))..(u32::from(LABEL.end) * u32::from(CELL.0));
    let widths: Vec<f32> = (cut + 2..frame.width() - cut - 2)
        .filter(|x| !gap.contains(x))
        .map(|x| down(&frame, x))
        .collect();
    assert!(
        widths.windows(2).all(|pair| pair[1] + 0.02 >= pair[0]),
        "the stroke never thins along a rule: {widths:?}"
    );
    let (first, last) = (
        *widths.first().expect("a quiet end"),
        *widths.last().expect("a lit end"),
    );
    assert!(
        last > first + 2.0,
        "and it arrives at the lamp heavier than it left the quiet end: {first} -> {last}"
    );
}

#[test]
fn the_first_two_thirds_of_a_rule_are_flat_and_the_last_third_is_the_taper() {
    let frame = frame();
    let after = u32::from(LABEL.end) * u32::from(CELL.0) + border::PAD + 2;
    for x in [after, frame.width() / 2, frame.width() * 2 / 3] {
        assert!(
            (border::QUIET - 0.01..=border::QUIET + 0.01).contains(&down(&frame, x)),
            "two thirds of the rule is one pixel wide, and it is {x}: {}",
            down(&frame, x)
        );
    }
    assert!(
        ((border::LAMP - 0.1)..=(border::LAMP + 0.001))
            .contains(&across(&frame, frame.height() / 4)),
        "the right vertical is the lamp's own weight, and it walks between its two \
         corners at that weight: {}",
        across(&frame, frame.height() / 4)
    );
}
