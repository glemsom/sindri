//! What the loop hands the driver, and the thread that answers it: one queue, so
//! nothing overtakes anything else — a load reaches the conversation before the
//! prompt that was written behind it, because the thread that runs them is one
//! thread and the queue is in order.

use std::sync::mpsc::{self, Sender};

use crate::app::App;
use crate::driver::{Driver, Tap, Task};
use crate::msg::Msg;

pub(super) fn begin(app: &mut App, jobs: &Sender<Task>) {
    if app.take_renew() && jobs.send(Task::Renew).is_err() {
        return;
    }
    if app.take_reload() && jobs.send(Task::Reload).is_err() {
        return;
    }
    // The names the wire serves, asked beside the modal that states them: the
    // frame draws and takes every key while the thread is out with it.
    if let Some(serving) = app.take_ask()
        && jobs.send(Task::Ask(serving)).is_err()
    {
        return;
    }
    // The load goes first, and the prompt that waits on it is not started until
    // the front end has answered: a turn with a skill the reader asked for left
    // out is a turn they did not ask for.
    if let Some(skill) = app.take_load()
        && jobs.send(Task::Load(skill)).is_err()
    {
        return;
    }
    while let Some(job) = app.start() {
        if jobs.send(Task::Run(job)).is_err() {
            break;
        }
    }
}

pub(super) fn drive(mut driver: Box<dyn Driver>, tx: Sender<Msg>) -> Sender<Task> {
    let (tasks, queue) = mpsc::channel::<Task>();
    std::thread::spawn(move || {
        for task in queue {
            let sent = match task {
                Task::Run(job) => {
                    let finished = driver.run(&job.prompt, job.cancel, Tap::new(tx.clone()));
                    tx.send(Msg::Finished(finished))
                }
                Task::Load(skill) => {
                    let loaded = driver.load(&skill, Tap::new(tx.clone()));
                    tx.send(Msg::Loaded(loaded))
                }
                Task::Ask(serving) => {
                    // The ask is made of the `Serving` it states, and the
                    // answer travels back labelled with the same pair: a
                    // listing that lands after the row has moved is dropped
                    // where it lands.
                    let names = driver.models(&serving);
                    tx.send(Msg::Listed(serving, names))
                }
                Task::Renew => tx.send(Msg::Renewed(driver.renew().map(Box::new))),
                Task::Reload => tx.send(Msg::Reloaded(driver.reload().map(Box::new))),
            };
            if sent.is_err() {
                return;
            }
        }
    });
    tasks
}
