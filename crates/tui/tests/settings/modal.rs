//! The modal itself: every key of the schema on screen, a save that applies,
//! a refusal that writes nothing, and a draft `Esc` throws away.

use super::*;

#[test]
fn the_thinking_toggle_is_a_keystroke_and_lands_in_the_file() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    for _ in 0..6 {
        down(&mut f.app);
    }
    assert!(
        row(&screen(&mut f.app, 100, 30), "thinking").contains("off"),
        "the file said nothing, so the toggle shows the run's default"
    );
    press(&mut f.app, KeyCode::Char(' '), KeyModifiers::NONE);
    assert!(
        row(&screen(&mut f.app, 100, 30), "thinking").contains("on"),
        "space flips it"
    );
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        written(&f.path).contains("thinking = true"),
        "and saving writes it: {}",
        written(&f.path)
    );
}

#[test]
fn the_extra_writable_paths_are_a_field_the_run_would_accept() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    for _ in 0..7 {
        down(&mut f.app);
    }
    type_text(&mut f.app, "/usr/share");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        written(&f.path).contains("/usr/share"),
        "the path is in the file: {}",
        written(&f.path)
    );
}

#[test]
fn an_unknown_key_in_the_file_refuses_the_modal_and_says_why() {
    let mut f = fixture(Some("model = \"m\"\nmax_turn = 5\n"));
    open(&mut f.app);

    assert!(
        f.app.overlay().is_none(),
        "no modal opens over a broken file"
    );
    let said = notes(&f.app);
    assert!(
        said.contains("max_turn") && said.contains(&f.path.display().to_string()),
        "the transcript names the key and the file: {said}"
    );
    assert_eq!(
        written(&f.path),
        "model = \"m\"\nmax_turn = 5\n",
        "and nothing was rewritten"
    );
}

#[test]
fn the_modal_fits_a_hundred_columns_and_a_hundred_and_sixty() {
    let mut f = fixture(Some("model = \"qwen3-32b\"\napi_key = \"sk-hidden\"\n"));
    open(&mut f.app);
    for _ in 0..3 {
        down(&mut f.app);
    }
    type_text(&mut f.app, "sk-typed-secret");
    for width in [100u16, 160] {
        let rows = screen(&mut f.app, width, 30);
        let text = shown(&rows);
        for wanted in [
            "settings",
            "provider",
            "model",
            "api_key",
            "thinking",
            "extra_headers",
            "plaintext",
            "Enter save",
            "Esc discard",
        ] {
            assert!(
                text.contains(wanted),
                "{width} columns shows {wanted}: {text}"
            );
        }
        assert!(
            !text.contains("sk-typed-secret") && !text.contains("sk-hidden"),
            "{width} columns never shows a secret: {text}"
        );
    }
}

#[test]
fn the_modal_dims_the_frame_it_covers_and_reads_at_full_strength_itself() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    let buf = buffer(&mut f.app, 100, 30);

    assert!(
        buf[(0, 0)].modifier.contains(Modifier::DIM),
        "what the modal leaves uncovered recedes instead of reading as a hole"
    );
    let title = (0..30)
        .find(|y| {
            (0..100)
                .map(|x| buf[(x, *y)].symbol())
                .collect::<String>()
                .contains(" settings ")
        })
        .expect("the modal's title row");
    // The rail keeps showing to the right of the modal, dimmed: the modal's
    // own columns are the ones between its two corners.
    let left = (0..100)
        .find(|x| buf[(*x, title)].symbol() == "╭")
        .expect("the modal's left corner");
    let right = (left..100)
        .find(|x| buf[(*x, title)].symbol() == "╮")
        .expect("the modal's right corner");
    assert!(
        (left..right)
            .filter(|x| buf[(*x, title)].symbol() != " ")
            .all(|x| !buf[(x, title)].modifier.contains(Modifier::DIM)),
        "and the modal itself is drawn at full strength: {title}"
    );
}
