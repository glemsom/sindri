//! The queue: the prompts typed while a run had the frame, oldest first, and
//! the two keys that take one back or drop it. It is state, not
//! drawing: the panel is `render::queue`'s.

use std::collections::VecDeque;

use crate::app::App;

impl App {
    /// What is waiting, oldest first: the front of the queue is the prompt
    /// that runs next.
    pub fn queue(&self) -> &VecDeque<String> {
        &self.queue
    }

    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// A cancel has been asked for and the run has not stopped yet. The frame
    /// says so the moment the key lands, because an in-flight request is not
    /// interrupted and its answer can be tens of seconds away.
    pub fn is_cancelling(&self) -> bool {
        self.is_running() && self.cancel.is_triggered()
    }

    /// `↑`: the newest queued prompt comes back into the composer, above
    /// whatever draft was typed after it, so taking one back loses nothing.
    pub(crate) fn take_back(&mut self) -> bool {
        let Some(queued) = self.queue.pop_back() else {
            return false;
        };
        let draft = self.composer.take();
        let text = if draft.trim().is_empty() {
            queued
        } else {
            format!("{queued}\n{draft}")
        };
        self.composer.replace(&text);
        true
    }

    /// `Ctrl-X`: the newest queued prompt is dropped. One, never the queue:
    /// the others are still what the user asked for.
    pub(crate) fn drop_queued(&mut self) -> bool {
        self.queue.pop_back().is_some()
    }
}
