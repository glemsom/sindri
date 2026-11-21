//! A child's stream read by a thread of its own, so a program that fills it cannot block on a pipe
//! nobody is reading, collected into a sink the call can take whether or not the read ended.

use std::io::{ErrorKind, Read};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How long a reader gets to see the end of its pipe once the program has ended: a grandchild left
/// holding it cannot hold the call open.
pub const GRACE: Duration = Duration::from_millis(250);

/// One stream of a child, drained as it is written.
pub struct Stream {
    sink: Arc<Mutex<Vec<u8>>>,
    reader: JoinHandle<()>,
}

impl Stream {
    pub fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> Stream {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let collected = Arc::clone(&sink);
        let reader = thread::spawn(move || {
            let Some(mut pipe) = pipe else {
                return;
            };
            let mut chunk = [0u8; 8 * 1024];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => return,
                    Ok(read) => collected
                        .lock()
                        .expect("the sink is not poisoned")
                        .extend_from_slice(&chunk[..read]),
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => return,
                }
            }
        });
        Stream { sink, reader }
    }

    /// Everything the stream said, once the grace for the last of it has run out.
    pub fn finish(self) -> Vec<u8> {
        let deadline = Instant::now() + GRACE;
        while !self.reader.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        self.sink.lock().expect("the sink is not poisoned").clone()
    }
}

/// The first whole line of what a stream said, trimmed, and none of the rest: a line something
/// else is still writing past the last newline says nothing.
pub fn first_line(said: &[u8]) -> String {
    let text = String::from_utf8_lossy(said);
    let whole = match text.rfind('\n') {
        Some(end) => &text[..=end],
        None => &text,
    };
    whole
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests;
