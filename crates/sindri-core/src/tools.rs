use serde_json::{Value, json};

use crate::open;
use crate::policy::Boundary;
use crate::skills::Catalog;

pub const BASH: &str = "bash";

pub const SKILL: &str = "skill";

pub const OPEN: &str = "open";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontEnd {
    Terminal,
    Batch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    name: &'static str,
    description: String,
    parameters: Value,
}

impl Tool {
    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    /// The schema a call's arguments must match, so a wire knows the shape of what it asks for.
    pub fn parameters(&self) -> &Value {
        &self.parameters
    }
}

/// One string argument, named and required: every tool here takes exactly one.
fn argument(name: &str, described: &str) -> Value {
    json!({
        "type": "object",
        "properties": {name: {"type": "string", "description": described}},
        "required": [name],
        "additionalProperties": false
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolSet {
    tools: Vec<Tool>,
}

impl ToolSet {
    pub fn new(boundary: &Boundary, catalog: &Catalog, front_end: FrontEnd) -> ToolSet {
        let mut tools = vec![Tool {
            name: BASH,
            description: boundary.description(),
            parameters: argument("command", "The command line to run with `bash`."),
        }];
        if let Some(description) = catalog.tool_description() {
            tools.push(Tool {
                name: SKILL,
                description,
                parameters: argument("name", "The skill to load, as the description names it."),
            });
        }
        if front_end == FrontEnd::Terminal {
            tools.push(Tool {
                name: OPEN,
                description: open::description(boundary),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "url": {"type": "string", "description": "A web page to open."},
                        "path": {"type": "string", "description": "A file to open."}
                    },
                    "additionalProperties": false
                }),
            });
        }
        ToolSet { tools }
    }

    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|tool| tool.name == name)
    }
}
