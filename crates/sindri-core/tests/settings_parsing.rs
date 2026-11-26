use sindri_core::config::{Config, FieldKind};

#[test]
fn schema_names_semantic_kinds_and_boolean_serialized_choices() {
    for (name, kind) in [
        ("provider", FieldKind::Choice),
        ("route", FieldKind::Choice),
        ("model", FieldKind::Text),
        ("base_url", FieldKind::Url),
        ("api_key", FieldKind::Credential),
        ("thinking", FieldKind::Boolean),
        ("extra_writable_paths", FieldKind::Paths),
        ("extra_headers", FieldKind::Headers),
        ("context_window", FieldKind::Integer),
        ("max_turns", FieldKind::Integer),
        ("timeout", FieldKind::DurationSeconds),
        ("command_budget_ms", FieldKind::DurationMilliseconds),
        ("output_budget_bytes", FieldKind::ByteSize),
        ("status", FieldKind::Boolean),
        ("face", FieldKind::Boolean),
        ("ssh_agent", FieldKind::Boolean),
    ] {
        let field = Config::field(name).unwrap();
        assert_eq!(field.kind, kind);
        assert_eq!(field.switch(), kind == FieldKind::Boolean);
        if field.switch() {
            assert_eq!(field.choices(None), Some(vec!["true", "false", ""]));
            assert_eq!(field.read("").unwrap(), Config::default());
            for word in ["true", "false"] {
                assert_eq!(field.shown(&field.read(word).unwrap()), word);
            }
        }
    }
}

#[test]
fn human_quantities_read_and_write_existing_integer_units() {
    for (name, text, expected) in [
        ("timeout", "1h 30m", 5400),
        ("timeout", "1500", 1500),
        ("timeout", "1.5 minutes", 90),
        ("command_budget_ms", "1.5s", 1500),
        ("command_budget_ms", "250ms", 250),
        ("output_budget_bytes", "1.5 KiB", 1536),
        ("output_budget_bytes", "2 MB", 2_000_000),
        ("output_budget_bytes", "4096", 4096),
    ] {
        let field = Config::field(name).unwrap();
        let config = field.read(text).unwrap();
        assert_eq!(field.shown(&config), expected.to_string());
        let mut document = toml_edit::DocumentMut::new();
        field.write(&config, &mut document).unwrap();
        assert_eq!(document[name].as_integer(), Some(expected));
    }
}

#[test]
fn read_refuses_invalid_unsaveable_and_fractional_quantities() {
    for (name, words) in [
        ("context_window", vec!["0", "-1", "9223372036854775808"]),
        ("max_turns", vec!["4294967296", "-1", "1.5"]),
        (
            "timeout",
            vec!["-1s", "500ms", "1fortnight", "9223372036854775808", ""],
        ),
        (
            "command_budget_ms",
            vec!["1us", "1e3s", "NaN", "18446744073709551615s"],
        ),
        (
            "output_budget_bytes",
            vec!["-1B", "0.1B", "1XB", "18446744073709551615KiB"],
        ),
    ] {
        for text in words {
            assert!(
                Config::field(name).unwrap().read(text).is_err(),
                "{name}: {text}"
            );
        }
    }
    for name in [
        "max_turns",
        "timeout",
        "command_budget_ms",
        "output_budget_bytes",
    ] {
        assert!(Config::field(name).unwrap().read("0").is_ok());
    }
}

#[test]
fn urls_require_absolute_http_or_https_with_host() {
    let field = Config::field("base_url").unwrap();
    let accepts = |text: &str| {
        assert_eq!(field.read(text).unwrap().base_url.as_deref(), Some(text));
    };
    accepts("https://example.com/v1");
    accepts("http://localhost:8080");
    accepts("http://[::1]:8000/v1");
    for text in [
        "",
        "/v1",
        "example.com",
        "ftp://example.com",
        "https://",
        "http:///v1",
        "https://bad host",
    ] {
        assert!(field.read(text).is_err(), "{text}");
    }
}

#[test]
fn collection_encoding_preserves_commas_in_paths_and_header_values() {
    for (key, item) in [
        ("extra_writable_paths", "/one,two"),
        ("extra_headers", "Accept: text/plain, application/json"),
    ] {
        let field = Config::field(key).unwrap();
        let text = sindri_core::config::collection_text(&[item.to_owned()]);
        let config = field.read(&text).unwrap();
        let mut document = toml_edit::DocumentMut::new();
        field.write(&config, &mut document).unwrap();
        assert_eq!(
            document[key].as_array().unwrap().get(0).unwrap().as_str(),
            Some(item)
        );
    }
}
