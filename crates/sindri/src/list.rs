//! The list a card draws: the query it answers at its head, the one line it says in place of its
//! rows, and one row per item with the row a reader is on marked — each filled down the band a card
//! hands it and cut at the frame's edge.

use crate::ink::{Ink, Look};
use crate::pen::{self, Clip};
use crate::rows::wrap;

/// The columns a row keeps beside a name, before the words that stand beside it.
const GAP: u16 = 2;

/// The words a list's query is typed after.
const QUERY: &str = "query: ";

/// What a row of a card chooses when a press lands on it: a file the picker offers, a name the wire
/// served, or a command the draft's word leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    File(usize),
    Model(usize),
    Command(usize),
}

/// One row of a card as data: the spans it draws, each in its own look, and the rule that decides
/// what happens to a row the card has no room for.
pub struct ScreenRow {
    pub spans: Vec<(String, Look)>,
    pub clip: Clip,
    /// What a press on this row chooses, or nothing where no key hangs from it.
    pub choice: Option<Choice>,
}

impl ScreenRow {
    /// A row whole or not there at all.
    pub fn whole(spans: Vec<(String, Look)>) -> ScreenRow {
        ScreenRow {
            spans,
            clip: Clip::Whole,
            choice: None,
        }
    }

    /// A row cut at its right edge: a value that runs on is cut rather than dropped.
    pub fn cut(spans: Vec<(String, Look)>) -> ScreenRow {
        ScreenRow {
            spans,
            clip: Clip::Edge,
            choice: None,
        }
    }

    /// The same row, marked as one a press chooses: a card's rows are the ones a click has an act
    /// for.
    pub fn choosing(self, choice: Choice) -> ScreenRow {
        ScreenRow {
            choice: Some(choice),
            ..self
        }
    }
}

/// One item of a list: the words a reader is looking for, and the words that stand beside them,
/// each in its own ink.
pub struct Item {
    pub name: String,
    pub beside: Vec<(String, Ink)>,
    /// The ink the name and the marker's blank columns stand in when the reader is not on the row.
    pub ink: Ink,
}

impl Item {
    /// An item that is only its name: the words that stand beside it are the screen's to add.
    pub fn new(name: impl Into<String>) -> Item {
        Item {
            name: name.into(),
            beside: Vec::new(),
            ink: Ink::Plain,
        }
    }

    /// An item whose name says what it is and whose words beside it say the rest: the gap between
    /// them, and those words set in the quieter ink.
    pub fn beside(name: impl Into<String>, said: impl Into<String>) -> Item {
        Item {
            name: name.into(),
            beside: vec![(format!("{}{}", pen::blank(GAP), said.into()), Ink::Dim)],
            ink: Ink::Plain,
        }
    }
}

/// The items a column of names makes: each name padded to the widest of them and the gap beside it,
/// so the words beside them begin in one column, the names in the ink the column is given and the
/// words of each row in the ink it carries.
pub fn columned<'a>(rows: impl Iterator<Item = (&'a str, (&'a str, Ink))>, ink: Ink) -> Vec<Item> {
    let rows: Vec<(&str, (&str, Ink))> = rows.collect();
    let widest = rows
        .iter()
        .map(|(name, _)| pen::cells(name) + GAP)
        .max()
        .unwrap_or(0);
    rows.into_iter()
        .map(|(name, (said, beside))| Item {
            name: pen::pad(name, widest),
            beside: vec![(said.to_owned(), beside)],
            ink,
        })
        .collect()
}

/// What stands under a list the visit refused: a blank row, then the reason wrapped to the band and
/// each line drawn in the ink an error is read in.
pub fn verdict(words: &str, width: u16) -> Vec<ScreenRow> {
    let mut rows = vec![ScreenRow::whole(Vec::new())];
    for line in wrap(words, width) {
        rows.push(ScreenRow::cut(vec![(line, Look::plain(Ink::Err))]));
    }
    rows
}

/// What a list shows: the query it answers, the one line it says in place of its rows, one row per
/// item, and the row the reader is on.
pub struct List<'a> {
    pub query: Option<&'a str>,
    pub said: Option<&'a str>,
    pub items: Vec<Item>,
    pub selected: usize,
    pub marker: &'a str,
    pub choice: Option<fn(usize) -> Choice>,
}

impl List<'_> {
    /// The rows this list fills its band with: the query at the head, the one line it says in place
    /// of its rows, or one row per item with the selected one marked — each cut at the frame's edge.
    pub fn rows(&self) -> Vec<ScreenRow> {
        let mut rows = Vec::new();
        if let Some(typed) = self.query {
            rows.push(ScreenRow::cut(vec![
                (QUERY.to_owned(), Look::plain(Ink::Dim)),
                (typed.to_owned(), Look::plain(Ink::Muted)),
            ]));
        }
        if let Some(words) = self.said {
            rows.push(ScreenRow::whole(vec![(
                words.to_owned(),
                Look::plain(Ink::Dim),
            )]));
            return rows;
        }
        for (at, item) in self.items.iter().enumerate() {
            let (marker, look) = match at == self.selected {
                true => (self.marker.to_owned(), Look::plain(Ink::Accent)),
                false => (pen::blank(pen::cells(self.marker)), Look::plain(item.ink)),
            };
            let mut spans = vec![(marker, look), (item.name.clone(), look)];
            spans.extend(
                item.beside
                    .iter()
                    .map(|(said, ink)| (said.clone(), Look::plain(*ink))),
            );
            let mut row = ScreenRow::cut(spans);
            if let Some(choice) = self.choice {
                row = row.choosing(choice(at));
            }
            rows.push(row);
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKER: &str = "\u{203a} ";

    /// The words of a row, each with the ink it is drawn in.
    fn spans(row: &ScreenRow) -> Vec<(&str, Ink)> {
        row.spans
            .iter()
            .map(|(said, look)| (said.as_str(), look.ink()))
            .collect()
    }

    fn list<'a>(query: Option<&'a str>, items: Vec<Item>, marker: &'a str) -> List<'a> {
        List {
            query,
            said: None,
            items,
            selected: 0,
            marker,
            choice: None,
        }
    }

    #[test]
    fn the_query_line_is_one_row_cut_and_its_two_inks_say_which_half_the_reader_typed() {
        let rows = list(Some("rows"), Vec::new(), MARKER).rows();

        assert_eq!(
            rows.len(),
            1,
            "a list whose query is `rows` and holds no item draws the query line and nothing under \
             it"
        );
        assert_eq!(
            spans(&rows[0]),
            vec![(QUERY, Ink::Dim), ("rows", Ink::Muted)],
            "the words the query is typed after stand quieter than what the reader typed, so the \
             two halves of the line are read as different things"
        );
        assert_eq!(
            rows[0].clip,
            Clip::Edge,
            "a query wider than the frame is cut at its right edge rather than dropped: it is what \
             the reader is pointing at"
        );
    }

    #[test]
    fn a_list_that_says_a_line_in_place_of_its_rows_draws_no_row_of_an_item() {
        let mut list = list(Some("gpt"), Vec::new(), MARKER);
        list.items.push(Item::new("gpt-4o"));
        list.said = Some("asking the wire what it serves");

        let rows = list.rows();

        assert_eq!(
            rows.len(),
            2,
            "a list given a line to say in place of its rows draws the query and that line, and \
             no row of an item it holds: {} rows, {:?}",
            rows.len(),
            rows.iter().map(spans).collect::<Vec<_>>()
        );
        assert_eq!(
            spans(&rows[1]),
            vec![("asking the wire what it serves", Ink::Dim)],
            "the line is the list's own word about itself, so it stands dim where the names it \
             stands in for would be"
        );
        assert_eq!(
            rows[1].clip,
            Clip::Whole,
            "and that line is never read in part: half a sentence about why the list is empty \
             says nothing a reader can act on"
        );
    }

    #[test]
    fn the_row_a_reader_is_on_wears_the_marker_and_the_accent_and_every_other_row_a_blank_of_it() {
        let items = ["rail.rs", "rows.rs", "view.rs"]
            .into_iter()
            .map(Item::new)
            .collect();
        let mut list = list(None, items, MARKER);
        list.selected = 1;

        let rows = list.rows();

        assert_eq!(
            rows.len(),
            3,
            "a list of three items draws three rows, so a reader can count what the query left"
        );
        assert_eq!(
            spans(&rows[1]),
            vec![(MARKER, Ink::Accent), ("rows.rs", Ink::Accent)],
            "the row the reader is on opens with the marker and is drawn in the accent, so the \
             reader's eye lands on it first"
        );
        assert_eq!(
            spans(&rows[0]),
            vec![("  ", Ink::Plain), ("rail.rs", Ink::Plain)],
            "a row the reader is not on keeps the marker's own columns blank, so every name begins \
             in one column and the marker is what says which row is the reader's"
        );
        assert_eq!(
            spans(&rows[2]),
            vec![("  ", Ink::Plain), ("view.rs", Ink::Plain)],
            "and the row under the reader's is drawn as plain as the one over it: the list has one \
             marked row, not a row that is more or less than the others"
        );
    }

    #[test]
    fn a_row_the_reader_is_not_on_stands_in_the_ink_its_item_names_and_the_readers_row_in_the_accent()
     {
        let items = ["provider", "thinking"]
            .into_iter()
            .map(|name| Item {
                ink: Ink::Muted,
                ..Item::new(name)
            })
            .collect();
        let mut list = list(None, items, MARKER);
        list.selected = 1;

        let rows = list.rows();

        assert_eq!(
            spans(&rows[0]),
            vec![("  ", Ink::Muted), ("provider", Ink::Muted)],
            "a row the reader is not on stands in the ink its item names, the marker's own \
             columns with it: a screen that lists quieter keys than it lists values has to be \
             able to say so"
        );
        assert_eq!(
            spans(&rows[1]),
            vec![(MARKER, Ink::Accent), ("thinking", Ink::Accent)],
            "and the row the reader is on is the accent whatever its item names, because the \
             reader's eye is what the accent is for"
        );
    }

    #[test]
    fn the_words_beside_a_name_begin_the_gap_after_it_in_the_quieter_ink() {
        let item = Item::beside("rows.rs", "src");

        assert_eq!(
            item.beside,
            vec![(String::from("  src"), Ink::Dim)],
            "the words beside a name are set off from it by the gap, in the ink that stands them \
             behind the name a reader is looking for"
        );

        let rows = list(None, vec![item], MARKER).rows();
        assert_eq!(
            spans(&rows[0]),
            vec![
                (MARKER, Ink::Accent),
                ("rows.rs", Ink::Accent),
                ("  src", Ink::Dim)
            ],
            "and a row of the list draws them after the name they belong to, the whole of the row \
             in the one line a card gives it"
        );
    }

    #[test]
    fn a_column_of_names_pads_each_one_so_the_words_beside_begin_in_one_column() {
        let items = columned(
            [
                ("gpt-4o", ("the larger one", Ink::Dim)),
                ("o3", ("the newer one", Ink::Dim)),
            ]
            .into_iter(),
            Ink::Plain,
        );

        assert_eq!(
            items
                .iter()
                .map(|item| pen::cells(&item.name))
                .collect::<Vec<_>>(),
            vec![pen::cells("gpt-4o") + GAP, pen::cells("gpt-4o") + GAP],
            "the words beside a column of names begin in one column, which is the widest name \
             and the gap after it: {:?}",
            items
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            items[1].beside,
            vec![(String::from("the newer one"), Ink::Dim)],
            "the words beside each name are its own, and they carry no gap of their own: the \
             column the name was padded to is the gap"
        );
        assert!(
            columned(std::iter::empty(), Ink::Plain).is_empty(),
            "a list of no rows pads no name, and a column of nothing is the empty column"
        );
    }

    #[test]
    fn a_verdict_stands_a_blank_row_under_the_list_and_its_words_wrapped_below_in_the_error_ink() {
        let words = "the wire refused the api key because it will not parse as a number";
        let rows = verdict(words, 20);

        assert_eq!(
            spans(&rows[0]),
            Vec::new(),
            "the refusal stands a blank row under the list, so it reads as a verdict on the visit \
             and not as the last key's value"
        );
        assert_eq!(
            rows[0].clip,
            Clip::Whole,
            "and that row is whole: it is a held breath, not a line of words to cut"
        );
        assert_eq!(
            rows[1..]
                .iter()
                .map(|row| pen::cells(&row.spans[0].0))
                .collect::<Vec<_>>(),
            vec![20, 18, 19, 6],
            "the reason wraps to the width the band gave it, so a card narrow as this one still \
             reads the whole of it: {:?}",
            rows.iter().map(spans).collect::<Vec<_>>()
        );
        let said: Vec<&str> = rows[1..]
            .iter()
            .flat_map(|row| row.spans[0].0.split_whitespace())
            .collect();
        assert_eq!(
            said.join(" "),
            words,
            "and no word of the reason is lost or said twice in the wrapping, because a reader \
             who cannot read why a value was refused cannot change it"
        );
        for row in &rows[1..] {
            assert_eq!(
                spans(row),
                vec![(row.spans[0].0.as_str(), Ink::Err)],
                "every line of the verdict is drawn in the ink an error is read in, so it is not \
                 mistaken for the list's own words"
            );
            assert_eq!(
                row.clip,
                Clip::Edge,
                "a line wider than the frame is cut at its right edge rather than dropped"
            );
        }
    }

    #[test]
    fn a_column_of_names_stands_in_the_ink_it_was_given_and_each_beside_word_in_its_own() {
        let items = columned(
            [
                ("provider", ("anthropic", Ink::Plain)),
                ("model", ("claude-sonnet", Ink::Accent)),
            ]
            .into_iter(),
            Ink::Muted,
        );

        assert_eq!(
            items.iter().map(|item| item.ink).collect::<Vec<_>>(),
            vec![Ink::Muted, Ink::Muted],
            "every name of the column stands in the ink the column was given, so one ink says \
             what the whole column of names is"
        );
        assert_eq!(
            items
                .iter()
                .map(|item| item.beside.clone())
                .collect::<Vec<_>>(),
            vec![
                vec![(String::from("anthropic"), Ink::Plain)],
                vec![(String::from("claude-sonnet"), Ink::Accent)],
            ],
            "and the words beside each name carry the ink their row was handed, so one row of the \
             column can be read in the accent while the rest are plain"
        );
    }

    #[test]
    fn a_list_with_no_query_line_draws_its_item_rows_first() {
        let items = ["commands", "keys"].into_iter().map(Item::new).collect();
        let rows = list(None, items, MARKER).rows();

        assert_eq!(
            spans(&rows[0]),
            vec![(MARKER, Ink::Accent), ("commands", Ink::Accent)],
            "a screen whose query is the draft above it draws no query line of its own, so the \
             row the reader is on is the list's first row"
        );
        assert_eq!(
            rows.len(),
            2,
            "and nothing stands at the head of the list in place of a query it does not have"
        );
    }

    #[test]
    fn the_row_a_press_chooses_hangs_from_the_row_of_the_list_it_was_drawn_in() {
        let items = ["a", "b", "c"].into_iter().map(Item::new).collect();
        let mut names = list(Some(""), items, MARKER);
        names.choice = Some(Choice::Model);

        let rows = names.rows();

        assert_eq!(
            rows.iter().map(|row| row.choice).collect::<Vec<_>>(),
            vec![
                None,
                Some(Choice::Model(0)),
                Some(Choice::Model(1)),
                Some(Choice::Model(2))
            ],
            "a press on a row chooses the item that row was drawn for, and the query line above \
             them chooses nothing: a click there is a click on no name"
        );

        let mut said = list(None, Vec::new(), MARKER);
        said.said = Some("no served name answers");
        assert_eq!(
            said.rows()[0].choice,
            None,
            "and a list's one line about itself is no row to choose either"
        );
    }
}
