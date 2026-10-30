//! One row of the rail: a label in a fixed field and its value beside it.

/// One row of the rail: a label in a fixed field and its value beside it.
/// One row of the rail: a label in a fixed field and its value beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    label: &'static str,
    value: String,
    block: Block,
    fit: Fit,
    keep: bool,
    /// Whether the figure just grew: the frame accents it for the moment it
    /// takes to be seen, so motion reads as motion and not as a second colour
    /// (ADR-0011 §4).
    fresh: bool,
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

    /// Whether the figure has just grown: the rail accents it for the moment
    /// it takes to be seen (ADR-0011 §4).
    pub fn fresh(&self) -> bool {
        self.fresh
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

    pub(super) fn spend(label: &'static str, value: String) -> Fact {
        Fact {
            label,
            value,
            block: Block::Spend,
            fit: Fit::End,
            keep: false,
            fresh: false,
        }
    }

    pub(super) fn identity(label: &'static str, value: String, fit: Fit) -> Fact {
        Fact {
            label,
            value,
            block: Block::Identity,
            fit,
            keep: false,
            fresh: false,
        }
    }

    pub(super) fn posture(value: String) -> Fact {
        Fact {
            label: "sandbox",
            value,
            block: Block::Posture,
            fit: Fit::End,
            keep: false,
            fresh: false,
        }
    }

    pub(super) fn kept(mut fact: Fact) -> Fact {
        fact.keep = true;
        fact
    }

    /// Marks the fact as one that has just moved: the rail's own bookkeeping,
    /// set by the rail and never by the renderer.
    pub(super) fn moved(mut fact: Fact, fresh: bool) -> Fact {
        fact.fresh = fresh;
        fact
    }
}
