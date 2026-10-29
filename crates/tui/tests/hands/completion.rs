//! `@` completion: what the walk offers, how many rows, which dotfiles, and the
//! keys that take or dismiss an offer.

use super::*;

#[test]
fn at_completes_workspace_paths_in_the_background() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "look at @src/ma");
    let (generation, typed) = app
        .take_walk()
        .expect("typing an @ path asks for a background walk");
    assert_eq!(typed, "src/ma", "the walk ranks against what was typed");

    // The walk answers on its own thread; the frame is told what it found.
    let found: Vec<String> = (0..20).map(|index| format!("src/main{index}.rs")).collect();
    app.paths(generation, found, false);
    assert_eq!(
        app.completion().map(|completion| completion.items.len()),
        Some(8),
        "about eight rows, never the tree"
    );
    let rows = shown(&screen(&mut app, 100, 24));
    assert!(
        rows.contains("src/main0.rs"),
        "the offers are drawn: {rows}"
    );

    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(
        app.composer().text(),
        "look at src/main0.rs",
        "Tab takes the offer and replaces what was typed after the @"
    );
    assert!(app.completion().is_none(), "and the offer closes");
    assert_eq!(app.queued(), 0, "accepting a path does not send the prompt");
}

#[test]
fn enter_takes_a_completion_and_esc_dismisses_one() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "@mai");
    let generation = app
        .take_walk()
        .map(|(generation, _)| generation)
        .unwrap_or(0);
    app.paths(
        generation,
        vec!["main.rs".to_string(), "src/main.rs".to_string()],
        false,
    );
    screen(&mut app, 100, 24);

    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.completion().is_none(), "Esc dismisses the completion");
    assert_eq!(
        app.composer().text(),
        "@mai",
        "and changes nothing that was typed"
    );

    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let job = app
        .start()
        .expect("Enter with no completion offered sends the prompt");
    assert_eq!(job.prompt, "@mai");
}

#[test]
fn a_dotfile_is_offered_only_when_a_dot_is_typed() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "@gi");
    let generation = app
        .take_walk()
        .map(|(generation, _)| generation)
        .unwrap_or(0);
    app.paths(
        generation,
        vec![".gitignore".to_string(), "gist.md".to_string()],
        false,
    );
    assert_eq!(
        app.completion().map(|completion| completion.items.clone()),
        Some(vec!["gist.md".to_string()])
    );

    let mut dotted = app_with(Fake::taking());
    type_text(&mut dotted, "@.gi");
    let generation = dotted
        .take_walk()
        .map(|(generation, _)| generation)
        .unwrap_or(0);
    dotted.paths(generation, vec![".gitignore".to_string()], false);
    assert_eq!(
        dotted
            .completion()
            .map(|completion| completion.items.clone()),
        Some(vec![".gitignore".to_string()]),
        "a dot asks for dotfiles"
    );
}

#[test]
fn the_arrow_keys_walk_the_completion_so_every_offer_is_reachable() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "@mai");
    let generation = app
        .take_walk()
        .map(|(generation, _)| generation)
        .unwrap_or(0);
    app.paths(
        generation,
        vec![
            "main.rs".to_string(),
            "main2.rs".to_string(),
            "main3.rs".to_string(),
        ],
        false,
    );
    screen(&mut app, 100, 24);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(
        app.completion().map(|completion| completion.chosen),
        Some(2),
        "Down walks the offers, not the composer"
    );
    assert!(shown(&screen(&mut app, 100, 24)).contains("▸ main3.rs"));
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(
        app.composer().text(),
        "main3.rs",
        "Tab takes the marked offer, the trigger included"
    );
}

#[test]
fn a_walk_that_stopped_at_its_cap_says_so() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "@src");
    let generation = app
        .take_walk()
        .map(|(generation, _)| generation)
        .unwrap_or(0);
    app.paths(generation, vec!["src/main.rs".to_string()], true);
    let rows = shown(&screen(&mut app, 100, 24));
    assert!(
        rows.contains("stopped at its cap"),
        "a completion that is not everything says so: {rows}"
    );
}
