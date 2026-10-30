//! The rail: the facts a user must never lose sight of — what a run has cost,
//! how close it is to the context limit, which model and workspace are in
//! play, which sandbox is on, and where the record lives (ADR-0011). One
//! ordered list of facts feeds both the column and the one-line strip, so the
//! two can never disagree.

use crate::driver::{Stop, Tokens};

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
}

impl Rail {
    pub fn new(launch: Launch) -> Self {
        Rail {
            launch,
            turns: 0,
            tokens: Tokens::default(),
            context: 0,
            stop: None,
        }
    }

    pub fn turn(&mut self, turn: u32) {
        self.turns = self.turns.max(turn);
    }

    pub fn reconfigured(&mut self, launch: Launch) {
        self.launch = launch;
    }

    pub fn settled(&mut self, turns: u32, tokens: Tokens, stop: Stop) {
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
        self.context
    }

    /// The rail's rows, in the one order the column and the strip read them:
    /// the numbers the run is spending, then the identity block under the
    /// rule, and the posture as a chip of its own.
    pub fn facts(&self) -> Vec<Fact> {
        let mut facts = vec![
            Fact::spend("cache", self.cache()),
            Fact::spend("turns", format!("{}/{}", self.turns, self.launch.max_turns)),
            Fact::spend(
                "tokens",
                format!(
                    "{} in · {} out",
                    compact(self.tokens.input),
                    compact(self.tokens.output)
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
    }

    /// The rail as one line, for a terminal too narrow to give it a column:
    /// the same facts in the same order, in as much of them as the line can
    /// hold. The posture, the model and the workspace are kept their room, so
    /// a narrower terminal loses the meter before it loses the run.
    pub fn strip(&self, width: usize) -> String {
        let facts = self.facts();
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

/// One row of the rail: a label in a fixed field and its value beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    label: &'static str,
    value: String,
    block: Block,
    fit: Fit,
    keep: bool,
}

/// Which block of the rail a fact belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    /// A number the run is spending, above the rule.
    Spend,
    /// What the run is: the identity block under the rule.
    Identity,
    /// The sandbox posture, which draws as a chip rather than as another
    /// label/value pair.
    Posture,
}

/// How a value longer than its column is shortened. A value is never wrapped
/// onto a second row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Cut at the end.
    End,
    /// An id or a path, whose two ends both say something: the head names it
    /// and the tail says where it ends.
    Middle,
}

impl Fact {
    pub fn label(&self) -> &'static str {
        self.label
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn block(&self) -> Block {
        self.block
    }

    pub fn fit(&self) -> Fit {
        self.fit
    }

    /// Whether the one-line strip may never drop this fact.
    pub fn keeps(&self) -> bool {
        self.keep
    }

    /// Whether clicking this row copies what the fact says rather than the row
    /// it was drawn on: the GUID is pasted into `sindri sessions show`, and an
    /// ellipsised one is no use there.
    pub fn copies_whole(&self) -> bool {
        self.label == "session"
    }

    /// The fact as one strip entry, with the column's label.
    pub fn line(&self) -> String {
        match self.block {
            Block::Posture => format!("⬢ {}", self.value),
            _ => format!("{} {}", self.label, self.value),
        }
    }

    /// The same fact where the strip has no room for the column's label.
    pub fn brief(&self) -> String {
        match self.block {
            Block::Posture => format!("⬢ {}", self.value),
            _ => format!("{} {}", self.short_label(), self.value),
        }
    }

    fn short_label(&self) -> &'static str {
        match self.label {
            "workspace" => "ws",
            other => other,
        }
    }

    fn spend(label: &'static str, value: String) -> Fact {
        Fact {
            label,
            value,
            block: Block::Spend,
            fit: Fit::End,
            keep: false,
        }
    }

    fn identity(label: &'static str, value: String, fit: Fit) -> Fact {
        Fact {
            label,
            value,
            block: Block::Identity,
            fit,
            keep: false,
        }
    }

    fn posture(value: String) -> Fact {
        Fact {
            label: "sandbox",
            value,
            block: Block::Posture,
            fit: Fit::End,
            keep: false,
        }
    }

    fn kept(mut fact: Fact) -> Fact {
        fact.keep = true;
        fact
    }
}

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
