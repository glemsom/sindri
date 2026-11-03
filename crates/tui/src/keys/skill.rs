//! `/skill`: a name loaded by hand, or — with no name — a question about the
//! catalog. A bare name arms the skill and starts no turn; a tail becomes the
//! prompt the run answers once the load has landed. It lives beside the other
//! keys rather than in them because the conversation it writes into is the front
//! end's, and the frame's whole part in it is asking at the one moment the
//! history is not already in flight.

use crate::app::{App, Loading, Renew};
use crate::command::SkillAsk;

impl App {
    pub(super) fn skill_command(&mut self, ask: Option<SkillAsk>) {
        let Some(ask) = ask else {
            return self.skill_question();
        };
        // The conversation is not the frame's to write in while a run has it, a
        // new session is being proved, or a load is still unanswered: a body
        // injected into a history that is already sent is a body that turn did
        // not carry, and two loads in flight would leave the first answer
        // releasing the second one's prompt.
        if let Some(why) = self.busy() {
            self.note_error(format!(
                "cannot load {} while {why} — ask again when it is done",
                ask.name
            ));
            return;
        }
        if !self.skill_known(&ask.name) {
            let known = self.invocable().join(", ");
            self.note_error(match known.is_empty() {
                true => format!(
                    "there is no skill called {}: this workspace holds none you may invoke",
                    ask.name
                ),
                false => format!("there is no skill called {}: {}", ask.name, known),
            });
            return;
        }
        self.loading = Loading::Asked(ask.name);
        self.armed = ask.prompt;
        self.completion = None;
        self.walking = None;
    }

    /// Whether the workspace holds a skill a human may invoke under this name.
    pub(super) fn skill_known(&self, name: &str) -> bool {
        self.invocable().iter().any(|held| held == name)
    }

    /// The skills a human may reach for, as the boot handed them over: the one
    /// list `/skill` is checked against and offered from.
    pub(super) fn invocable(&self) -> &[String] {
        &self.rail.launch().skills
    }

    /// Why the conversation cannot take a skill right now, in the frame's words.
    pub(super) fn busy(&self) -> Option<&'static str> {
        if self.running.is_some() {
            return Some("a run is in flight");
        }
        if self.is_loading() {
            return Some("another skill is still loading");
        }
        (self.renew != Renew::Idle).then_some("a new session is being started")
    }

    /// `/skill` with no name: which skills this workspace holds, and how one is
    /// typed. A frame that answered nothing would look like a load that did not
    /// happen.
    pub(super) fn skill_question(&mut self) {
        let names = self.invocable().to_vec();
        match names.is_empty() {
            true => self.note_info("this workspace holds no skill you may invoke"),
            false => self.note_info(format!(
                "skills: {} — `/skill:name` loads one, `/skill:name <prompt>` runs it",
                names.join(", ")
            )),
        }
    }
}
