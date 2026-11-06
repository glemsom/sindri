//! The tools a run registers, and the one it drops when there is nothing to name:
//! `bash` always, because a shell is the only way to act, and `skill` only when
//! the catalog holds a skill worth naming.
//!
//! Absence is total rather than a refusal. A tool the front end cannot honour is
//! not in the list at all, so the model's schema never mentions it and no turn is
//! spent finding out it is there. That is why the skill tool's description is
//! derived from the catalog rather than the preamble: skills are paid for where
//! they exist, and a run without one pays nothing.

use crate::policy::Boundary;
use crate::skills::Catalog;

/// The name of the one tool every run registers.
pub const BASH: &str = "bash";

/// The name of the tool a run registers only when the catalog offers a skill.
pub const SKILL: &str = "skill";

/// One tool a run registers: the name a call uses, and the words the model is
/// given about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    /// The name a call names.
    name: &'static str,
    /// The description the model's schema carries, derived from the value in
    /// force rather than written by hand.
    description: String,
}

impl Tool {
    /// The name a call names.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The words the model is given about this tool.
    pub fn description(&self) -> &str {
        &self.description
    }
}

/// The tools a front end registers for a run, in the order it registers them.
///
/// There is one constructor and it takes the values the descriptions are derived
/// from, so a tool set cannot be assembled that describes a boundary or a catalog
/// other than the one in force.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolSet {
    /// The tools, `bash` first and the skill tool after it when there is one.
    tools: Vec<Tool>,
}

impl ToolSet {
    /// The tool set a run registers: `bash` described by the boundary in force,
    /// and the skill tool described by the catalog when the catalog has a skill
    /// to name.
    pub fn new(boundary: &Boundary, catalog: &Catalog) -> ToolSet {
        let mut tools = vec![Tool {
            name: BASH,
            description: boundary.description(),
        }];
        if let Some(description) = catalog.tool_description() {
            tools.push(Tool {
                name: SKILL,
                description,
            });
        }
        ToolSet { tools }
    }

    /// The tools in the order a front end registers them.
    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// The tool a name names, or `None` when this run never registered it.
    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|tool| tool.name == name)
    }
}
