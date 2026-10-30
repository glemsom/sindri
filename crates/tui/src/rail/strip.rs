//! The rail's one priority order, and the one-line strip that reads it: the
//! facts a short terminal gives up first, and the same facts in the same order
//! in as much of them as a line can hold, with never a fact cut in half
//! (ADR-0011 §22).

use super::{Block, Fact, SEPARATOR};

/// The separator's width, counted in cells once so every room is the same
/// room.
const GAP: usize = 3;

/// The rail as one line, for a terminal too narrow to give it a column: the
/// facts the line may never lose are given their room first, and what is left
/// goes to the rest, each fact in the order the column reads it.
pub(super) fn strip(facts: &[Fact], width: usize) -> String {
    // What the facts the line may never lose need, at their briefest.
    let never_lost: usize = facts
        .iter()
        .filter(|fact| fact.keeps())
        .map(|fact| fact.brief().chars().count() + GAP)
        .sum();
    let mut shown = if never_lost > width {
        narrow(facts, width)
    } else {
        read(facts, width)
    };
    // Drawn in the order the column reads them, whichever order they were
    // given their room in.
    shown.sort_by_key(|(at, _)| *at);
    shown
        .into_iter()
        .map(|(_, entry)| entry)
        .collect::<Vec<String>>()
        .join(SEPARATOR)
}

/// The one priority order the rail is read by: a fact lower in the order is
/// given up before any fact above it, so the column and the strip can never
/// disagree about what a frame has no room for. It ends with the posture — a
/// run that does not say what sandbox it is in is the one fact nothing else on
/// the screen stands in for — and puts the numbers the run is spending above
/// what the run *is*, because the identity block is the one part of the rail
/// the transcript's first line already names (ADR-0011 §21, §22).
pub fn priority(fact: &Fact) -> u8 {
    if fact.block() == Block::Posture {
        return 10;
    }
    match fact.label() {
        "context" => 9,
        "tokens" => 8,
        "turns" => 7,
        "cache" => 6,
        "stop" => 5,
        "model" => 4,
        "workspace" => 3,
        "branch" => 2,
        "session" => 1,
        // The session temp is the fact a reader can be told again at any
        // time: it goes first.
        _ => 0,
    }
}

/// The strip where the line can hold the facts it may never lose: every fact
/// in the order the column reads it, each one given the room left over after
/// what the facts after it need is held back.
fn read(facts: &[Fact], width: usize) -> Vec<(usize, String)> {
    let mut shown: Vec<(usize, String)> = Vec::new();
    for (at, fact) in facts.iter().enumerate() {
        let after: usize = facts[at + 1..]
            .iter()
            .filter(|later| later.keeps())
            .map(|later| later.brief().chars().count() + GAP)
            .sum();
        match whole(fact, room(width, &shown, after)) {
            Some(entry) => shown.push((at, entry)),
            // Shortened rather than skipped, as on a narrow line: a word of a
            // fact that did not fit whole is still worth the cells.
            None => {
                if let Some(entry) = fact.cut(room(width, &shown, after)) {
                    shown.push((at, entry));
                }
            }
        }
    }
    shown
}

/// The strip where the line cannot: the facts it may never lose are given
/// their room in the one priority order, the posture first, each one shortened
/// rather than dropped while a word of it can still be said, and what is left
/// of the line goes to the rest.
fn narrow(facts: &[Fact], width: usize) -> Vec<(usize, String)> {
    let mut shown: Vec<(usize, String)> = Vec::new();
    let mut kept: Vec<usize> = (0..facts.len()).filter(|at| facts[*at].keeps()).collect();
    kept.sort_by_key(|at| std::cmp::Reverse(priority(&facts[*at])));
    for (n, at) in kept.iter().enumerate() {
        // What the kept facts after this one need at the very least, so the
        // first of them cannot eat the whole line.
        let after: usize = kept[n + 1..]
            .iter()
            .map(|later| facts[*later].least() + GAP)
            .sum();
        let room = room(width, &shown, after);
        let entry = whole(&facts[*at], room).or_else(|| facts[*at].cut(room));
        if let Some(entry) = entry {
            shown.push((*at, entry));
        }
    }
    let rest: Vec<usize> = (0..facts.len())
        .filter(|at| !facts[*at].keeps() && !shown.iter().any(|(drawn, _)| drawn == at))
        .collect();
    for at in rest {
        let room = room(width, &shown, 0);
        let entry = whole(&facts[at], room).or_else(|| facts[at].cut(room));
        if let Some(entry) = entry {
            shown.push((at, entry));
        }
    }
    shown
}

/// How many cells are left for one more entry on the strip: the line's own
/// width, less what is already on it, the separator before it, and whatever is
/// held back for the facts that must follow.
fn room(width: usize, shown: &[(usize, String)], reserved: usize) -> usize {
    let used: usize = shown.iter().map(|(_, entry)| entry.chars().count()).sum();
    let separators = shown.len() * GAP;
    width.saturating_sub(used + separators + reserved)
}

/// One fact's whole entry — the column's own label where it fits, the brief
/// one where it does not — in `room` cells.
fn whole(fact: &Fact, room: usize) -> Option<String> {
    [fact.line(), fact.brief()]
        .into_iter()
        .find(|entry| entry.chars().count() <= room)
}

#[cfg(test)]
mod tests {
    use super::{Fact, priority};
    use crate::rail::{Launch, Rail};
    use std::time::Duration;

    fn facts() -> Vec<Fact> {
        Rail::new(Launch {
            session: "4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77".to_string(),
            temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
            branch: Some("t30-the-chrome".to_string()),
            sandbox: "bwrap · 1 writable".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            workspace: "~/git/sindri".to_string(),
            max_turns: 250,
            context_limit: Some(200_000),
        })
        .facts(Duration::ZERO)
    }

    /// A fact by the label the rail states it under.
    fn ranked(facts: &[Fact], label: &str) -> u8 {
        priority(
            facts
                .iter()
                .find(|fact| fact.label() == label)
                .expect("the rail states every fact"),
        )
    }

    #[test]
    fn the_priority_order_ends_with_the_posture_and_gives_the_temp_up_first() {
        let facts = facts();
        // The one order both the column and the strip read, least important
        // first: the posture is the fact nothing else on the screen stands in
        // for, and it ends the list (ADR-0011 §22).
        let posture = ranked(&facts, "sandbox");
        let mut order: Vec<u8> = facts.iter().map(priority).collect();
        order.sort_unstable();
        assert_eq!(order.last(), Some(&posture));
        assert!(
            order.windows(2).all(|pair| pair[0] < pair[1]),
            "every fact has its own place in the order: {order:?}"
        );
        assert_eq!(ranked(&facts, "temp"), *order.first().expect("a first"));
        // The numbers outrank the identity, because the first frame already
        // names the run, and the meter outranks the rest of the numbers.
        assert!(ranked(&facts, "context") > ranked(&facts, "tokens"));
        assert!(ranked(&facts, "tokens") > ranked(&facts, "turns"));
        assert!(ranked(&facts, "turns") > ranked(&facts, "cache"));
        assert!(ranked(&facts, "cache") > ranked(&facts, "model"));
        assert!(ranked(&facts, "model") > ranked(&facts, "branch"));
        assert!(ranked(&facts, "branch") > ranked(&facts, "session"));
    }
}
