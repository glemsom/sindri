use std::io::{self, Read};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Duration;

use super::*;

struct Chatter {
    permits: Receiver<()>,
}

impl Read for Chatter {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.permits.recv().is_err() {
            return Ok(0);
        }
        buffer[..1].copy_from_slice(b"x");
        Ok(1)
    }
}

#[test]
fn the_first_line_a_stream_said_comes_back_trimmed_and_alone() {
    assert_eq!(
        first_line(b"no handler for .html\ntry a browser\n"),
        "no handler for .html",
        "a reader is handed the reason and not a wall of the same complaint"
    );
}

#[test]
fn a_stream_that_ended_without_a_newline_still_said_a_line() {
    assert_eq!(
        first_line(b"  no handler for .html  "),
        "no handler for .html",
        "an opener that ends its words without a newline still said them"
    );
}

#[test]
fn a_line_still_being_written_past_the_last_newline_is_no_line() {
    assert_eq!(
        first_line(b"first line\nno handler for .ht"),
        "first line",
        "a line still being written when the grace ran out is not a line the opener finished"
    );
}

#[test]
fn a_stream_that_said_nothing_has_no_line() {
    assert_eq!(first_line(b""), "");
    assert_eq!(first_line(b"\n\n"), "");
}

#[test]
fn a_stream_a_call_has_given_up_on_keeps_no_more_of_what_it_says() {
    let (permits, granted) = channel();
    let stream = Stream::drain(Some(Chatter { permits: granted }));
    for _ in 0..3 {
        permits.send(()).expect("the reader is waiting");
    }
    thread::sleep(Duration::from_millis(50));
    stream.forget();
    for _ in 0..3 {
        permits.send(()).expect("the reader is still reading");
    }
    thread::sleep(Duration::from_millis(50));
    drop(permits);

    assert_eq!(
        stream.finish(),
        b"xxx",
        "a stream no call is waiting on is read and thrown away, so a browser outliving the call \
         cannot grow its sink"
    );
}
