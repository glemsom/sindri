//! The rail: the facts a user must never lose sight of — what a run has cost,
//! how close it is to the context limit, which model and workspace are in
//! play, which sandbox is on, and where the record lives (ADR-0011). One
//! ordered list of facts feeds both the column and the one-line strip, so the
//! two can never disagree.

mod fact;

use std::time::Duration;

use crate::driver::{Stop, Tokens};
use crate::pulse;

pub use self::fact::{Block, Fact, Fit};

#[derive(Debug, Clone)]
pub struct Launch {
    pub session: String,
    pub temp: String,
    pub branch: Option<String>,
    pub sandbox: String,
    /// The model this run asks: the one fact that says what the user is
    /// talking to.
    pub model: String,
    /// The directory the frame was launched in — and the one the sandbox
    /// mounts — shortened to `~` under the home: the header of every turn
    /// names it, and the rail states it.
    pub workspace: String,
    pub max_turns: u32,
    pub context_limit: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Rail {
    launch: Launch,
    turns: u32,
    tokens: Tokens,
    /// The size of the conversation as the provider last counted it: the
    /// whole history goes in every turn, so this is the last input
    /// count, not a sum.
    context: u64,
    stop: Option<Stop>,
    /// The characters that have streamed since this run began: the only
    /// usage the frame is handed while the run it describes is still running,
    /// turned into tokens at the one rate it can honestly guess and trued up
    /// by the report (ADR-0011 §4).
    streamed: u64,
    /// When each spend figure last grew, so the rail can accent what just
    /// moved for a moment.
    fresh: Vec<(&'static str, Duration)>,
}

/// The characters one token is worth, as the frame counts them: a guess, and
/// only ever a guess until the provider's own report lands.
const CHARS_PER_TOKEN: u64 = 4;

impl Rail {
    pub fn new(launch: Launch) -> Self {
        Rail {
            launch,
            turns: 0,
            tokens: Tokens::default(),
            context: 0,
            stop: None,
            streamed: 0,
            fresh: Vec::new(),
        }
    }

    pub fn turn(&mut self, turn: u32, now: Duration) {
        if turn > self.turns {
            self.moved("turns", now);
        }
        self.turns = self.turns.max(turn);
    }

    /// A run has begun: what streams from here is the live figure, and the
    /// last report's is the confirmed one. The accents go with it — a figure
    /// that was fresh for the last run is not fresh for this one.
    pub fn begun(&mut self) {
        self.streamed = 0;
        self.fresh.clear();
    }

    /// `chars` more characters of the answer have arrived: the output figure
    /// and the context meter both move on it, now rather than at the end of
    /// the run they describe.
    pub fn streamed(&mut self, chars: usize, now: Duration) {
        let chars = chars as u64;
        if chars == 0 {
            return;
        }
        self.streamed = self.streamed.saturating_add(chars);
        self.moved("tokens", now);
        self.moved("context", now);
    }

    pub fn reconfigured(&mut self, launch: Launch) {
        self.launch = launch;
    }

    pub fn settled(&mut self, turns: u32, tokens: Tokens, stop: Stop, now: Duration) {
        // The report replaces the estimate: what streamed was only ever a
        // stand-in for what the provider says it spent.
        self.streamed = 0;
        self.moved("tokens", now);
        self.moved("context", now);
        self.turns = self.turns.max(turns);
        self.tokens = Tokens {
            input: self.tokens.input.saturating_add(tokens.input),
            output: self.tokens.output.saturating_add(tokens.output),
            cached: self.tokens.cached.saturating_add(tokens.cached),
        };
        self.context = tokens.input;
        self.stop = Some(stop);
    }

    pub fn session(&self) -> String {
        self.launch.session.clone()
    }

    pub fn workspace(&self) -> String {
        self.launch.workspace.clone()
    }

    pub fn context_used(&self) -> u64 {
        self.context + self.estimate()
    }

    /// The live guess at what has been spent since the run began: nothing at
    /// all once the run has settled and the estimate is put away.
    fn estimate(&self) -> u64 {
        self.streamed / CHARS_PER_TOKEN
    }

    /// Records that a figure just grew, and when.
    fn moved(&mut self, label: &'static str, now: Duration) {
        self.fresh.retain(|(known, _)| *known != label);
        self.fresh.push((label, now));
    }

    /// Whether a figure grew recently enough for the frame to point at it.
    fn just_moved(&self, label: &str, now: Duration) -> bool {
        self.fresh
            .iter()
            .any(|(known, at)| *known == label && now.saturating_sub(*at) < pulse::FRESH)
    }

    /// The rail's rows, in the one order the column and the strip read them:
    /// the numbers the run is spending, then the identity block under the
    /// rule, and the posture as a chip of its own.
    pub fn facts(&self, now: Duration) -> Vec<Fact> {
        let mut facts = vec![
            Fact::spend("cache", self.cache()),
            Fact::spend("turns", format!("{}/{}", self.turns, self.launch.max_turns)),
            Fact::spend(
                "tokens",
                format!(
                    "{} in · {} out",
                    compact(self.tokens.input),
                    compact(self.tokens.output + self.estimate())
                ),
            ),
            Fact::spend("context", self.meter()),
        ];
        if let Some(stop) = self.stop {
            facts.push(Fact::spend("stop", stop.as_str().to_string()));
        }
        // What the run talks to and what it can touch are never the facts a
        // reader has to go looking for, so the strip keeps their room back.
        facts.push(Fact::kept(Fact::identity(
            "model",
            self.launch.model.clone(),
            Fit::End,
        )));
        facts.push(Fact::kept(Fact::identity(
            "workspace",
            self.launch.workspace.clone(),
            Fit::Middle,
        )));
        facts.push(Fact::identity(
            "branch",
            self.launch
                .branch
                .clone()
                .unwrap_or_else(|| "—".to_string()),
            Fit::End,
        ));
        facts.push(Fact::identity(
            "session",
            self.launch.session.clone(),
            Fit::Middle,
        ));
        facts.push(Fact::identity(
            "temp",
            self.launch.temp.clone(),
            Fit::Middle,
        ));
        facts.push(Fact::kept(Fact::posture(self.launch.sandbox.clone())));
        facts
            .into_iter()
            .map(|fact| {
                let fresh = self.just_moved(fact.label(), now);
                Fact::moved(fact, fresh)
            })
            .collect()
    }

    /// The rail as one line, for a terminal too narrow to give it a column:
    /// the same facts in the same order, in as much of them as the line can
    /// hold. The posture, the model and the workspace are kept their room, so
    /// a narrower terminal loses the meter before it loses the run.
    pub fn strip(&self, width: usize, now: Duration) -> String {
        let facts = self.facts(now);
        let kept: Vec<&Fact> = facts.iter().filter(|fact| fact.keeps()).collect();
        let reserved: usize = kept
            .iter()
            .map(|fact| fact.brief().chars().count() + SEPARATOR.chars().count())
            .sum();
        if reserved > width {
            // Even the facts the line may not lose do not fit: the posture
            // leads, because trust is the one fact never dropped.
            let mut line = String::new();
            let mut kept = kept;
            kept.sort_by_key(|fact| fact.block() != Block::Posture);
            for fact in kept {
                let brief = fact.brief();
                let room = width.saturating_sub(line.chars().count() + SEPARATOR.chars().count());
                if brief.chars().count() <= room {
                    if !line.is_empty() {
                        line.push_str(SEPARATOR);
                    }
                    line.push_str(&brief);
                }
            }
            return line.chars().take(width).collect();
        }

        let mut shown: Vec<String> = Vec::new();
        let mut used = 0usize;
        for (index, fact) in facts.iter().enumerate() {
            let rest: usize = facts[index + 1..]
                .iter()
                .filter(|fact| fact.keeps())
                .map(|fact| fact.brief().chars().count() + SEPARATOR.chars().count())
                .sum();
            let separator = if shown.is_empty() {
                0
            } else {
                SEPARATOR.chars().count()
            };
            let room = width.saturating_sub(used + separator + rest);
            // Shortened before skipped: the column's label where it fits, the
            // brief one where it does not.
            let Some(line) = [fact.line(), fact.brief()]
                .into_iter()
                .find(|line| line.chars().count() <= room)
            else {
                continue;
            };
            used += separator + line.chars().count();
            shown.push(line);
        }
        shown.join(SEPARATOR).chars().take(width).collect()
    }

    fn cache(&self) -> String {
        if self.tokens.input == 0 {
            return "—".to_string();
        }
        let share = self.tokens.cached.saturating_mul(100) / self.tokens.input;
        format!("{share}%")
    }

    fn meter(&self) -> String {
        let used = self.context_used();
        let cells = 8usize;
        match self.launch.context_limit.filter(|limit| *limit > 0) {
            Some(limit) => {
                let filled = ((used.saturating_mul(cells as u64) / limit) as usize).min(cells);
                format!(
                    "{}{} {}/{}",
                    "█".repeat(filled),
                    "░".repeat(cells - filled),
                    compact(used),
                    compact(limit)
                )
            }
            None => format!("{} used", compact(used)),
        }
    }
}

/// What separates one fact from the next on the one-line strip.
const SEPARATOR: &str = " · ";

pub fn compact(count: u64) -> String {
    if count < 1_000 {
        return count.to_string();
    }
    if count < 1_000_000 {
        let tenths = count / 100;
        return match tenths % 10 {
            0 => format!("{}k", tenths / 10),
            remainder => format!("{}.{remainder}k", tenths / 10),
        };
    }
    let tenths = count / 100_000;
    match tenths % 10 {
        0 => format!("{}M", tenths / 10),
        remainder => format!("{}.{remainder}M", tenths / 10),
    }
}

#[cfg(test)]
mod tests {
    use super::compact;

    #[test]
    fn a_count_is_exact_until_it_is_too_long_to_read() {
        assert_eq!(compact(0), "0");
        assert_eq!(compact(999), "999");
        assert_eq!(compact(12_400), "12.4k");
        assert_eq!(compact(2_100_000), "2.1M");
    }
}
