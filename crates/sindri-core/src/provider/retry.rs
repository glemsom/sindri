//! Three attempts, waiting 200 ms and then twice that, and only ever for a failure the wire
//! itself called retryable — never one that already streamed text, and never a cancel.

use std::time::Duration;

use super::{Failure, Verdict};

pub const ATTEMPTS: usize = 3;

pub const BACKOFF: Duration = Duration::from_millis(200);

pub trait Backoff {
    fn wait(&self, delay: Duration);
}

impl<F: Fn(Duration)> Backoff for F {
    fn wait(&self, delay: Duration) {
        self(delay);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Sleep;

impl Backoff for Sleep {
    fn wait(&self, delay: Duration) {
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }
}

fn delay_before(number: usize) -> Duration {
    if number <= 1 {
        return Duration::ZERO;
    }
    let doublings = u32::try_from(number - 2).unwrap_or(u32::MAX);
    BACKOFF * 2u32.saturating_pow(doublings)
}

fn allowed(failure: &Failure) -> bool {
    failure.retryable && !failure.streamed && failure.verdict != Verdict::Cancelled
}

pub fn until_final<T>(
    backoff: &dyn Backoff,
    mut attempt: impl FnMut() -> Result<T, Failure>,
) -> Result<T, Failure> {
    let mut number = 1;
    loop {
        backoff.wait(delay_before(number));
        match attempt() {
            Ok(answer) => return Ok(answer),
            Err(failure) if number < ATTEMPTS && allowed(&failure) => number += 1,
            Err(failure) => return Err(failure),
        }
    }
}
