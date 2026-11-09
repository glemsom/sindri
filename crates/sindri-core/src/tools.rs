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
}

impl Tool {
    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }
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
        }];
        if let Some(description) = catalog.tool_description() {
            tools.push(Tool {
                name: SKILL,
                description,
            });
        }
        if front_end == FrontEnd::Terminal {
            tools.push(Tool {
                name: OPEN,
                description: open::description(boundary),
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
