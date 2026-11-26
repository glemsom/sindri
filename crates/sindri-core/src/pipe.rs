use std::io::{ErrorKind, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How long a reader gets to see the end of its pipe once the program has ended: a grandchild left
/// holding it cannot hold the call open.
const GRACE: Duration = Duration::from_millis(250);

pub struct Stream {
    sink: Arc<Mutex<Vec<u8>>>,
    keeping: Arc<AtomicBool>,
    reader: JoinHandle<()>,
}

impl Stream {
    pub fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> Stream {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let collected = Arc::clone(&sink);
        let keeping = Arc::new(AtomicBool::new(true));
        let held = Arc::clone(&keeping);
        let reader = thread::spawn(move || {
            let Some(mut pipe) = pipe else {
                return;
            };
            let mut chunk = [0u8; 8 * 1024];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => return,
                    Ok(read) => {
                        if held.load(Ordering::Relaxed) {
                            collected
                                .lock()
                                .expect("the sink is not poisoned")
                                .extend_from_slice(&chunk[..read]);
                        }
                    }
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => return,
                }
            }
        });
        Stream {
            sink,
            keeping,
            reader,
        }
    }

    /// Stops keeping what is still to come: the stream goes on being read, so the program writing
    /// it never blocks on a full pipe, and a program that outlives its call cannot grow the sink.
    pub fn forget(&self) {
        self.keeping.store(false, Ordering::Relaxed);
    }

    pub fn finished(&self) -> bool {
        self.reader.is_finished()
    }

    pub fn collected(self) -> Vec<u8> {
        self.forget();
        self.sink.lock().expect("the sink is not poisoned").clone()
    }

    pub fn finish(self) -> Vec<u8> {
        let deadline = Instant::now() + GRACE;
        while !self.reader.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        self.sink.lock().expect("the sink is not poisoned").clone()
    }
}

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
