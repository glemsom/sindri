use super::*;

fn decided() -> Config {
    Config {
        model: Some(String::from("claude-sonnet-4-5")),
        thinking: Some(true),
        api_key: Some(String::from("sk-not-a-real-key")),
        ..Config::default()
    }
}

fn shown(settings: &Settings, key: &str) -> String {
    settings
        .rows()
        .find(|(name, _)| *name == key)
        .map(|(_, text)| text.to_owned())
        .unwrap_or_else(|| panic!("the screen has a row for {key}"))
}

fn at(settings: &mut Settings, key: &str) {
    for _ in 0..settings.rows().count() {
        if settings
            .rows()
            .nth(settings.current())
            .map(|(name, _)| name)
            == Some(key)
        {
            return;
        }
        settings.walked(Move::Next);
    }
    panic!("{key} is a key of the schema, so the screen has a row for it");
}

#[test]
fn the_screen_walks_every_key_the_schema_has_in_the_order_the_schema_names_them() {
    let order = [
        "provider",
        "model",
        "route",
        "base_url",
        "api_key",
        "thinking",
        "extra_writable_paths",
        "extra_headers",
        "context_window",
        "max_turns",
        "timeout",
        "command_budget_ms",
        "output_budget_bytes",
        "status",
        "face",
        "ssh_agent",
    ];

    let settings = Settings::of(&Config::default());
    let rows: Vec<&str> = settings.rows().map(|(key, _)| key).collect();

    assert_eq!(
        rows, order,
        "the screen reads the table's rows in the table's order, so a key the schema gains is \
             a row here with nothing to remember"
    );
}

#[test]
fn a_row_starts_holding_the_value_in_force_whichever_layer_spoke_it() {
    let settings = Settings::of(&decided());

    assert_eq!(shown(&settings, "model"), "claude-sonnet-4-5");
    assert_eq!(
        shown(&settings, "thinking"),
        "true",
        "a switch reads as the word it will be written as"
    );
    assert_eq!(
        shown(&settings, "timeout"),
        "",
        "a key nothing speaks for is a row with nothing in it, because unset is a state"
    );
}

#[test]
fn a_secret_is_a_mask_and_the_first_glyph_replaces_it_rather_than_piling_onto_it() {
    let mut settings = Settings::of(&decided());
    at(&mut settings, "api_key");
    assert_eq!(
        shown(&settings, "api_key"),
        MASK,
        "a frame may not hold a secret, so the row says a key is set and not which"
    );

    settings.key(Key::Char('n'));

    assert_eq!(
        shown(&settings, "api_key"),
        "n",
        "the mask is what the file holds, not what the reader typed, so a glyph \
             replaces it: a mask saved as a key would be a key of bullets"
    );
}

#[test]
fn a_screen_no_frame_opened_holds_no_row_and_no_key_changes_one() {
    let mut settings = Settings::default();

    assert!(!settings.clear());
    assert!(!settings.space());
    assert!(!settings.key(Key::Char('x')));
    assert!(!settings.walked(Move::Next));
    assert!(
        settings.touched().is_empty(),
        "a visit to a screen that was never opened is a file with nothing to write"
    );
}

#[test]
fn ctrl_u_empties_the_value_of_the_row_a_key_acts_on() {
    let mut settings = Settings::of(&decided());
    at(&mut settings, "model");

    assert!(settings.clear());
    assert_eq!(
        shown(&settings, "model"),
        "",
        "an emptied value is a key handed back to the environment, so the row that held \
             the file's model holds nothing"
    );
    assert_eq!(
        settings.touched().rows().collect::<Vec<_>>(),
        vec![("model", "")],
        "and it is the one row the visit will write"
    );
}

#[test]
fn ctrl_u_on_a_row_already_empty_is_not_a_key_that_repaints() {
    let mut settings = Settings::of(&Config::default());
    at(&mut settings, "timeout");

    assert!(!settings.clear());
    assert!(
        settings.touched().is_empty(),
        "there was nothing to empty, so there is nothing to write"
    );
}

#[test]
fn space_flips_a_switch_from_unset_to_on_and_back_off_again() {
    let mut settings = Settings::of(&Config::default());
    at(&mut settings, "ssh_agent");

    settings.space();
    assert_eq!(
        shown(&settings, "ssh_agent"),
        "true",
        "a switch neither on nor off is what a reader pressing it means"
    );
    settings.space();
    assert_eq!(shown(&settings, "ssh_agent"), "false");
    assert_eq!(
        settings.touched().rows().collect::<Vec<_>>(),
        vec![("ssh_agent", "false")]
    );
}

#[test]
fn space_on_a_key_that_is_not_a_switch_is_a_glyph() {
    let mut settings = Settings::of(&Config::default());
    at(&mut settings, "extra_writable_paths");
    settings.key(Key::Char('/'));
    settings.key(Key::Char('a'));

    settings.space();
    settings.key(Key::Char('b'));

    assert_eq!(
        shown(&settings, "extra_writable_paths"),
        format!("/a{}b", ' '),
        "a path may hold a space, and a screen that could not type one could not name a \
             path: only the schema's own switches flip"
    );
}

#[test]
fn space_flips_every_row_the_table_calls_a_switch_and_types_into_every_other() {
    let switches: Vec<&str> = Config::SCHEMA
        .iter()
        .filter(|field| field.switch())
        .map(|field| field.name)
        .collect();
    assert_eq!(
        switches,
        vec!["thinking", "status", "face", "ssh_agent"],
        "the space flips a key only where the table names it a switch"
    );

    let set = Config {
        thinking: Some(true),
        status: Some(false),
        face: Some(true),
        ssh_agent: Some(false),
        ..Config::default()
    };
    let mut settings = Settings::of(&set);

    for (name, before, after) in [
        ("thinking", "true", "false"),
        ("status", "false", "true"),
        ("face", "true", "false"),
        ("ssh_agent", "false", "true"),
    ] {
        at(&mut settings, name);
        assert_eq!(
            shown(&settings, name),
            before,
            "the row of `{name}` holds the word the config does"
        );
        settings.space();
        assert_eq!(
            shown(&settings, name),
            after,
            "the space flips `{name}`, and every other row it left alone still holds its own"
        );
    }

    let mut typed = Settings::of(&Config::default());
    for field in Config::SCHEMA.iter().filter(|field| !field.switch()) {
        at(&mut typed, field.name);
        typed.space();
        assert_eq!(
            shown(&typed, field.name),
            " ",
            "the table calls `{}` no switch, so space is a glyph in its row",
            field.name
        );
    }
}

#[test]
fn a_glyph_lands_in_the_row_the_keys_act_on_and_leaves_every_other_row_alone() {
    let mut settings = Settings::of(&Config::default());
    at(&mut settings, "model");
    settings.key(Key::Char('a'));
    settings.key(Key::Char('b'));
    settings.key(Key::Backspace);
    at(&mut settings, "max_turns");
    settings.key(Key::Char('4'));

    assert_eq!(shown(&settings, "model"), "a");
    assert_eq!(
        settings.touched().rows().collect::<Vec<_>>(),
        vec![("max_turns", "4"), ("model", "a")],
        "the patch is the rows a reader wrote into, in the schema's order"
    );
    assert_eq!(
        shown(&settings, "base_url"),
        "",
        "a row nobody reached is a row with nothing in it"
    );
}

#[test]
fn a_backspace_erases_the_last_glyph_and_takes_a_whole_mask_with_it() {
    let mut settings = Settings::of(&decided());
    at(&mut settings, "model");
    settings.key(Key::Backspace);
    assert_eq!(shown(&settings, "model"), "claude-sonnet-4-");

    at(&mut settings, "api_key");
    settings.key(Key::Backspace);
    assert_eq!(
        shown(&settings, "api_key"),
        "",
        "a secret is not something a screen may hold even between two keystrokes, so a \
             backspace on the mask unsets the row whole rather than nibbling it"
    );
}

#[test]
fn a_refusal_is_said_until_the_reader_writes_something_else() {
    let mut settings = Settings::of(&decided());
    at(&mut settings, "max_turns");
    settings.key(Key::Char('x'));

    settings.refuse("bad value in max_turns: `x` will not parse");
    assert!(
        settings
            .refusal()
            .is_some_and(|reason| reason.contains("max_turns")),
        "a refusal names the key a reader has to go and fix"
    );

    settings.key(Key::Char('1'));
    assert_eq!(
        settings.refusal(),
        None,
        "the reason was about the value that was there, and it has been written over"
    );
}

#[test]
fn wire_selection_uses_supported_words_and_esc_leaves_the_draft_untouched() {
    let mut settings = Settings::of(&Config::default());
    assert!(!settings.activate());
    assert!(!settings.key(Key::Char('x')));
    settings.walked(Move::Next);
    assert!(settings.cancel_editor());
    assert!(settings.touched().is_empty());
    settings.activate();
    settings.walked(Move::Next);
    settings.activate();
    assert_eq!(shown(&settings, "provider"), "openai");
}

#[test]
fn anthropic_has_no_route_editor_and_switching_to_it_clears_an_override() {
    let mut settings = Settings::of(&Config {
        provider: Some(Wire::Openai),
        route: Some(sindri_core::provider::Route::Chat),
        ..Config::default()
    });
    settings.activate();
    settings.walked(Move::Previous);
    settings.activate();
    assert_eq!(shown(&settings, "route"), "");
    at(&mut settings, "route");
    settings.activate();
    assert!(!settings.editing());
}

#[test]
fn automatic_route_is_an_empty_override() {
    let mut settings = Settings::of(&Config {
        provider: Some(Wire::Openai),
        route: Some(sindri_core::provider::Route::Responses),
        ..Config::default()
    });
    at(&mut settings, "route");
    settings.activate();
    settings.walked(Move::Previous);
    settings.walked(Move::Previous);
    settings.activate();
    assert_eq!(shown(&settings, "route"), "");
}

#[test]
fn model_discovery_can_be_replaced_by_manual_entry_and_cancel_restores_the_value() {
    let mut settings = Settings::of(&decided());
    at(&mut settings, "model");
    assert!(settings.activate());
    assert!(settings.clear());
    settings.clear();
    settings.key(Key::Char('x'));
    assert!(settings.cancel_editor());
    assert_eq!(shown(&settings, "model"), "claude-sonnet-4-5");
    assert!(settings.touched().is_empty());
}

#[test]
fn draft_discovery_uses_changed_endpoint_and_preserves_unedited_credentials() {
    let mut settings = Settings::of(&decided());
    at(&mut settings, "base_url");
    settings.activate();
    for glyph in "https://example.test".chars() {
        settings.key(Key::Char(glyph));
    }
    settings.activate();
    let draft = settings.draft(&decided()).unwrap();
    assert_eq!(draft.base_url.as_deref(), Some("https://example.test"));
    assert_eq!(draft.api_key, decided().api_key);
}

#[test]
fn automatic_route_can_be_used_to_discover_models_from_the_draft() {
    let base = Config {
        provider: Some(Wire::Openai),
        route: Some(sindri_core::provider::Route::Chat),
        ..Config::default()
    };
    let mut settings = Settings::of(&base);
    at(&mut settings, "route");
    settings.activate();
    settings.walked(Move::Previous);
    settings.activate();
    assert_eq!(settings.draft(&base).unwrap().route, None);
}
