use super::*;
use crate::palette::Band;

const PALETTES: [Palette; 3] = [Palette::SLOTS, Palette::ANSI, Palette::TRUECOLOR];

#[test]
fn every_ink_reads_back_as_the_name_it_is_drawn_in() {
    for palette in PALETTES {
        for ink in Ink::ALL {
            let mut cell = Cell::default();
            cell.set_style(ink.style(&palette));

            assert_eq!(
                Ink::of(&cell, &palette),
                Some(ink),
                "a cell drawn in {} is that ink, and a fixture has to be able to say so",
                ink.name()
            );
            assert_eq!(Ink::from_name(ink.name()), Some(ink));
        }
    }
}

#[test]
fn a_cell_a_mark_covers_reads_back_as_the_mark_and_no_other_ink() {
    for palette in PALETTES {
        let mut cell = Cell::default();
        cell.set_style(Ink::Mark.style(&palette));

        assert_eq!(
            Ink::of(&cell, &palette),
            Some(Ink::Mark),
            "the marked cell is in `mark`, which is a name a reader can look up",
        );
    }
}

#[test]
fn the_accent_drawn_faint_reads_back_as_the_accent_drawn_faint() {
    let faint = Look::new(Ink::Accent, Face::Faint);

    assert_eq!(
        faint.name(),
        "accent+faint",
        "the face a look is drawn in is its name"
    );
    for palette in PALETTES {
        for band in [None, Some(Band::Rail), Some(Band::Prompt), Some(Band::Well)] {
            let mut cell = Cell::default();
            cell.set_style(faint.style(&palette));
            if let Some(band) = band {
                cell.set_style(Style::default().bg(palette.band(band)));
            }

            assert_eq!(
                Look::of(&cell, &palette),
                Some(faint),
                "a faint accent written on {band:?} is a look a fixture can name, so it reads \
                     back as itself"
            );
        }
    }
    assert_eq!(Look::from_name("accent+faint"), Some(faint));
}

#[test]
fn a_cell_wearing_two_faces_reads_back_as_both_of_them_in_one_order() {
    let emphasis = Look::new(Ink::Plain, Faces::BOLD.and(Faces::ITALIC));

    assert_eq!(
        emphasis.name(),
        "plain+bold+italic",
        "a look wears a set of faces, and the fixture writes them in one order"
    );
    assert_eq!(
        Look::from_name("plain+italic+bold"),
        Some(emphasis),
        "the faces are a set, so the order they are named in is not the look"
    );
    for palette in PALETTES {
        let mut cell = Cell::default();
        cell.set_style(emphasis.style(&palette));

        assert_eq!(
            Look::of(&cell, &palette),
            Some(emphasis),
            "a cell wearing bold and italic at once reads back as neither one of them alone"
        );
    }
}

#[test]
fn the_italic_face_is_one_an_ink_may_wear_and_every_palette_draws_the_design_in_it() {
    for ink in Ink::ALL {
        let italic = Look::new(ink, Face::Italic);
        for palette in PALETTES {
            let mut cell = Cell::default();
            cell.set_style(italic.style(&palette));

            assert_eq!(
                Look::of(&cell, &palette),
                Some(italic),
                "the {} ink drawn italic reads back as some other look, and every palette is \
                     asked because one of them draws the ink in the terminal's own colour",
                ink.name()
            );
        }
    }

    let slotted = Look::new(Ink::Accent, Face::Italic).style(&Palette::SLOTS);
    assert!(
        slotted.add_modifier.contains(Modifier::ITALIC),
        "the palette a terminal that draws no colour of its own is handed still draws the \
             face the design asks for"
    );
}

#[test]
fn a_look_may_carry_a_line_under_it_and_reads_back_as_its_ink_its_faces_and_that_line() {
    let linked = Look::new(Ink::Accent, Faces::ITALIC).underlined(Ink::Dim);

    assert_eq!(linked.ink(), Ink::Accent);
    assert_eq!(linked.faces(), Faces::ITALIC);
    assert_eq!(linked.underline(), Some(Ink::Dim));
    assert_eq!(
        linked.name(),
        "accent+italic~dim",
        "an underlined look is named by its ink, its faces and the ink under it"
    );
    assert_eq!(Look::from_name("accent+italic~dim"), Some(linked));

    for palette in PALETTES {
        for band in [None, Some(Band::Rail), Some(Band::Prompt), Some(Band::Well)] {
            let mut cell = Cell::default();
            cell.set_style(linked.style(&palette));
            if let Some(band) = band {
                cell.set_style(Style::default().bg(palette.band(band)));
            }

            assert_eq!(
                Look::of(&cell, &palette),
                Some(linked),
                "an underlined look written on {band:?} reads back as some other look, so a \
                     fixture cannot write down a link"
            );
        }
    }
}

#[test]
fn a_struck_run_is_a_face_the_look_wears_and_no_ink_hands_it_out() {
    let struck = Look::new(Ink::Accent, Faces::CROSSED);

    assert_eq!(
        Face::Crossed.name(),
        "crossed",
        "the face a struck run wears is named for the line through it"
    );
    assert_eq!(
        struck.name(),
        "accent+crossed",
        "a struck run is its block's ink wearing the face, and the fixture writes it as one"
    );
    assert_eq!(Look::from_name("accent+crossed"), Some(struck));
    assert_eq!(
        Faces::of_modifier(Modifier::CROSSED_OUT),
        Some(Faces::CROSSED),
        "a cell the terminal draws crossed reads back as the face, which is what a fixture \
         holding a struck run has to say"
    );
    for palette in PALETTES {
        for ink in Ink::ALL {
            let mut cell = Cell::default();
            cell.set_style(Look::plain(ink).style(&palette));

            assert!(
                !Look::of(&cell, &palette)
                    .expect("every cell painted is a look")
                    .faces()
                    .has(Face::Crossed),
                "the {} ink hands out the crossing, so a struck run would be an ink of its own: \
                 the design gives the crossing no colour",
                ink.name()
            );
        }
    }
}

/// Every set the faces compose, from the one a look wearing none has up to all of them: `Face::ALL`
/// names them once, so a face added there is crossed with the others without a second list.
fn sets_of_faces() -> impl Iterator<Item = Faces> {
    (0u8..(1 << (Face::ALL.len() - 1))).map(|worn| {
        Face::ALL
            .into_iter()
            .filter(|face| *face != Face::Plain)
            .enumerate()
            .filter(|(bit, _)| worn & (1 << bit) != 0)
            .fold(Faces::PLAIN, |set, (_, face)| set.and(Faces::wearing(face)))
    })
}

#[test]
fn every_set_of_faces_reads_back_as_the_look_it_is_drawn_in() {
    for palette in PALETTES {
        for ink in Ink::ALL {
            for faces in sets_of_faces() {
                let look = Look::new(ink, faces);
                let mut cell = Cell::default();
                cell.set_style(look.style(&palette));
                let read = Look::of(&cell, &palette).expect("every cell painted is a look");

                assert_eq!(
                    read.style(&palette),
                    look.style(&palette),
                    "a cell painted {:?} reads back as a look drawn another way, so the name a \
                         fixture writes is not the picture it holds",
                    look.name()
                );
                // Reverse video is the mark ink's, so an ink the palette draws in the mark's
                // colour wears it as the mark: one picture, and so one name for it.
                if palette.colour(ink) != palette.colour(Ink::Mark) || !faces.has(Face::Reversed) {
                    assert_eq!(
                        read,
                        look,
                        "a cell painted {:?} is that look, and a look nothing turns round is \
                             not the mark",
                        look.name()
                    );
                }
                assert_eq!(Look::from_name(&read.name()), Some(read));
            }
        }
    }
}
