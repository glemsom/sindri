//! The names the model row's fragment names: the row is a field before it is
//! ever a list, so what stands under it is only what the fragment reaches —
//! ranked by the one ranking the composer offers commands and skills with, and
//! within a tier in the order the wire named them, because that is the order the
//! row states them in. A name nothing offered is still a name the field keeps:
//! the list ranks what is offered and never narrows what is allowed.

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
        }
    }

    /// The list a fragment names now, keeping the match it was on where the
    /// fragment and the names it reaches are the ones already listed: a key that
    /// moves neither leaves the reader where they were. A listing that lands
    /// while the modal is open is the same rule, so a list under a typed
    /// fragment appears without throwing the cursor to the top of it.
    pub fn refresh(&mut self, offered: &[String], typed: &str) {
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

    /// The name a `Tab` takes: the match the list is on.
    pub fn take(&self) -> Option<&str> {
        self.names.get(self.chosen).map(String::as_str)
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
    fn an_arrow_with_no_match_to_move_to_leaves_the_row_to_take_the_key() {
        let mut list = List::of(&names(&["gpt-5"]), "zzz");
        assert!(
            !list.step(false),
            "a fragment nothing reaches steps nothing"
        );
        assert_eq!(list.take(), None, "and a Tab has nothing to take");
    }
}
