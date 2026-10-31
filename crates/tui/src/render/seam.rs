//! The glow in the margin: the seam's light strip, `seam`, drawn in the canvas
//! between the words and the rail where the terminal speaks Kitty graphics. It
//! runs from the top of the body down to the composer's last row, because the
//! margin beside the panel is canvas for the panel's whole height, and it
//! dissolves there at the footer rather than on the top rule that used to end
//! the body. It is generated once per strip height and transmitted once, its
//! protocol held across frames, so a ticking frame carries placeholders and a
//! resize makes a new strip. It fills a gap two cells wide, because
//! `Buffer::diff` skips the cell after a picture's escape — so the cell the
//! diff drops is the strip's own second, and never the rail's first column or
//! the face drawn above it (`render::GAP_COLUMNS`).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::StatefulWidget;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage};

use crate::app::App;
use crate::graphics::Graphics;
use crate::seam;

/// The strip fills the rect it is given rather than being fitted into it: an
/// 8×256 strip has a natural height of a dozen rows, and `Fit` would letterbox
/// it, so the glow would stop short of the composer's last row. `Scale` needs no
/// filter: the strip is drawn at the rect's own pixels.
const RESIZE: Resize = Resize::Scale(None);

/// The seam as a frame draws it: the terminal's one answer, and the strip that
/// answer has been given.
pub(crate) struct Seam {
    picker: Picker,
    /// The strip as it was last made: nothing until the frame has given it a
    /// rect.
    drawn: Option<Strip>,
}

/// One made strip: the cells it was made for, and the protocol that carries it.
/// A resize makes another; a frame at the same size reuses this one, which is
/// what keeps a ticking frame from transmitting the picture again.
struct Strip {
    cells: Rect,
    protocol: StatefulProtocol,
}

impl Seam {
    /// The seam the terminal's answer allows, or `None` where that answer is not
    /// Kitty graphics: the question was asked once, by [`Graphics`], and this
    /// only reads it.
    pub fn new(graphics: &Graphics) -> Option<Seam> {
        let picker = graphics.picker();
        if picker.protocol_type() != ProtocolType::Kitty {
            return None;
        }
        Some(Seam {
            picker: picker.clone(),
            drawn: None,
        })
    }

    /// The seam a test draws, built from the made-up answer a test can afford:
    /// nothing is asked of a terminal. A frame's own tests get theirs from
    /// `App::with_graphics(Some(Graphics::kitty()))`, which is the one answer
    /// the face and the pictures read too.
    #[cfg(test)]
    pub fn kitty() -> Option<Seam> {
        Seam::new(&Graphics::kitty())
    }

    /// Draws the strip into `area`, making it if that rect is a size it has not
    /// been made for yet.
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        if self.drawn.as_ref().is_none_or(|strip| strip.cells != area) {
            let (wide, tall) = self.picker.font_size();
            let strip = seam::strip(
                u32::from(area.width) * u32::from(wide),
                u32::from(area.height) * u32::from(tall),
            );
            self.drawn = Some(Strip {
                cells: area,
                protocol: self.picker.new_resize_protocol(strip.into()),
            });
        }
        if let Some(strip) = self.drawn.as_mut() {
            StatefulImage::<StatefulProtocol>::default()
                .resize(RESIZE)
                .render(area, buf, &mut strip.protocol);
        }
    }
}

/// The strip's rect: the margin's own columns, carried from the top of the body
/// down to the composer's last row. Now that the panel ends at the seam the
/// margin beside it is canvas for its whole height, so the light goes on beside
/// the composer instead of dying on the row above its top rule; and because the
/// panel's columns end where the margin's begin, the two rects are beside each
/// other and never over. `seam::strip` fades across the pixels it is given, so
/// the longer reach is a longer dissolve, arriving at nothing on the composer's
/// last row — the row above the footer, which the strip does not reach.
pub(crate) fn reach(margin: Rect, panel: Rect) -> Rect {
    Rect {
        height: panel.bottom().saturating_sub(margin.y),
        ..margin
    }
}

/// Draws the glow beside the rail. A frame with no seam to draw leaves the
/// margin as the canvas it always was.
pub(crate) fn draw(app: &mut App, area: Rect, buf: &mut Buffer) {
    if let Some(seam) = app.seam.as_mut() {
        seam.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A margin one cell wide and twelve rows tall, at the made-up cell size.
    fn margin() -> Rect {
        Rect::new(67, 0, 1, 12)
    }

    /// What the first cell of each of the rect's rows holds.
    fn column(area: Rect, buf: &Buffer) -> Vec<String> {
        (0..area.height)
            .map(|y| buf[(area.x, area.y + y)].symbol().to_string())
            .collect()
    }

    /// Whether a row's sequence carries the picture itself rather than only its
    /// placeholders: the transmission is the part that costs a terminal work.
    fn transmits(symbol: &str) -> bool {
        symbol.contains("\u{1b}_G")
    }

    #[test]
    fn the_strip_is_transmitted_once_and_a_ticking_frame_re_transmits_nothing() {
        let area = margin();
        let mut seam = Seam::kitty().expect("a made-up Kitty answer draws the seam");
        let mut first = Buffer::empty(area);
        seam.render(area, &mut first);
        assert!(
            column(area, &first)
                .iter()
                .filter(|row| transmits(row))
                .count()
                == 1,
            "the picture goes out once, with the frame that first carries it"
        );

        let mut tick = Buffer::empty(area);
        seam.render(area, &mut tick);
        assert!(
            column(area, &tick).iter().all(|row| !transmits(row)),
            "a frame at the same size carries placeholders and transmits nothing: {:?}",
            column(area, &tick)
        );
        assert!(
            column(area, &tick)
                .iter()
                .all(|row| row.contains('\u{10EEEE}')),
            "and every row of the strip still carries its placeholder"
        );
    }

    /// The margin and the panel at the size the issue asks for: a body of twenty
    /// rows, a composer of four beneath it, and two cells of canvas beside both.
    fn beside_the_composer() -> (Rect, Rect) {
        (Rect::new(66, 0, 2, 20), Rect::new(0, 20, 66, 4))
    }

    #[test]
    fn the_strips_reach_is_the_body_and_the_composer_to_its_last_row() {
        let (margin, panel) = beside_the_composer();
        let strip = reach(margin, panel);
        assert_eq!(
            strip,
            Rect::new(66, 0, 2, 24),
            "twenty rows of body and four of composer, in the margin's columns"
        );
        assert_eq!(
            strip.bottom(),
            panel.bottom(),
            "and its last row is the composer's last row"
        );
        assert!(
            !strip.intersects(panel),
            "beside the panel, never inside it: {strip:?} and {panel:?}"
        );
        assert_eq!(
            strip.x,
            panel.right(),
            "one cell off the panel's right border, as the margin always was"
        );
    }

    #[test]
    fn a_strip_that_reaches_the_composer_is_transmitted_once_and_holds_after() {
        let (margin, panel) = beside_the_composer();
        let area = reach(margin, panel);
        let mut seam = Seam::kitty().expect("a made-up Kitty answer draws the seam");
        let mut first = Buffer::empty(area);
        seam.render(area, &mut first);
        assert_eq!(
            column(area, &first)
                .iter()
                .filter(|row| transmits(row))
                .count(),
            1,
            "a taller strip is a new picture, and it goes out once"
        );
        assert!(
            column(area, &first)
                .iter()
                .all(|row| row.contains('\u{10EEEE}')),
            "and every one of its twenty-four rows carries it"
        );

        let mut tick = Buffer::empty(area);
        seam.render(area, &mut tick);
        assert!(
            column(area, &tick).iter().all(|row| !transmits(row)),
            "the cache is keyed by the rect, so a frame at the same reach \
             re-transmits nothing"
        );
    }

    #[test]
    fn a_resize_re_fits_the_strip_and_transmits_it_again() {
        let mut seam = Seam::kitty().expect("a made-up Kitty answer draws the seam");
        let mut buf = Buffer::empty(margin());
        seam.render(margin(), &mut buf);

        let taller = Rect {
            height: 20,
            ..margin()
        };
        let mut resized = Buffer::empty(taller);
        seam.render(taller, &mut resized);
        assert!(
            column(taller, &resized)
                .iter()
                .filter(|row| transmits(row))
                .count()
                == 1,
            "a body of a different height is a new strip, sent again: {:?}",
            column(taller, &resized)
        );
        assert!(
            column(taller, &resized)
                .iter()
                .all(|row| row.contains('\u{10EEEE}')),
            "and it fills the rect it was given, to the composer: {:?}",
            column(taller, &resized)
        );
    }

    #[test]
    fn the_strips_own_cell_is_the_only_cell_it_paints() {
        let area = Rect {
            width: 1,
            ..margin()
        };
        let wide = Buffer::empty(Rect { width: 4, ..area });
        let mut seam = Seam::kitty().expect("a made-up Kitty answer draws the seam");
        let mut buf = wide;
        seam.render(area, &mut buf);
        for y in 0..area.height {
            assert!(
                !buf[(area.x, area.y + y)].skip,
                "one cell wide leaves nothing to skip on row {y}"
            );
            for x in 1..area.width {
                assert_eq!(
                    buf[(area.x + x, area.y + y)].symbol(),
                    " ",
                    "the cell beside the strip is not the strip's"
                );
            }
        }
    }

    #[test]
    fn a_terminal_that_does_not_speak_kitty_gets_no_glow() {
        let graphics = crate::graphics::Graphics::from_picker(Picker::from_fontsize((8, 18)));
        assert!(
            Seam::new(&graphics).is_none(),
            "the glow is drawn where the terminal speaks Kitty, and nowhere else"
        );
    }
}
