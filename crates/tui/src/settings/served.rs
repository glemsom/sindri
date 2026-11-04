//! The names the wire serves, and what they answer for. One ask is one
//! `Serving` — the wire a run speaks and the base URL it addresses, together,
//! because a list for one of them says nothing about the other — so a listing
//! whose `Serving` the frame is no longer holding is dropped where it lands
//! rather than drawn over a row that has moved on. The names are kept rather
//! than counted: the model row states the count today, and the completion and
//! the list that open on the same listing read the names themselves.

/// One wire at one address: what a listing answers for, and what an ask is
/// asked of. The two travel as one value, so a listing can be compared against
/// the ask that is out with one `==` and nothing else to keep in step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Serving {
    wire: String,
    base_url: Option<String>,
}

impl Serving {
    /// The ask a run's own facts make: the wire its launch names and the base
    /// URL it addresses, so the names a frame shows are the ones that run would
    /// be served.
    pub fn of(wire: impl Into<String>, base_url: Option<String>) -> Serving {
        Serving {
            wire: wire.into(),
            base_url,
        }
    }

    /// The wire that would name them: what the row says beside the count.
    pub fn wire(&self) -> &str {
        &self.wire
    }

    /// The address they would answer for, where the wire is spoken at one of
    /// the run's own choosing.
    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }
}

/// What one ask came to, which the row states and which no caller outside this
/// module needs: `names` and `note` are what is read of it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Listing {
    /// The names the wire said it serves, in the order it named them.
    Named(Vec<String>),
    /// The wire could not be asked, or named nothing it could be asked. A hint
    /// and never a refusal: the modal this row sits in is the front door for a
    /// run with no key, which is exactly the run that cannot be asked.
    Absent,
}

/// The names a wire serves, what they answer for, and whether the ask is still
/// out: the whole of what the model row states about the wire, read here so the
/// count, the completion and the list under the row cannot disagree.
#[derive(Debug, Clone, Default)]
pub struct Served {
    /// What the listing on hand answers for, or the ask that is still out.
    serving: Option<Serving>,
    /// What that ask came to: `None` before one was made and while it is out.
    listing: Option<Listing>,
    /// An ask the frame has made and no thread has taken yet.
    wanted: Option<Serving>,
}

impl Served {
    /// A new ask, which drops whatever was on hand: the listing for the last
    /// one is a listing for a wire this frame is no longer stating.
    pub fn ask(&mut self, serving: Serving) {
        self.listing = None;
        self.wanted = Some(serving.clone());
        self.serving = Some(serving);
    }

    /// An ask the frame is not already stating, and whether one was made: a
    /// `Serving` this frame holds is not asked for again, because asking again
    /// is a keystroke of its own — a wire chosen, a modal reopened on another.
    pub fn want(&mut self, serving: Serving) -> bool {
        if self.serving.as_ref() == Some(&serving) {
            return false;
        }
        self.ask(serving);
        true
    }

    /// The ask a thread is to take, once: the frame asks, the loop carries it.
    pub fn take(&mut self) -> Option<Serving> {
        self.wanted.take()
    }

    /// What the listing on hand — or the ask that is out — answers for.
    pub fn serving(&self) -> Option<&Serving> {
        self.serving.as_ref()
    }

    /// Whether an ask is out: the row draws and the modal takes every key
    /// either way, because nothing here is waited on.
    pub fn asking(&self) -> bool {
        self.serving.is_some() && self.listing.is_none()
    }

    /// The names offered, in the order the wire named them, and none where no
    /// listing landed.
    pub fn names(&self) -> &[String] {
        match &self.listing {
            Some(Listing::Named(names)) => names,
            Some(Listing::Absent) | None => &[],
        }
    }

    /// A listing landing, and whether the frame was still asking it: one for a
    /// wire or base URL that has changed since is dropped here, and the row it
    /// would have said something on is left exactly as it was.
    pub fn landed(&mut self, serving: Serving, names: Option<Vec<String>>) -> bool {
        if self.serving.as_ref() != Some(&serving) {
            return false;
        }
        self.wanted = None;
        self.listing = Some(match names {
            Some(names) => Listing::Named(names),
            None => Listing::Absent,
        });
        true
    }

    /// What the model row says beside the name it holds: how many the wire
    /// offered and which wire named them, or — where there is no list to state
    /// — that there is none. `None` while the ask is out, because a count
    /// nothing has answered for is a count the frame would be inventing.
    pub fn note(&self) -> Option<String> {
        let serving = self.serving.as_ref()?;
        Some(match self.listing.as_ref()? {
            Listing::Named(names) => format!("{} · {}", counted(names.len()), serving.wire()),
            Listing::Absent => "no list · type a name".to_string(),
        })
    }
}

/// A count and the word for what it counts: one name is a name.
fn counted(names: usize) -> String {
    if names == 1 {
        "1 name".to_string()
    } else {
        format!("{names} names")
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        reason = "a panicking test is how a broken contract reports itself"
    )]

    use super::*;

    fn one() -> Serving {
        Serving::of("anthropic", None)
    }

    #[test]
    fn a_listing_for_a_wire_that_has_changed_since_is_dropped_where_it_lands() {
        let mut served = Served::default();
        served.ask(one());
        let elsewhere = Serving::of("openai", Some("http://localhost:8080".to_string()));
        served.ask(elsewhere.clone());

        assert!(
            !served.landed(one(), Some(vec!["claude-sonnet-4-6".to_string()])),
            "the ask on hand is another wire's, so this listing is nobody's"
        );
        assert!(served.names().is_empty(), "and no name reached the frame");
        assert!(served.asking(), "the later ask is still out");

        assert!(served.landed(elsewhere, Some(vec!["gpt-5".to_string()])));
        assert_eq!(served.names(), ["gpt-5".to_string()]);
        assert!(
            !served.asking(),
            "the listing that landed is the ask on hand's"
        );
    }

    #[test]
    fn the_wire_the_frame_is_already_stating_is_not_asked_for_again() {
        let mut served = Served::default();
        assert!(served.want(one()), "no wire is being stated yet");

        served.landed(one(), Some(vec!["claude-sonnet-4-6".to_string()]));
        assert!(
            !served.want(one()),
            "the names on hand answer for this wire already"
        );
        assert_eq!(
            served.names(),
            ["claude-sonnet-4-6".to_string()],
            "so the listing is left standing rather than dropped"
        );

        let elsewhere = Serving::of("openai", None);
        assert!(served.want(elsewhere), "another wire is another ask");
        assert!(served.asking(), "and it is out");
        assert!(served.names().is_empty(), "with the old names gone");
    }

    #[test]
    fn a_base_url_is_as_much_what_the_names_answer_for_as_the_wire_is() {
        let mut served = Served::default();
        let at_one = Serving::of("openai", Some("http://localhost:8080".to_string()));
        served.ask(at_one.clone());

        assert!(
            !served.landed(
                Serving::of("openai", Some("http://localhost:9090".to_string())),
                Some(vec!["gpt-5".to_string()])
            ),
            "the same wire at another address names whatever that server names"
        );
        assert!(served.asking());
        assert!(served.landed(at_one, None));
        assert!(served.names().is_empty(), "a wire with no list offers none");
    }

    #[test]
    fn the_note_counts_the_names_and_names_the_wire_that_offered_them() {
        let mut served = Served::default();
        served.ask(one());
        assert_eq!(served.note(), None, "an ask in flight states no count");

        assert!(served.landed(one(), Some(vec!["one".to_string()])));
        assert_eq!(served.note(), Some("1 name · anthropic".to_string()));

        served.ask(one());
        assert!(served.landed(one(), Some(vec!["one".to_string(), "two".to_string()])));
        assert_eq!(served.note(), Some("2 names · anthropic".to_string()));

        served.ask(one());
        assert!(served.landed(one(), None));
        assert_eq!(served.note(), Some("no list · type a name".to_string()));
    }
}
