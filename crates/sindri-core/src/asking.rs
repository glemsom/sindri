use std::sync::{Arc, Mutex};

use crate::provider::{Answer, Ask, Client, Deltas, Failure};
use crate::run::{Reply, Request, Responder};
use crate::tools::ToolSet;
use crate::turn::Usage;

#[derive(Clone, Debug)]
pub struct Asking {
    client: Client,
    ask: Ask,
    tools: ToolSet,
    spent: Arc<Mutex<Usage>>,
    compacted: Arc<Mutex<Option<(Vec<crate::turn::Turn>, Vec<crate::turn::Turn>)>>>,
    event: Arc<Mutex<bool>>,
}

impl Asking {
    pub fn new(client: Client, ask: Ask, tools: ToolSet) -> Asking {
        Asking {
            client,
            ask,
            tools,
            spent: Arc::new(Mutex::new(Usage::default())),
            compacted: Arc::new(Mutex::new(None)),
            event: Arc::new(Mutex::new(false)),
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
            .earlier(request.earlier().iter().cloned())
            .turns(request.history().iter().cloned())
            .tools(self.tools.tools())
            .prompt(request.prompt())
    }
}

impl Responder for Asking {
    fn compacted(&self) -> bool {
        std::mem::take(
            &mut *self
                .event
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
    fn replay_loss(&self, earlier: &[crate::turn::Turn]) -> bool {
        self.ask.transition(earlier).1
    }
    fn respond(&self, request: &Request, deltas: &mut dyn Deltas) -> Result<Reply, Failure> {
        let mut earlier = request.earlier().to_vec();
        let mut cache = self
            .compacted
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((original, replacement)) = cache.as_ref() {
            if earlier.starts_with(original) {
                earlier = replacement
                    .iter()
                    .cloned()
                    .chain(earlier[original.len()..].iter().cloned())
                    .collect();
            } else {
                *cache = None;
            }
        }
        let asked = self.ask(request).earlier(earlier);
        let compacted = asked.compact(&self.client)?;
        let answer = self
            .client
            .ask(compacted.as_ref().unwrap_or(&asked), deltas)?;
        if let Some(compacted) = compacted {
            *cache = Some((request.earlier().to_vec(), compacted.retained().to_vec()));
            *self
                .event
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        }
        self.spent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .charge(&answer.usage);
        Ok(Reply::from(answer))
    }
}

impl From<Answer> for Reply {
    fn from(answer: Answer) -> Reply {
        Reply {
            assistant: answer.assistant,
            text: answer.text,
            reasoning: answer.reasoning,
            responses: answer.responses,
            calls: answer.calls,
            usage: answer.usage,
        }
    }
}
