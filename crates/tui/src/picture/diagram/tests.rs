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
    // A git graph's branches spin the same wheel, and the label on one is read
    // against it; these are the fields a `gitGraph` or a mindmap would otherwise
    // take mermaid's own luminous inks for.
    for (field, colours) in [
        ("git_colors", &theme.git_colors),
        ("git_inv_colors", &theme.git_inv_colors),
        ("git_branch_label_colors", &theme.git_branch_label_colors),
    ] {
        named.extend(colours.iter().map(|ink| (field, ink.clone())));
    }
    named.extend([
        (
            "git_commit_label_color",
            theme.git_commit_label_color.clone(),
        ),
        (
            "git_commit_label_background",
            theme.git_commit_label_background.clone(),
        ),
        ("git_tag_label_color", theme.git_tag_label_color.clone()),
        (
            "git_tag_label_background",
            theme.git_tag_label_background.clone(),
        ),
        ("git_tag_label_border", theme.git_tag_label_border.clone()),
    ]);
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
    assert_eq!(
        seen, 64,
        "every field the renderer names as a colour, and thirty of them a series"
    );
    assert_eq!(theme.font_family, "DejaVu Sans", "the box's one font");
    assert_eq!(
        theme.font_size, 14.0,
        "left at the crate's default: the fit is the lever, not the size"
    );
}

/// The ground a diagram is read on is the theme's and it is the panel: the svg
/// paints an opaque rectangle behind everything, so this field is the block's
/// background — a colour we chose, and not the renderer's own white, which on a
/// dark canvas is a light theme pasted onto the frame.
#[test]
fn a_diagram_sits_on_the_panel_and_never_on_the_renderers_white() {
    assert_eq!(
        anvil().background,
        ink(theme::PANEL),
        "the block's background is the panel the fence would have sat on, \
         in place of the white a renderer defaults to"
    );
}

/// The inks a diagram is *read* by are chosen here, and they are the frame's:
/// an edge is a line, a node's rim is chrome, a subgraph is quieter than what
/// it holds, and a label masks the line it interrupts with the panel. Which
/// colour sits where is this mapping's to say; that each is a palette name is
/// `every_ink_a_diagram_is_drawn_in_is_a_colour_the_frame_names`'s.
#[test]
fn the_lines_and_labels_a_diagram_is_read_by_are_the_frames_inks() {
    let theme = anvil();
    for (what, drawn, named) in [
        ("an edge", &theme.line_color, theme::PROMPT),
        ("a node's rim", &theme.primary_border_color, theme::PROMPT),
        ("a node", &theme.primary_color, theme::RAISED),
        ("a node's label", &theme.primary_text_color, theme::PLAIN),
        (
            "a label's backing",
            &theme.edge_label_background,
            theme::PANEL,
        ),
        ("a subgraph's rim", &theme.cluster_border, theme::FAINT),
        ("a lifeline", &theme.sequence_actor_line, theme::PROMPT),
    ] {
        assert_eq!(
            *drawn,
            ink(named),
            "{what} is drawn in {drawn}, and the theme names {named:?} for it"
        );
    }
}

#[test]
fn a_diagram_is_rasterised_on_the_panels_ground() {
    let shape = render(DIAGRAM).expect("a state diagram parses");
    let (width, height) = shape.size;
    assert!(width > 1 && height > 1, "it is a picture of a size");

    let image = raster(&shape, 160, 90).expect("it rasterises");
    let pixels = image.to_rgba8();
    assert_eq!((pixels.width(), pixels.height()), (160, 90));

    let Color::Rgb(r, g, b) = theme::PANEL else {
        panic!("the panel is truecolour");
    };
    let ground = [r, g, b, 255];
    let (right, bottom) = (pixels.width() - 1, pixels.height() - 1);
    for (x, y) in [(0, 0), (right, 0), (0, bottom), (right, bottom)] {
        assert_eq!(
            pixels.get_pixel(x, y).0,
            ground,
            "({x},{y}) is a corner no shape reached, and it is the panel the fence would sit on"
        );
    }
    assert!(
        pixels.pixels().any(|pixel| pixel.0 != ground),
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

/// A `gitGraph` is drawn on the same wheel as a pie, and it is the diagram that
/// would otherwise be drawn in mermaid's own luminous inks: every hex the svg
/// carries leads back to a colour the frame names.
#[test]
fn a_git_graph_is_drawn_in_the_palette_and_never_in_mermaids_inks() {
    let body = "gitGraph\n    commit\n    branch dev\n    commit\n    checkout main\n \
                merge dev\n    tag v1.0\n";
    let shape = render(body).expect("a git graph parses");
    let mut hexes: Vec<(usize, &str)> = Vec::new();
    let digits = shape.svg.as_bytes();
    let mut at = 0;
    while at + 1 < digits.len() {
        if digits[at] == b'#' {
            let start = at + 1;
            let mut end = start;
            while end < digits.len() && (digits[end] as char).is_ascii_hexdigit() {
                end += 1;
            }
            if end - start == 6 {
                hexes.push((at, &shape.svg[start..end]));
                at = end;
                continue;
            }
        }
        at += 1;
    }
    assert!(
        hexes.len() > 4,
        "the svg was drawn in colours: {}",
        hexes.len()
    );
    // The renderer's own inks are written as `hsl(...)` and `rgb(...)`; a form
    // this scan cannot read is a form mermaid's palette could slip through.
    for form in ["hsl(", "rgb("] {
        assert!(
            !shape.svg.contains(form),
            "the svg names a colour as `{form}`: an ink written down outside the theme"
        );
    }
    // Refused by its place in the drawing, the way `theme::owns_pixel` refuses a
    // pixel by its coordinates: the hue, and the words it was written among.
    for (at, hex) in hexes {
        let named = colour(&format!("#{hex}"));
        assert!(
            theme::owns(named),
            "the svg paints {named:?} at byte {at}, a hue the frame never names: {}",
            &shape.svg[at.saturating_sub(40)..(at + 40).min(shape.svg.len())]
        );
    }
}

#[test]
fn a_fence_that_will_not_parse_is_the_parsers_own_reason() {
    let Err(Refusal::Diagram(reason)) = render("flowchart TD\n    A --> \n") else {
        panic!("a broken fence draws no svg");
    };
    assert!(!reason.is_empty(), "and the reason says what it was");
}
