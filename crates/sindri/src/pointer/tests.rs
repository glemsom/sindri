use super::*;

#[test]
fn a_pointer_is_asked_of_the_terminals_a_hyperlink_is_placed_on_and_of_no_other() {
    assert!(
        speaks(Some("xterm-kitty"), None, None),
        "a terminal that names itself after one that reports a pointer is asked for one"
    );
    assert!(
        speaks(Some("xterm-256color"), Some("WezTerm"), None),
        "and one named by the program that runs it"
    );
    assert!(
        speaks(Some("xterm-256color"), None, Some("6000")),
        "a VTE places a hyperlink and reports a pointer from the version that learned one"
    );
    assert!(
        !speaks(Some("xterm-256color"), None, Some("4900")),
        "and an older one is handed a frame that takes none"
    );
    assert!(
        !speaks(Some("xterm-256color"), None, None),
        "an xterm names no pointer this frame knows how to read"
    );
    assert!(
        !speaks(None, None, None),
        "and neither does a terminal that names nothing"
    );
}
