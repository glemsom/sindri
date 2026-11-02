//! A turn's calls, run: one wave of up to [`PARALLEL_CALLS`] scripts at a time.
//! A model that emitted several calls in one turn declared them independent of
//! each other, so they may overlap — and their results come back in the order
//! it asked for them, whatever order they finished in. The executor is shared
//! and the sandbox names each spill file after its own counter, so two waves
//! cannot collide; two scripts that write one file are a race whoever wins.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::{Loaded, Tools};
use crate::provider::{Call, ToolOutcome};

/// The most scripts one turn runs at once. Small on purpose: it keeps a budget
/// stop within a few commands of the line, and it keeps a machine with one core
/// from spending the whole turn swapping.
pub const PARALLEL_CALLS: usize = 4;

/// One wave, each call run once, results in submission order. `None` is a call
/// a cancel killed mid-flight, which the loop reads as the run ending. The
/// loaded set is shared by the whole wave: two identical calls in one turn are
/// one turn, and the turn leaves one body behind.
pub fn wave(tools: &Tools, loaded: &Loaded, calls: &[Call]) -> Vec<Option<ToolOutcome>> {
    match calls {
        [] => Vec::new(),
        [one] => vec![tools.call(one, loaded)],
        several => spread(tools, loaded, several),
    }
}

fn spread(tools: &Tools, loaded: &Loaded, calls: &[Call]) -> Vec<Option<ToolOutcome>> {
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..calls.len().min(PARALLEL_CALLS))
            .map(|_| {
                scope.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(call) = calls.get(index) else { break };
                        mine.push((index, tools.call(call, loaded)));
                    }
                    mine
                })
            })
            .collect();
        let mut done = Vec::with_capacity(calls.len());
        for worker in workers {
            match worker.join() {
                Ok(mine) => done.extend(mine),
                // A worker's panic is the executor's, and belongs to the thread
                // that asked for the run: raised here, it fails as it always did.
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        done.sort_by_key(|(index, _)| *index);
        done.into_iter().map(|(_, outcome)| outcome).collect()
    })
}
