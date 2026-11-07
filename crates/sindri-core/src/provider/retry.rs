//! §4's retry policy in one place: an ask is tried three times, waiting 200 ms
//! and then twice that, and only ever for a failure the wire itself called
//! retryable — never for a status code, a reply that already streamed text, or
//! a cancel.
//!
//! The wait is a seam, not a call to `sleep`: [`Backoff`] is handed the delay
//! before every attempt, so a test asserts the sequence `0, 200, 400` instead
//! of spending it, and [`Sleep`] is the only implementation that really waits.
//! Nothing here knows what a wire is, so nothing here can consult a status.

use std::time::Duration;

use super::{Failure, Verdict};

/// How many times one ask is tried, the first included.
pub const ATTEMPTS: usize = 3;

/// How long the second attempt waits; every later one doubles it.
pub const BACKOFF: Duration = Duration::from_millis(200);

/// The waiting half of the policy, injected so a test proves the delays without
/// sleeping through them.
pub trait Backoff {
    /// Wait `delay` before the next attempt. Zero means the first attempt,
    /// which waits for nothing.
    fn wait(&self, delay: Duration);
}

impl<F: Fn(Duration)> Backoff for F {
    fn wait(&self, delay: Duration) {
        self(delay);
    }
}

/// The backoff a real ask takes: the calling thread sleeps, which is what an
/// ask that blocks until the reply ends already does.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sleep;

impl Backoff for Sleep {
    fn wait(&self, delay: Duration) {
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }
}

/// How long to wait before attempt `number`, counted from one: nothing before
/// the first, then [`BACKOFF`] doubling.
fn delay_before(number: usize) -> Duration {
    if number <= 1 {
        return Duration::ZERO;
    }
    let doublings = u32::try_from(number - 2).unwrap_or(u32::MAX);
    BACKOFF * 2u32.saturating_pow(doublings)
}

/// Whether §4 allows another attempt: the wire's own retry verdict, with
/// nothing already streamed and no cancel, however retryable the wire calls it.
fn allowed(failure: &Failure) -> bool {
    failure.retryable && !failure.streamed && failure.verdict != Verdict::Cancelled
}

/// `attempt` until it answers, its failure forbids another attempt, or the
/// attempts run out — handing back the last failure either way.
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
