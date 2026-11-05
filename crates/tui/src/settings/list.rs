//! The names the model row's fragment names: the row is a field before it is
//! ever a list, so what stands under it is only what the fragment reaches —
//! ranked by the one ranking the composer offers commands and skills with, and
//! within a tier in the order the wire named them, because that is the order the
//! row states them in. A name nothing offered is still a name the field keeps:
//! the list ranks what is offered and never narrows what is allowed.
//!
//! The same names fold open under the row when it is asked to choose: a fold is
//! every name the wire offered, with the cursor on the one the row holds, until
//! the fragment under it changes and narrows it to what the fragment reaches
//! again. Open or shut is the one thing `Enter` on that row reads, so it is held
//! here beside the cursor the fold takes a name from.

use crate::completion::ranked;

/// The offered names a fragment reaches, and the one a `Tab` would take. The
/// fragment it was ranked from is held beside them, so a keystroke that changes
/// the fragment starts the list at its nearest name again rather than where the
/// last one was left.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct List {
    /// The fragment the names were ranked from.
    typed: String,
    /// The names it reaches, nearest first.
    names: Vec<String>,
    /// The match a `Tab` would take.
    chosen: usize,
    /// Whether the names stand open under the row, as the row asked for them.
    fold: bool,
    /// The fragment the fold opened on: the fold stands on every name the wire
    /// offered for as long as the fragment is the one it opened with, and is
    /// the list the fragment reaches from the first keystroke that moves it.
    opened: String,
}

impl List {
    /// The names a fragment reaches, nearest first: the ones it starts, then the
    /// ones it sits inside, then the ones it is a subsequence of. An empty
    /// fragment reaches nothing, because a row nobody is typing in is a field
    /// and not a list of every name the wire serves.
    fn of(offered: &[String], typed: &str) -> List {
        let typed = typed.trim();
        List {
            names: match typed {
                "" => Vec::new(),
                typed => ranked(offered.iter().map(String::as_str), typed)
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
            },
            typed: typed.to_string(),
            chosen: 0,
            fold: false,
            opened: String::new(),
        }
    }

    /// The list a fragment names now, keeping the match it was on where the
    /// fragment and the names it reaches are the ones already listed: a key that
    /// moves neither leaves the reader where they were. A listing that lands
    /// while the modal is open is the same rule, so a list under a typed
    /// fragment appears without throwing the cursor to the top of it.
    pub fn refresh(&mut self, offered: &[String], typed: &str) {
        if self.fold {
            // A fold stands on every name the wire offered while the fragment is
            // the one it opened on, and is the list that fragment reaches once a
            // keystroke moves it — backspaced to where it started, the whole
            // choice is the fold's again. A listing that lands meanwhile is the
            // wire's, so a fold standing still draws the names it just named.
            let typed = typed.trim();
            let moved = typed != self.typed;
            self.names = match typed == self.opened {
                true => offered.to_vec(),
                false => List::of(offered, typed).names,
            };
            if moved {
                self.chosen = 0;
            }
            self.chosen = self.chosen.min(self.names.len().saturating_sub(1));
            self.typed = typed.to_string();
            return;
        }
        let fresh = List::of(offered, typed);
        let kept = fresh.typed == self.typed && fresh.names == self.names;
        self.chosen = match kept {
            true => self.chosen.min(fresh.names.len().saturating_sub(1)),
            false => 0,
        };
        self.typed = fresh.typed;
        self.names = fresh.names;
    }

    /// One match further from the top, or towards it, and whether there was a
    /// match to move to: a step with none left answers `false` and the arrow is
    /// the cursor's instead, so the end of a list is never a row a reader cannot
    /// leave — the row keeps its list and the keys keep their way out.
    pub fn step(&mut self, up: bool) -> bool {
        let Some(last) = self.names.len().checked_sub(1) else {
            return false;
        };
        if self.fold {
            // The fold holds the cursor: a step past either end of it stays on
            // the row, because the row's own arrows are what the fold has taken.
            self.chosen = match up {
                true => self.chosen.saturating_sub(1),
                false => (self.chosen + 1).min(last),
            };
            return true;
        }
        let at = match up {
            true => self.chosen.checked_sub(1),
            false => (self.chosen < last).then(|| self.chosen + 1),
        };
        match at {
            Some(at) => {
                self.chosen = at;
                true
            }
            None => false,
        }
    }

    /// One end of the list, as `Home` and `End` ask for them: the first name
    /// the list reaches when `first`, and the last otherwise — of the names the
    /// fragment reaches rather than of every name the wire offered, because a
    /// fragment narrows what the two ends mean. A list holding no name has no
    /// end to stand on, so the key is not the list's and the screen holds
    /// nothing for it.
    pub fn end(&mut self, first: bool) -> bool {
        let Some(last) = self.names.len().checked_sub(1) else {
            return false;
        };
        self.chosen = match first {
            true => 0,
            false => last,
        };
        true
    }

    /// The name a `Tab` takes, and the one `Enter` takes while the fold is
    /// open: the match the list is on.
    pub fn take(&self) -> Option<&str> {
        self.names.get(self.chosen).map(String::as_str)
    }

    /// The names opened under the row: every one the wire offered, in the order
    /// it named them, and the cursor on the name the row holds when the wire
    /// named it and on the first name when it did not — a name no list holds is
    /// no place to open on. The row's own text is the fragment the fold stands
    /// on, so a keystroke in the field narrows it.
    pub fn open(&mut self, offered: &[String], current: &str) {
        let current = current.trim();
        self.choice(offered, current);
        self.typed = current.to_string();
        self.opened = current.to_string();
    }

    /// The same names on a screen of their own, where there is no field to be
    /// the fragment: the screen stands on the whole list with its cursor on the
    /// name the run holds, and stays there until the reader types. Every name
    /// rather than none, because a screen is a choice before it is a filter.
    pub fn choice(&mut self, offered: &[String], current: &str) {
        let current = current.trim();
        self.fold = true;
        self.typed = String::new();
        self.opened = String::new();
        self.names = offered.to_vec();
        self.chosen = self
            .names
            .iter()
            .position(|name| name == current)
            .unwrap_or(0);
    }

    /// The fold shut again, with the draft beside it exactly as it was.
    pub fn close(&mut self) {
        self.fold = false;
    }

    /// Whether the names stand open under the row: the one fact `Enter` on that
    /// row reads before it decides what it means.
    pub fn folded(&self) -> bool {
        self.fold
    }

    /// The names the fragment reaches, in the order they are drawn.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Which of them the list is on.
    pub fn chosen(&self) -> usize {
        self.chosen
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        reason = "a panicking test is how a broken contract reports itself"
    )]

    use super::List;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn a_fragment_offers_the_names_it_starts_then_the_ones_it_sits_inside_then_the_rest() {
        let offered = names(&["g-p-t", "big-gpt", "gpt-5", "claude-sonnet-4-6"]);
        assert_eq!(
            List::of(&offered, "gpt").names(),
            names(&["gpt-5", "big-gpt", "g-p-t"]),
            "the three tiers in order, and the name the fragment never reaches left out"
        );
    }

    #[test]
    fn names_a_fragment_reaches_alike_keep_the_order_the_wire_named_them_in() {
        let offered = names(&["qwen3-32b", "qwen2-8b"]);
        assert_eq!(
            List::of(&offered, "qwen").names(),
            offered,
            "a tier is the wire's own order, because that is the order the row states"
        );
    }

    #[test]
    fn a_row_nobody_is_typing_in_is_a_field_and_not_a_list() {
        let offered = names(&["gpt-5", "claude-sonnet-4-6"]);
        assert!(
            List::of(&offered, "  ").is_empty(),
            "an empty fragment offers no list, so the arrows still move the row"
        );
    }

    #[test]
    fn a_fragment_that_changes_starts_the_list_at_its_nearest_name_again() {
        let offered = names(&["gpt-5", "gpt-4"]);
        let mut list = List::of(&offered, "gpt");
        assert!(list.step(false), "there are two matches to step through");
        assert_eq!(list.take(), Some("gpt-4"));
        assert!(
            !list.step(false),
            "a step past the last match is the cursor's, so the row is never a trap"
        );

        list.refresh(&offered, "gpt-4");
        assert_eq!(
            list.take(),
            Some("gpt-4"),
            "the same fragment naming the same names keeps its place"
        );
        list.refresh(&offered, "gpt");
        assert_eq!(
            list.take(),
            Some("gpt-5"),
            "a fragment that changed starts again at the nearest name"
        );
    }

    #[test]
    fn a_fold_stands_on_every_name_and_opens_on_the_one_the_row_holds() {
        let offered = names(&["qwen3-32b", "llama3-8b", "gpt-5"]);
        let mut list = List::default();
        list.open(&offered, "llama3-8b");
        assert!(list.folded(), "the fold is open");
        assert_eq!(
            list.names(),
            offered,
            "a fold is every name the wire offered, not the ones a fragment reaches"
        );
        assert_eq!(
            list.take(),
            Some("llama3-8b"),
            "the cursor opens on the row's own name"
        );

        let mut elsewhere = List::default();
        elsewhere.open(&offered, "hand-typed");
        assert_eq!(
            elsewhere.take(),
            Some("qwen3-32b"),
            "a name no list holds is no place to open on, so the fold opens on the first"
        );
    }

    #[test]
    fn a_fold_typed_into_narrows_to_the_names_the_fragment_reaches() {
        let offered = names(&["qwen3-32b", "qwen2-8b", "gpt-5"]);
        let mut list = List::default();
        list.open(&offered, "");
        assert_eq!(list.len(), 3, "the fold stands on every name");

        list.refresh(&offered, "qwen");
        assert_eq!(
            list.names(),
            names(&["qwen3-32b", "qwen2-8b"]),
            "a fragment typed under the fold narrows it as it always did"
        );
        assert!(list.folded(), "and the fold stays open to take one");

        list.refresh(&offered, "");
        assert_eq!(
            list.len(),
            3,
            "a fragment emptied is the whole choice again"
        );
    }

    #[test]
    fn the_fold_holds_its_cursor_at_either_end_of_itself() {
        let mut list = List::default();
        list.open(&names(&["one", "two"]), "");
        assert!(
            list.step(true),
            "the fold takes the arrow, so the row keeps its cursor"
        );
        assert_eq!(
            list.take(),
            Some("one"),
            "and the first name is as far as it goes"
        );
        assert!(list.step(false));
        assert_eq!(list.take(), Some("two"));
        assert!(list.step(false), "the last name holds it too");
        assert_eq!(list.take(), Some("two"), "so the cursor stays on it");
    }

    #[test]
    fn an_arrow_with_no_match_to_move_to_leaves_the_row_to_take_the_key() {
        let mut list = List::of(&names(&["gpt-5"]), "zzz");
        assert!(
            !list.step(false),
            "a fragment nothing reaches steps nothing"
        );
        assert_eq!(list.take(), None, "and a Tab has nothing to take");
    }
}
