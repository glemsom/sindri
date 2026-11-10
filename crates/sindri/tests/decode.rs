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
fn shift_enter_under_the_kitty_protocol_breaks_the_line() {
    assert_eq!(event(b"\x1b[13;2u"), key(Key::Newline));
}

#[test]
fn a_kitty_enter_report_without_a_modifier_still_sends() {
    assert_eq!(event(b"\x1b[13u"), key(Key::Enter));
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
