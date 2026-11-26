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
            .earlier(request.earlier().iter().cloned())
            .turns(request.history().iter().cloned())
            .tools(self.tools.tools())
            .prompt(request.prompt())
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
