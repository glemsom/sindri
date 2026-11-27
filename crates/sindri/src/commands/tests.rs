use super::*;

fn spelling(draft: &str) -> String {
    Card::of(draft).expect("a card holds this draft").spelling()
}

fn names(draft: &str) -> Vec<String> {
    Card::of(draft)
        .map(|card| card.rows().iter().map(|row| row.name.to_string()).collect())
        .unwrap_or_default()
}

#[test]
fn a_bare_slash_holds_every_command() {
    let held = names("/");

    assert_eq!(
        held.len(),
        TABLE.len(),
        "every command starts the empty word, so the card holds them all: {held:?}"
    );
    for row in TABLE {
        assert!(
            held.contains(&row.name.to_string()),
            "the card holds no row for {:?}: {held:?}",
            row.name
        );
    }
}

#[test]
fn a_word_no_command_holds_holds_no_card() {
    for draft in [
        "/skill:commit",
        "/skill commit",
        "/skill",
        "/skillet",
        "/usr",
        "/etc/hosts",
        "//",
        "/src/rows.rs",
    ] {
        assert!(
            Card::of(draft).is_none(),
            "{draft:?} names no command, so it is a draft the reader meant to send"
        );
    }
}

#[test]
fn the_word_the_draft_names_is_sent_as_the_command_it_stands_for() {
    assert_eq!(spelling("/mod"), "/model");
    assert_eq!(spelling("/model"), "/model");
    assert_eq!(spelling("/model gpt-5"), "/model gpt-5");
    assert_eq!(spelling("/qui"), "/quit");
    assert_eq!(spelling("/quit "), "/quit ");
    assert_eq!(
        spelling("/"),
        HELP,
        "a bare slash names the first row of the list, which is the one `Enter` sends"
    );
}

#[test]
fn every_row_of_the_card_spells_the_line_it_sends() {
    let card = Card::of("/n").expect("a card holds this word");

    assert_eq!(
        card.spelling_of(0),
        Some(String::from(NEW)),
        "the first row the card draws is the line `Enter` sends"
    );
    assert_eq!(
        card.spelling_of(1),
        Some(String::from(SETTINGS)),
        "and a row under it spells its own name with what the word left"
    );
    assert_eq!(
        card.spelling_of(2),
        None,
        "a row the card does not hold spells nothing at all"
    );
}

#[test]
fn a_draft_whose_first_glyph_is_not_a_slash_holds_no_card() {
    for draft in ["and/or", " /mod", ""] {
        assert!(
            Card::of(draft).is_none(),
            "{draft:?} does not begin with a slash, so nothing about it asks for the commands"
        );
    }
}

#[test]
fn the_builtin_names_are_unique() {
    let names: Vec<String> = TABLE.iter().map(|row| row.name.to_string()).collect();
    let mut unique = names.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        names.len(),
        "two rows of the list name one command, so one of them can never be chosen: {names:?}"
    );
}

#[test]
fn every_builtin_name_reads_back_as_a_command() {
    for row in TABLE.iter() {
        assert!(
            read(&row.name).is_some(),
            "the list names {:?} and a line of it is no command the frame answers",
            row.name
        );
    }
}

#[test]
fn the_card_recognises_the_word_it_holds_a_row_for() {
    for word in ["/", "/mod", "/model", "/qui", "/settings", "/stats"] {
        assert!(
            recognised(word),
            "{word:?} is a word a row of the card holds, so the card knows it"
        );
    }
    for word in [
        "/skill:commit",
        "/skill commit",
        "/skill",
        "model",
        "",
        "/skillet",
        "/usr",
        "/etc/hosts",
        "//",
        "/src/rows.rs",
    ] {
        assert!(
            !recognised(word),
            "{word:?} names no row of the card, so it is no command the card knows"
        );
    }
}
