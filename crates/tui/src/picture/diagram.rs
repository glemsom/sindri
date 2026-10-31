//! A mermaid fence's diagram, drawn in Anvil's inks: the renderer's `Theme` is
//! the frame's palette field by field, the svg it writes is measured before a
//! pixel is spent, and the same svg is rasterised onto the panel's ground when
//! the frame asks for the pixels. No hex is written here — every colour is read
//! out of `theme.rs` — so a diagram cannot drift from the frame it sits in.
//!
//! It is the first picture in the frame whose contents are not the frame's: the
//! `Theme` below is what we choose, and what the renderer draws with it — every
//! node, every antialiased edge — is the renderer's to settle. Ours is the
//! ground, `PANEL` as `background` and under the pixmap both, so the block's is
//! the panel the fence would have sat on and never mermaid's white; ours are the
//! inks a diagram is read by. The contrast between a node's fill and the text in
//! it is mermaid's: handing it `RAISED` and `PLAIN` is this mapping's whole job,
//! and the palette's floor answers for the pair (`tests/theme`). Every field the
//! renderer names is set below, so a second palette is a change to `theme.rs`
//! alone; the hues the crate itself writes for diagram kinds that have no field
//! — an ER diagram's key chips, a gantt's grid — are the renderer's, the way its
//! antialiasing is, and no palette reaches them.

use std::sync::{Arc, OnceLock};

use image::DynamicImage;
use mermaid_rs_renderer::{LayoutConfig, RenderOptions, Theme, render_strict};
use ratatui::style::Color;
use resvg::{tiny_skia, usvg};

use crate::picture::Refusal;
use crate::theme;

/// The wheel a diagram spins a series on: twelve palette inks, ordered so that
/// neighbours differ in hue and lightness both. A pie's slices, a git graph's
/// branches and the commit a merge is highlighted with all take it, so a second
/// colour in a diagram is never a colour the frame's palette does not name.
const WHEEL: [Color; 12] = [
    theme::PROMPT,
    theme::NUMBER,
    theme::STRING,
    theme::HEADING,
    theme::CODE,
    theme::TYPE,
    theme::BAD,
    theme::FUNCTION,
    theme::GOOD,
    theme::KEYWORD,
    theme::PLAIN,
    theme::SEAM,
];

/// The renderer's theme: `modern()` with the frame's own inks in place of
/// mermaid's, so a diagram reads as a card in the transcript and not as a light
/// theme pasted onto it. Every field the renderer names as a colour is set
/// here; the sizes it names are left at its own, because the block fits a
/// diagram to the column and a larger one is shrunk by the same factor.
fn anvil() -> Theme {
    let mut theme = Theme::modern();
    // The crate's own list opens with a font no Linux has, which falls back
    // silently and differently per box; the face's box has exactly one.
    theme.font_family = "DejaVu Sans".to_string();
    // The ground, and the ink on it: the svg paints an opaque rectangle behind
    // the whole diagram, so this is the block's background — the panel the
    // fence would have sat on, and never the renderer's own white.
    theme.background = ink(theme::PANEL);
    theme.text_color = ink(theme::PLAIN);
    theme.primary_text_color = ink(theme::PLAIN);
    // A node is the tier above the panel it is drawn on; its edge and every
    // line between two of them is the frame's prompt ink.
    theme.primary_color = ink(theme::RAISED);
    theme.primary_border_color = ink(theme::PROMPT);
    theme.line_color = ink(theme::PROMPT);
    // The two tiers under a node, for the diagrams that stack three shades.
    theme.secondary_color = ink(theme::PANEL);
    theme.tertiary_color = ink(theme::CANVAS);
    // A label sits on the line it interrupts, so its backing is the panel.
    theme.edge_label_background = ink(theme::PANEL);
    // A subgraph is a fence around nodes: a ground a step under the panel, and
    // an edge quieter than a node's so it does not compete with what it holds.
    theme.cluster_background = ink(theme::CANVAS);
    theme.cluster_border = ink(theme::FAINT);
    // A sequence's actors are nodes, its lifelines are lines, and an activation
    // is an outline rather than a third fill tier.
    theme.sequence_actor_fill = ink(theme::RAISED);
    theme.sequence_actor_border = ink(theme::PROMPT);
    theme.sequence_actor_line = ink(theme::PROMPT);
    theme.sequence_activation_fill = ink(theme::PANEL);
    theme.sequence_activation_border = ink(theme::PROMPT);
    // A note is a raised box with the ink prose gives a code word: the one
    // thing on the canvas that is neither a line nor a node.
    theme.sequence_note_fill = ink(theme::RAISED);
    theme.sequence_note_border = ink(theme::CODE);
    theme.pie_colors = WHEEL.map(ink);
    // Every slice is a light ink, so the label inside it is the canvas; the
    // seam between two slices is the canvas they sit on and the rim is a line.
    theme.pie_section_text_color = ink(theme::CANVAS);
    theme.pie_title_text_color = ink(theme::PLAIN);
    theme.pie_legend_text_color = ink(theme::PLAIN);
    theme.pie_stroke_color = ink(theme::PANEL);
    theme.pie_outer_stroke_color = ink(theme::PROMPT);
    // A git graph spins the same wheel, and the square that marks a commit is
    // the ink opposite its branch's, so a highlight is never the colour under
    // it. A branch's label is read on a wheel ink, so it is the canvas, as a
    // slice's label is; a commit's message is a label masking the line it sits
    // on, and a tag is a raised box with the node's ink and a line's rim.
    theme.git_colors = core::array::from_fn(|at| ink(WHEEL[at]));
    theme.git_inv_colors = core::array::from_fn(|at| ink(WHEEL[(at + 6) % WHEEL.len()]));
    theme.git_branch_label_colors = core::array::from_fn(|_| ink(theme::CANVAS));
    theme.git_commit_label_color = ink(theme::PLAIN);
    theme.git_commit_label_background = ink(theme::PANEL);
    theme.git_tag_label_color = ink(theme::PLAIN);
    theme.git_tag_label_background = ink(theme::RAISED);
    theme.git_tag_label_border = ink(theme::PROMPT);
    // Opaque: a slice at seven tenths over the panel composites to a hue no
    // palette names.
    theme.pie_opacity = 1.0;
    theme
}

fn options() -> RenderOptions {
    RenderOptions {
        theme: anvil(),
        layout: LayoutConfig::default(),
    }
}

/// One rendered diagram: the svg the renderer wrote for a fence's body, and the
/// size it says it is. The svg is kept rather than the pixels because drawing
/// one and measuring one are the same drawing — the store measures an entry
/// every frame and draws it once — so a body costs the renderer once, whatever
/// the frame asks of it afterwards.
pub(super) struct Shape {
    pub(super) svg: String,
    /// The size in pixels, read off the svg's own attributes: no font is loaded
    /// to learn it, and the walk over fontconfig is the raster's business.
    pub(super) size: (u32, u32),
}

/// The diagram a fence's body draws, or the parser's own reason there is none.
pub(super) fn render(body: &str) -> Result<Shape, Refusal> {
    let svg = render_strict(body, options()).map_err(diagnostic)?;
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).map_err(diagnostic)?;
    let size = tree.size().to_int_size();
    Ok(Shape {
        svg,
        size: (size.width(), size.height()),
    })
}

/// The pixels a rendered diagram draws at `width` by `height`, on the panel's
/// ground: the whole diagram, fitted and never drawn past its own pixels.
pub(super) fn raster(shape: &Shape, width: u32, height: u32) -> Option<DynamicImage> {
    let mut fonts = usvg::Options::default();
    *fonts.fontdb_mut() = fonts_system().as_ref().clone();
    drawn(&shape.svg, width, height, &fonts)
}

/// The svg drawn, with whatever fonts the caller brought: a diagram's words are
/// the one part of it that needs a font database, so the ground and the shapes
/// are drawn with or without one and only the words are not.
fn drawn(svg: &str, width: u32, height: u32, fonts: &usvg::Options<'_>) -> Option<DynamicImage> {
    let tree = usvg::Tree::from_str(svg, fonts).ok()?;
    let natural = tree.size();
    let (width, height) = (width.max(1), height.max(1));
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    pixmap.fill(colour(theme::PANEL));
    let scale =
        (width as f32 / natural.width().max(1.0)).min(height as f32 / natural.height().max(1.0));
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // Demultiplied, because the pixels leave for an image crate that expects
    // straight alpha — and the ground is opaque, so nothing was multiplied.
    let pixels = image::RgbaImage::from_raw(width, height, pixmap.take_demultiplied())?;
    Some(DynamicImage::ImageRgba8(pixels))
}

/// The system's fonts, walked once: a diagram's text is drawn with the font the
/// theme names, and re-reading fontconfig for every diagram of an answer would
/// cost more than the diagram.
fn fonts_system() -> &'static Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut database = usvg::fontdb::Database::new();
        database.load_system_fonts();
        Arc::new(database)
    })
}

/// What the parser said, as a reason the frame can print under the source.
fn diagnostic(error: impl std::fmt::Display) -> Refusal {
    Refusal::Diagram(error.to_string())
}

/// The `#rrggbb` the renderer parses, out of a colour the frame names: the
/// digits are `theme.rs`'s and never a literal.
fn ink(colour: Color) -> String {
    let Color::Rgb(r, g, b) = colour else {
        panic!("a diagram is drawn in the theme's truecolour");
    };
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The same colour, as the ground a pixmap is filled with.
fn colour(named: Color) -> tiny_skia::Color {
    let Color::Rgb(r, g, b) = named else {
        panic!("a diagram is drawn in the theme's truecolour");
    };
    tiny_skia::Color::from_rgba8(r, g, b, 255)
}

#[cfg(test)]
mod tests;
