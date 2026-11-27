use sindri_core::recall::Recall;

/// The draft a walk hands back: its words, and the place the caret stands among them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub text: String,
    pub caret: usize,
}

impl Draft {
    pub fn of(text: &str, caret: usize) -> Draft {
        Draft {
            text: text.to_owned(),
            caret,
        }
    }
}

/// The recall a reader is walking, and the draft that walk interrupted. The position is counted
/// from the newest prompt, so an edit to a recalled prompt changes neither it nor the held draft,
/// and the next `Up` takes the prompt before the one on screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Walk {
    recall: Recall,
    at: Option<usize>,
    interrupted: Option<Draft>,
}

impl Walk {
    pub fn of(recall: Recall) -> Walk {
        Walk {
            recall,
            at: None,
            interrupted: None,
        }
    }

    pub fn walking(&self) -> bool {
        self.at.is_some()
    }

    /// The prompt before the one on screen, holding `draft` the first time it is asked for.
    pub fn back(&mut self, draft: Draft) -> Option<Draft> {
        let at = self.at.map_or(0, |at| at + 1);
        let prompt = self.recall.older(at)?;
        if self.at.is_none() {
            self.interrupted = Some(draft);
        }
        self.at = Some(at);
        Some(Draft::of(prompt, prompt.len()))
    }

    /// The prompt after the one on screen, and past the newest the draft the walk interrupted.
    pub fn forward(&mut self) -> Option<Draft> {
        let at = self.at?;
        match at {
            0 => {
                let draft = self.interrupted.take()?;
                self.at = None;
                Some(draft)
            }
            _ => {
                let prompt = self.recall.older(at - 1)?;
                self.at = Some(at - 1);
                Some(Draft::of(prompt, prompt.len()))
            }
        }
    }

    /// Keeps a prompt as it is sent, and ends the walk with it.
    pub fn sent(&mut self, prompt: &str) {
        self.recall.send(prompt);
        self.end();
    }

    pub fn end(&mut self) {
        self.at = None;
        self.interrupted = None;
    }
}
