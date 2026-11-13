//! What a terminal's bytes become, proved where a caller can reach it: no terminal, no built
//! binary, and nothing here asks whether a stream is one.

use sindri::composer::Key;
use sindri::decode::{Decoder, Event};

fn events(chunks: &[&[u8]]) -> Vec<Event> {
    let mut decoder = Decoder::new();
    let mut events = Vec::new();
    for chunk in chunks {
        decoder.push(chunk);
        while let Some(event) = decoder.next_event() {
            events.push(event);
        }
    }
    events
}

fn event(bytes: &[u8]) -> Event {
    let mut all = events(&[bytes]);
    assert_eq!(
        all.len(),
        1,
        "one key is one event, and these bytes made {all:?}"
    );
    all.pop().expect("the event")
}

fn key(key: Key) -> Event {
    Event::Key(key)
}

#[test]
fn a_bare_escape_is_the_key_that_closes_a_screen() {
    assert_eq!(
        event(b"\x1b"),
        key(Key::Escape),
        "one ESC read on its own is the Esc key, and not the start of a sequence \
         that never came"
    );
}

#[test]
fn a_glyph_typed_is_one_key() {
    assert_eq!(event(b"a"), key(Key::Char('a')));
}

#[test]
fn a_glyph_split_across_two_reads_arrives_whole() {
    assert_eq!(events(&[&[0xe2, 0x9c], &[0x93]]), vec![key(Key::Char('✓'))]);
}

#[test]
fn ctrl_j_is_the_newline_every_terminal_sends() {
    assert_eq!(event(b"\n"), key(Key::Newline));
}

#[test]
fn a_reported_shift_enter_breaks_the_line() {
    assert_eq!(event(b"\x1b[13;2u"), key(Key::Newline));
}

#[test]
fn a_reported_ctrl_enter_breaks_the_line() {
    assert_eq!(
        event(b"\x1b[13;5u"),
        key(Key::Newline),
        "Ctrl+Enter is a line break of the draft, and never the key that sends one"
    );
}

#[test]
fn a_reported_enter_without_a_modifier_still_sends() {
    assert_eq!(event(b"\x1b[13u"), key(Key::Enter));
}

#[test]
fn a_reported_key_is_the_key_the_byte_beside_it_would_be() {
    // A terminal asked for the protocol reports the keys that name no glyph as `CSI code;mods u`,
    // so every key the frame answers has to be answered in that spelling too.
    for (bytes, named) in [
        (&b"\x1b[99;5u"[..], Key::Cancel),
        (&b"\x1b[113;5u"[..], Key::Quit),
        (&b"\x1b[120;5u"[..], Key::Drop),
        (&b"\x1b[121;5u"[..], Key::Copy),
        (&b"\x1b[117;5u"[..], Key::Clear),
        (&b"\x1b[106;5u"[..], Key::Newline),
        (&b"\x1b[104;5u"[..], Key::Backspace),
        (&b"\x1b[109;5u"[..], Key::Enter),
        (&b"\x1b[105;5u"[..], Key::Tab),
        (&b"\x1b[91;5u"[..], Key::Escape),
        (&b"\x1b[127u"[..], Key::Backspace),
        (&b"\x1b[27u"[..], Key::Escape),
        (&b"\x1b[9u"[..], Key::Tab),
        (&b"\x1b[9;2u"[..], Key::BackTab),
    ] {
        assert_eq!(
            event(bytes),
            key(named),
            "a report of {:?} is that key and no other",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn a_lock_a_report_carries_beside_its_modifiers_names_no_other_key() {
    // A report's modifier field carries the locks the keyboard is in as well as the keys held, and
    // a reader with caps lock on still types `Ctrl-C` rather than a key nobody answers.
    assert_eq!(
        event(b"\x1b[99;69u"),
        key(Key::Cancel),
        "caps lock and Ctrl"
    );
    assert_eq!(event(b"\x1b[113;133u"), key(Key::Quit), "num lock and Ctrl");
    assert_eq!(
        event(b"\x1b[13;66u"),
        key(Key::Newline),
        "caps lock and Shift"
    );
}

#[test]
fn a_report_of_an_alternate_key_is_the_key_before_the_colon() {
    assert_eq!(
        event(b"\x1b[13:13;2u"),
        key(Key::Newline),
        "a report says which glyph a layout makes of the key after a colon, and the key \
         before it is the one the frame answers"
    );
}

#[test]
fn a_report_nobody_answers_is_not_a_key() {
    assert!(
        events(&[b"\x1b[97;5u"]).is_empty(),
        "Ctrl-A names no act, and a report of it is not a glyph of the draft either"
    );
    assert!(
        events(&[b"\x1b[13;3u"]).is_empty(),
        "Alt+Enter names no act: a line break nobody asked for is worse than none"
    );
    assert_eq!(
        events(&[b"\x1b[97;5u", b"a"]),
        vec![key(Key::Char('a'))],
        "the key behind a report nobody answers is still read"
    );
}

#[test]
fn ctrl_x_is_the_byte_that_drops_the_newest_queued_prompt() {
    assert_eq!(event(b"\x18"), key(Key::Drop));
}

#[test]
fn ctrl_q_quits_and_ctrl_c_cancels() {
    assert_eq!(event(b"\x11"), key(Key::Quit));
    assert_eq!(event(b"\x03"), key(Key::Cancel));
}

#[test]
fn backspace_arrives_as_either_byte_raw_mode_leaves_it() {
    assert_eq!(event(b"\x7f"), key(Key::Backspace));
    assert_eq!(event(b"\x08"), key(Key::Backspace));
}

#[test]
fn the_arrows_arrive_in_either_spelling() {
    assert_eq!(event(b"\x1b[A"), key(Key::Up));
    assert_eq!(event(b"\x1bOB"), key(Key::Down), "the older spelling");
    assert_eq!(event(b"\x1b[C"), key(Key::Right));
    assert_eq!(event(b"\x1b[D"), key(Key::Left));
}

#[test]
fn home_and_end_arrive_in_either_spelling() {
    assert_eq!(event(b"\x1b[H"), key(Key::Home));
    assert_eq!(event(b"\x1b[1~"), key(Key::Home));
    assert_eq!(event(b"\x1b[F"), key(Key::End));
    assert_eq!(event(b"\x1b[4~"), key(Key::End));
}

#[test]
fn a_sequence_read_in_pieces_is_still_one_key() {
    assert_eq!(
        events(&[b"\x1b[", b"13;", b"2u"]),
        vec![key(Key::Newline)],
        "a key split across three reads is one key, not four"
    );
}

#[test]
fn a_bracketed_paste_is_one_event_however_many_lines_it_holds() {
    assert_eq!(
        event(b"\x1b[200~one\r\ntwo\rthree\x1b[201~"),
        Event::Paste(String::from("one\r\ntwo\rthree")),
        "the clipboard's text, whole, and not one event per line"
    );
}

#[test]
fn a_bracketed_paste_split_across_reads_is_one_event() {
    assert_eq!(
        events(&[b"\x1b[200~one\r", b"two\r", b"three", b"\x1b[201~"]),
        vec![Event::Paste(String::from("one\rtwo\rthree"))],
        "a paste arrives a read at a time, and the reads are one paste"
    );
}

#[test]
fn a_paste_that_has_not_ended_yet_is_not_a_key() {
    assert!(
        events(&[b"\x1b[200~one\r"]).is_empty(),
        "no line break inside a paste can escape as the key that sends it"
    );
}

#[test]
fn a_paste_that_arrives_as_a_burst_sends_nothing() {
    let events = events(&[b"one\rtwo\r"]);
    assert!(
        !events.contains(&key(Key::Enter)),
        "a terminal without bracketed paste sends a paste as pressed keys, and \
         no line break in one may send: {events:?}"
    );
    assert_eq!(events.len(), 8, "one event per glyph and per line break");
}

#[test]
fn the_enter_key_is_told_from_a_paste_line_break() {
    assert_eq!(
        event(b"\r"),
        key(Key::Enter),
        "a carriage return read on its own is the key that sends"
    );
    assert_eq!(
        events(&[b"one\rtwo"]),
        vec![
            key(Key::Char('o')),
            key(Key::Char('n')),
            key(Key::Char('e')),
            key(Key::Newline),
            key(Key::Char('t')),
            key(Key::Char('w')),
            key(Key::Char('o')),
        ],
        "the same byte inside a burst is a paste's line break, and no line break may send"
    );
}

#[test]
fn ctrl_y_arrives_as_the_key_that_copies() {
    assert_eq!(
        event(b"\x19"),
        key(Key::Copy),
        "`Ctrl-Y` is one control byte, and it is the key the mark is copied with \
         rather than a glyph of the draft"
    );
}

#[test]
fn a_shifted_arrow_arrives_as_the_key_that_grows_the_mark() {
    assert_eq!(
        event(b"\x1b[1;2A"),
        key(Key::ShiftUp),
        "a modified arrow carries its modifiers between the bracket and the final \
         byte, which is how a terminal says shift was held"
    );
    assert_eq!(event(b"\x1b[1;2B"), key(Key::ShiftDown));
}

#[test]
fn a_key_after_a_paste_is_read_again() {
    assert_eq!(
        events(&[b"\x1b[200~a\x1b[201~", b"\r"]),
        vec![Event::Paste(String::from("a")), key(Key::Enter)],
        "the paste ends where its sequence ends, and the next key is a key"
    );
}

#[test]
fn a_terminal_that_reports_focus_is_heard_as_the_window_being_read_and_left() {
    assert_eq!(
        event(b"\x1b[I"),
        Event::Focus(true),
        "`CSI I` is the report a terminal sends when its window becomes the one being read, \
         and it is no key: nothing a reader types arrives as it"
    );
    assert_eq!(
        event(b"\x1b[O"),
        Event::Focus(false),
        "and `CSI O` is the report that nobody is looking"
    );
}
