//! The composer's border as the frame draws it: the stroke `border.rs` paints,
//! placed on the composer's cells under the words. The picture is made once for
//! the rect and the ink it was made for and re-made when either changes — the
//! `Strip` lifecycle of `render/seam.rs` — because making it costs a full
//! raster of the panel and a transmit, and a frame that merely ticked must cost
//! neither. Nothing here decides what the stroke looks like: that is
//! `border::stroke`'s, a pure function of a rect, a cell, an ink and a label.

use std::ops::Range;

use image::RgbaImage;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::border::{self, Ink};
use crate::graphics::Graphics;
use crate::graphics::kitty::Placement;
use crate::theme;

/// The word the border carries: the one thing on the frame that is not a
/// stroke.
pub(crate) const TITLE: &str = " prompt ";
/// The columns of the rect the label takes, from its left edge: one column in,
/// which is where `Block::title` puts a left-aligned title, and the eight the
/// word and its air need. The picture cuts its gap for these cells, so the same
/// word lands in the same place whether it is cut into a stroke or drawn by a
/// block.
pub(crate) const LABEL: Range<u16> = 1..TITLE.len() as u16 + 1;

/// The composer's border as a picture: the stroke made for a rect and an ink,
/// and the placement that carries it.
pub(crate) struct Stroke {
    /// The terminal's cell in pixels, which is the only thing that turns a rect
    /// in cells into a picture.
    cell: (u16, u16),
    placement: Placement,
    /// The image as it was last made, and what it was made for: a frame at the
    /// same rect in the same ink re-places it rather than re-making it.
    made: Option<Made>,
}

/// One made stroke: the cells and the ink it was made for, and the pixels.
struct Made {
    cells: Rect,
    ink: Ink,
    image: RgbaImage,
}

impl Stroke {
    /// The stroke this terminal allows, or `None` where it draws no raw image:
    /// the question is `Placement`'s, and the answer holds for the frame's life.
    pub(crate) fn new(graphics: &Graphics) -> Option<Stroke> {
        Some(Stroke {
            cell: graphics.picker().font_size(),
            placement: Placement::new(graphics)?,
            made: None,
        })
    }

    /// Places the stroke over `area` and says whether it did: `false` where the
    /// picture would cost the frame a cell it has, and the caller draws its
    /// glyphs instead.
    pub(crate) fn draw(&mut self, area: Rect, ink: Ink, buf: &mut Buffer) -> bool {
        if area.width < 2 || area.height == 0 {
            return false;
        }
        let made = match self.made.take() {
            Some(made) if made.cells == area && made.ink == ink => made,
            _ => {
                let image = border::stroke(area, self.cell, ink, LABEL);
                // Pixels the terminal has never seen, at a rect it may well
                // have one for: `renew` is what tells the placement so.
                self.placement.renew();
                Made {
                    cells: area,
                    ink,
                    image,
                }
            }
        };
        // Layer `0`: the stroke is drawn where no glyph lives, and the label is
        // painted into the gap the picture leaves, never over.
        self.placement.place(&made.image, area, 0, buf);
        self.made = Some(made);
        true
    }
}

/// The cells the label's word takes: the ones the picture cut a gap for.
pub(crate) fn label_cells(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(LABEL.start),
        y: area.y,
        width: (area.width.saturating_sub(LABEL.start)).min(TITLE.len() as u16),
        height: 1.min(area.height),
    }
}

/// ` prompt ` drawn as text over the gap the stroke left for it: the span
/// `Block::title` draws, at the cells it draws it in, because the label is the
/// one thing on the border that is a word rather than a stroke.
pub(crate) fn label(ink: Style, area: Rect, buf: &mut Buffer) {
    let at = label_cells(area);
    if at.width == 0 || at.height == 0 {
        return;
    }
    Paragraph::new(Line::from(Span::styled(TITLE, ink)))
        .style(theme::surface())
        .render(at, buf);
}

/// Hands the cells of `area` back to the terminal: a picture's escape owns every
/// cell it covers, and a covered cell is never sent — so a glyph written into one
/// would be drawn into a cell the terminal is never told about.
pub(crate) fn shown(area: Rect, buf: &mut Buffer) {
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_skip(false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The composer's rect in a screen of forty columns, offset so a cell the
    /// stroke painted is never one it painted by accident.
    const AREA: Rect = Rect {
        x: 1,
        y: 2,
        width: 40,
        height: 4,
    };

    /// The stroke a made-up Kitty answer places.
    fn stroke() -> Stroke {
        Stroke::new(&Graphics::kitty()).expect("a made-up Kitty answer places")
    }

    /// The stroke drawn into a screen of its own: one frame, as a terminal is
    /// sent one.
    fn drawn(stroke: &mut Stroke, area: Rect, ink: Ink) -> Buffer {
        let mut buf = Buffer::empty(Rect::new(
            0,
            0,
            area.width + area.x + 2,
            area.height + area.y + 2,
        ));
        assert!(stroke.draw(area, ink, &mut buf), "the stroke is placed");
        buf
    }

    /// Whether a buffer carries the pixels themselves, rather than only the
    /// place the terminal keeps them in: the transmission is what costs work.
    fn transmits(buf: &Buffer) -> bool {
        buf.content.iter().any(|cell| cell.symbol().contains("a=T"))
    }

    /// Whether a buffer places what the terminal already holds.
    fn places(buf: &Buffer) -> bool {
        buf.content.iter().any(|cell| cell.symbol().contains("a=p"))
    }

    #[test]
    fn the_frame_is_made_once_and_a_ticking_frame_re_places_it() {
        let mut stroke = stroke();
        assert!(
            transmits(&drawn(&mut stroke, AREA, Ink::Prompt)),
            "the first frame transmits the picture"
        );
        let tick = drawn(&mut stroke, AREA, Ink::Prompt);
        assert!(
            !transmits(&tick),
            "a frame at the same rect and ink carries no pixels again"
        );
        assert!(
            places(&tick),
            "and still places the image the terminal holds, or it would be gone"
        );
    }

    #[test]
    fn a_resize_makes_the_picture_for_the_cells_it_fills() {
        let mut stroke = stroke();
        let _ = drawn(&mut stroke, AREA, Ink::Prompt);
        let wider = Rect {
            width: AREA.width + 6,
            ..AREA
        };
        assert!(
            transmits(&drawn(&mut stroke, wider, Ink::Prompt)),
            "a composer of another size is a new picture, sent again"
        );
    }

    #[test]
    fn a_change_of_ink_makes_the_picture_again_at_the_same_rect() {
        let mut stroke = stroke();
        let _ = drawn(&mut stroke, AREA, Ink::Prompt);
        assert!(
            transmits(&drawn(&mut stroke, AREA, Ink::Faint)),
            "the ink is part of what the picture is made for, and the rect did not \
             change to say the pixels had"
        );
    }

    #[test]
    fn the_picture_covers_the_rect_and_the_label_keeps_its_cells() {
        let mut stroke = stroke();
        let buf = drawn(&mut stroke, AREA, Ink::Prompt);
        let anchor = &buf[(AREA.x, AREA.y)];
        assert!(
            anchor.symbol().contains("\u{1b}_G"),
            "the escape rides in the composer's own top-left cell"
        );
        assert!(!anchor.skip, "and is sent, or the picture never arrives");
        for y in AREA.y..AREA.y + AREA.height {
            for x in AREA.x..AREA.x + AREA.width {
                if (x, y) == (AREA.x, AREA.y) {
                    continue;
                }
                assert!(buf[(x, y)].skip, "({x}, {y}) is covered by the picture");
            }
        }
    }
}
