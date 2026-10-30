//! Live output: a port that sees each command's output as it arrives, so a
//! front end can show the tail of a running command without the executor
//! knowing that a front end exists (ADR-0011).

use std::io::{self, BufRead, BufReader, Read};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Stdout,
    Stderr,
}

pub trait OutputSink: Send + Sync {
    fn output(&self, side: Side, text: &str);
}

/// Reads a stream to EOF on its own thread, handing each line to `sink` as it
/// arrives and keeping the whole stream for the result the model reads: the
/// tail a human watches and the bytes the model gets are the same bytes.
pub fn drain<R: Read + Send + 'static>(
    source: R,
    side: Side,
    sink: Option<Arc<dyn OutputSink>>,
) -> std::thread::JoinHandle<io::Result<Vec<u8>>> {
    std::thread::spawn(move || match sink {
        None => {
            let mut buffer = Vec::new();
            let mut source = source;
            source.read_to_end(&mut buffer)?;
            Ok(buffer)
        }
        Some(sink) => {
            let mut reader = BufReader::new(source);
            let mut seen = Vec::new();
            loop {
                let mut line = Vec::new();
                if reader.read_until(b'\n', &mut line)? == 0 {
                    return Ok(seen);
                }
                let text = String::from_utf8_lossy(&line);
                sink.output(side, text.trim_end_matches(['\n', '\r']));
                seen.extend_from_slice(&line);
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{Side, drain};
    use std::io::Cursor;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Recorder(Mutex<Vec<(Side, String)>>);

    impl super::OutputSink for Recorder {
        fn output(&self, side: Side, text: &str) {
            if let Ok(mut seen) = self.0.lock() {
                seen.push((side, text.to_string()));
            }
        }
    }

    #[test]
    fn every_line_reaches_the_sink_and_the_whole_stream_survives() {
        let recorder = Arc::new(Recorder::default());
        let handle = drain(
            Cursor::new(b"one\ntwo\nthree".to_vec()),
            Side::Stdout,
            Some(recorder.clone()),
        );
        let all = handle.join().expect("the reader thread ran").expect("read");
        assert_eq!(all, b"one\ntwo\nthree", "the bytes are kept for the model");
        let seen = recorder.0.lock().expect("lock").clone();
        assert_eq!(
            seen,
            vec![
                (Side::Stdout, "one".to_string()),
                (Side::Stdout, "two".to_string()),
                (Side::Stdout, "three".to_string()),
            ],
            "each line arrives once, in order, without its newline"
        );
    }

    #[test]
    fn without_a_sink_nothing_is_decorated() {
        let handle = drain(Cursor::new(b"only\n".to_vec()), Side::Stderr, None);
        let all = handle.join().expect("the reader thread ran").expect("read");
        assert_eq!(all, b"only\n");
    }
}
