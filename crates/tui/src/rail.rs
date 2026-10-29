//! The rail: the facts a user must never lose sight of — what a run has cost,
//! how close it is to the context limit, which sandbox is on, and where the
//! record lives (ADR-0011 §4). At 100 columns it is a column; below that it is
//! one strip, never nothing.

use crate::driver::{Stop, Tokens};

/// What a launch knows before the first frame is drawn.
#[derive(Debug, Clone)]
pub struct Launch {
    /// The session GUID this launch is recorded under.
    pub session: String,
    /// The session temp path, as the sandbox mounts it.
    pub temp: String,
    /// The git branch of the workspace, when it is a repository.
    pub branch: Option<String>,
    /// The sandbox posture, in one line.
    pub sandbox: String,
    /// The turn cap.
    pub max_turns: u32,
    /// The model's context window, where it is known.
    pub context_limit: Option<u64>,
}

/// The rail's own state: what the launch said, and what the run has spent.
#[derive(Debug, Clone)]
pub struct Rail {
    launch: Launch,
    turns: u32,
    /// What the conversation has cost, summed over the turns of every run.
    tokens: Tokens,
    /// The size of the conversation as the provider last counted it: the whole
    /// history goes in every turn, so this is the last input count, not a sum.
    context: u64,
    stop: Option<Stop>,
}

impl Rail {
    /// A rail that has run nothing yet.
    pub fn new(launch: Launch) -> Self {
        Rail {
            launch,
            turns: 0,
            tokens: Tokens::default(),
            context: 0,
            stop: None,
        }
    }

    /// One more turn.
    pub fn turn(&mut self, turn: u32) {
        self.turns = self.turns.max(turn);
    }

    /// What the run that just ended spent, and why it ended.
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

    /// The GUID this launch is recorded under.
    pub fn session(&self) -> String {
        self.launch.session.clone()
    }

    /// The context the conversation has used, as the provider last counted it.
    pub fn context_used(&self) -> u64 {
        self.context
    }

    /// The rail's facts, in the order they matter, each as its name and its
    /// value: the column and the strip are two renderings of this one list.
    pub fn facts(&self) -> Vec<(String, String)> {
        let mut facts = vec![
            ("cache".to_string(), self.cache()),
            (
                "turns".to_string(),
                format!("{}/{}", self.turns, self.launch.max_turns),
            ),
            (
                "tokens".to_string(),
                format!(
                    "{} in · {} out",
                    compact(self.tokens.input),
                    compact(self.tokens.output)
                ),
            ),
            ("context".to_string(), self.meter()),
        ];
        if let Some(stop) = self.stop {
            facts.push(("stop".to_string(), stop.as_str().to_string()));
        }
        facts.push(("session".to_string(), self.launch.session.clone()));
        facts.push(("temp".to_string(), self.launch.temp.clone()));
        facts.push((
            "branch".to_string(),
            self.launch
                .branch
                .clone()
                .unwrap_or_else(|| "—".to_string()),
        ));
        facts.push(("sandbox".to_string(), self.launch.sandbox.clone()));
        facts
    }

    /// The rail as one line, for a terminal too narrow to give it a column:
    /// facts are kept in the order they matter until the line is full, so the
    /// sandbox posture is never the fact that falls off the end.
    pub fn strip(&self, width: usize) -> String {
        let facts = self.facts();
        let pick = |name: &str, short: &str| {
            facts
                .iter()
                .find(|(label, _)| label == name)
                .map(|(_, value)| format!("{short} {value}"))
        };
        let order: Vec<String> = [
            pick("cache", "cache"),
            pick("turns", "turns"),
            pick("context", "ctx"),
            pick("sandbox", "sandbox"),
            pick("stop", "stop"),
            pick("branch", "branch"),
            pick("tokens", "tok"),
        ]
        .into_iter()
        .flatten()
        .collect();

        let mut fitted = String::new();
        for fact in order {
            let candidate = if fitted.is_empty() {
                fact.clone()
            } else {
                format!("{fitted} · {fact}")
            };
            if candidate.chars().count() <= width {
                fitted = candidate;
            }
        }
        fitted.chars().take(width).collect()
    }

    /// The share of input tokens the provider served from its cache.
    fn cache(&self) -> String {
        if self.tokens.input == 0 {
            return "—".to_string();
        }
        let share = self.tokens.cached.saturating_mul(100) / self.tokens.input;
        format!("{share}%")
    }

    /// The context meter: eight cells, filled by how much of the window is gone.
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

/// A count the way a rail says it: exact below a thousand, one decimal above.
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
