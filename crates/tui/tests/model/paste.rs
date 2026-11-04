//! A pasted fragment: a name copied from elsewhere is what the screen narrows
//! by, and a screen carrying a filter takes it the way it takes a letter.

use super::*;
use sindri_tui::overlay::Overlay;

/// The fragment the screen holds, read off the screen rather than off a row a
/// redraw may have already narrowed.
fn filter(f: &Fixture) -> String {
    let Some(Overlay::Model(model)) = f.app.overlay() else {
        panic!("the model screen is open");
    };
    model.filter().to_string()
}

#[test]
fn a_pasted_fragment_narrows_the_names_like_a_typed_one() {
    let mut f = offered(None, &["gpt-5", "qwen3-32b"]);
    assert_eq!(marked(&mut f), "gpt-5", "the cursor opens on a name");

    assert!(
        f.app.paste("qwen"),
        "the screen takes the paste: it has the screen, and a fragment is what \
         it filters by"
    );

    assert_eq!(
        filter(&f),
        "qwen",
        "the pasted fragment is the screen's, as one paste and not one key"
    );
    assert_eq!(
        marked(&mut f),
        "qwen3-32b",
        "and the names are ranked again on the frame it landed on"
    );
}
