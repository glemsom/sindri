//! The one adapter between the run loop and the provider seam: the tools a run holds out, the
//! calls a reply asks for back, and each turn replayed beside what its calls answered.

use std::sync::{Arc, Mutex};

use crate::provider::{Ask, Call, Client, Deltas, Earlier, Failure, Turn};
use crate::records::Usage;
use crate::run::{Exchange, Reply, Request, Responder};
use crate::tools::ToolSet;

#[derive(Clone, Debug)]
pub struct Asking {
    client: Client,
    ask: Ask,
    tools: ToolSet,
    spent: Arc<Mutex<Usage>>,
}

impl Asking {
    pub fn new(client: Client, ask: Ask, tools: ToolSet) -> Asking {
        Asking {
            client,
            ask,
            tools,
            spent: Arc::new(Mutex::new(Usage::default())),
        }
    }

    pub fn spent(&self) -> Usage {
        *self
            .spent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn ask(&self, request: &Request) -> Ask {
        self.ask
            .clone()
            .earlier(request.earlier().iter().map(|ended| Earlier {
                prompt: ended.prompt.clone(),
                answer: ended.text.clone(),
            }))
            .turns(request.history().iter().map(turn_of))
            .tools(self.tools.tools())
            .prompt(request.prompt())
    }
}

/// A turn of this run as the wire is asked it: call and answer paired, so a result never arrives
/// against a call the model did not make.
fn turn_of(ended: &Exchange) -> Turn {
    Turn {
        text: ended.text.clone(),
        calls: ended
            .calls
            .iter()
            .zip(&ended.results)
            .map(|(call, acted)| {
                (
                    Call {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    },
                    acted.text.clone(),
                )
            })
            .collect(),
    }
}

impl Responder for Asking {
    fn respond(&self, request: &Request, deltas: &mut dyn Deltas) -> Result<Reply, Failure> {
        let asked = self.ask(request);
        let answer = self.client.ask(&asked, deltas)?;
        self.spent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .charge(&answer.usage);
        let calls = answer
            .calls
            .into_iter()
            .map(|call| crate::run::Call::new(call.id, call.name, call.arguments));
        Ok(Reply::asking(answer.text, calls).spent(answer.usage))
    }
}
