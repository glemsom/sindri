//! The diagram's own laws: every ink is the frame's, and the pixels are drawn
//! on the panel the fence would have sat on.

use super::*;

/// A diagram the crate draws: flat and wide, so nothing is scaled to nothing.
const DIAGRAM: &str = "stateDiagram-v2\n    [*] --> Resting\n    Resting --> [*]\n";

/// Every colour of the renderer's theme, by the field it is named in.
fn inks(theme: &Theme) -> Vec<(&'static str, String)> {
    let mut named = vec![
        ("background", theme.background.clone()),
        ("text_color", theme.text_color.clone()),
        ("primary_text_color", theme.primary_text_color.clone()),
        ("primary_color", theme.primary_color.clone()),
        ("primary_border_color", theme.primary_border_color.clone()),
        ("line_color", theme.line_color.clone()),
        ("secondary_color", theme.secondary_color.clone()),
        ("tertiary_color", theme.tertiary_color.clone()),
        ("edge_label_background", theme.edge_label_background.clone()),
        ("cluster_background", theme.cluster_background.clone()),
        ("cluster_border", theme.cluster_border.clone()),
        ("sequence_actor_fill", theme.sequence_actor_fill.clone()),
        ("sequence_actor_border", theme.sequence_actor_border.clone()),
        ("sequence_actor_line", theme.sequence_actor_line.clone()),
        ("sequence_note_fill", theme.sequence_note_fill.clone()),
        ("sequence_note_border", theme.sequence_note_border.clone()),
        (
            "sequence_activation_fill",
            theme.sequence_activation_fill.clone(),
        ),
        (
            "sequence_activation_border",
            theme.sequence_activation_border.clone(),
        ),
        ("pie_title_text_color", theme.pie_title_text_color.clone()),
        (
            "pie_section_text_color",
            theme.pie_section_text_color.clone(),
        ),
        ("pie_legend_text_color", theme.pie_legend_text_color.clone()),
        ("pie_stroke_color", theme.pie_stroke_color.clone()),
        (
            "pie_outer_stroke_color",
            theme.pie_outer_stroke_color.clone(),
        ),
    ];
    named.extend(
        theme
            .pie_colors
            .iter()
            .map(|slice| ("pie_colors", slice.clone())),
    );
    named
}

/// A `#rrggbb` the renderer parses, as the colour the frame checks.
fn colour(ink: &str) -> Color {
    let digits = ink.strip_prefix('#').expect("an ink is a hex colour");
    assert_eq!(digits.len(), 6, "{ink} is six digits");
    let byte = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).expect("hex digits");
    Color::Rgb(byte(0), byte(2), byte(4))
}

#[test]
fn every_ink_a_diagram_is_drawn_in_is_a_colour_the_frame_names() {
    let theme = anvil();
    let mut seen = 0;
    for (field, ink) in inks(&theme) {
        assert!(
            crate::theme::owns(colour(&ink)),
            "{field} is drawn in {ink}, which the frame never paints"
        );
        seen += 1;
    }
    assert_eq!(seen, 35, "twenty-four fields, twelve of them slices");
    assert_eq!(theme.font_family, "DejaVu Sans", "the box's one font");
    assert_eq!(
        theme.font_size, 14.0,
        "left at the crate's default: the fit is the lever, not the size"
    );
}

#[test]
fn a_diagram_is_rasterised_on_the_panels_ground() {
    let shape = render(DIAGRAM).expect("a state diagram parses");
    let (width, height) = shape.size;
    assert!(width > 1 && height > 1, "it is a picture of a size");

    let image = raster(&shape, 160, 90).expect("it rasterises");
    let pixels = image.to_rgba8();
    assert_eq!((pixels.width(), pixels.height()), (160, 90));

    let ground = pixels.get_pixel(0, 0);
    let Color::Rgb(r, g, b) = theme::PANEL else {
        panic!("the panel is truecolour");
    };
    assert_eq!(
        ground.0,
        [r, g, b, 255],
        "a corner no shape reached is the panel the fence would sit on"
    );
    assert!(
        pixels.pixels().any(|pixel| pixel.0 != ground.0),
        "and the diagram was drawn on it"
    );
}

#[test]
fn a_diagram_is_drawn_with_the_fonts_the_theme_names() {
    // The same diagram, drawn with no font at all. Shapes and ground are the
    // same either way, so pixels that differ are the words: a box with no ink
    // in it would be a diagram the reader cannot read.
    let shape = render(DIAGRAM).expect("a state diagram parses");
    let with = raster(&shape, 160, 90).expect("it rasterises");
    let without = drawn(&shape.svg, 160, 90, &usvg::Options::default()).expect("it rasterises");
    assert_ne!(
        with.to_rgba8().as_raw(),
        without.to_rgba8().as_raw(),
        "the diagram's words are drawn with a font, not left out"
    );
}

#[test]
fn a_fence_that_will_not_parse_is_the_parsers_own_reason() {
    let Err(Refusal::Diagram(reason)) = render("flowchart TD\n    A --> \n") else {
        panic!("a broken fence draws no svg");
    };
    assert!(!reason.is_empty(), "and the reason says what it was");
}
