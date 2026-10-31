//! A record as `--json` prints it: the files on disk in their own shape, as one
//! object — `meta`, `entries` and, when the run said anything, `status`. Nothing
//! here derives a value the record does not already hold, so `show --json` and
//! `cat transcript.jsonl` cannot drift, and a script that outgrows `jq` can read
//! the directory itself and get the same facts.

use serde_json::{Map, Value};

use super::stored::Stored;

impl Stored {
    /// The whole record on one line, compact like `--format json`: a field the
    /// record lacks stays absent rather than becoming `null`, which would state
    /// a fact the run never had.
    pub fn json(&self) -> String {
        let mut object = Map::new();
        object.insert("meta".to_string(), value(&self.meta));
        object.insert(
            "entries".to_string(),
            Value::Array(self.entries.iter().map(value).collect()),
        );
        // An absent `status.json` and an empty one are the same record: a run
        // that had nothing to say leaves no documents.
        if !self.status.is_empty() {
            object.insert(
                "status".to_string(),
                Value::Array(self.status.iter().map(|line| document(line)).collect()),
            );
        }
        Value::Object(object).to_string()
    }
}

/// One value as JSON. Every value written here is a struct or an object of
/// strings and numbers, which `serde_json` cannot refuse; the fallback exists
/// because the signature has one, and is not a claim about the record.
fn value(serialized: &impl serde::Serialize) -> Value {
    serde_json::to_value(serialized).unwrap_or(Value::Null)
}

/// One `status.json` line as the file has it. Bubblewrap's documents are JSON
/// already; a line that is not keeps its own spelling as a string rather than
/// disappearing from the array, because a document a reader cannot parse is
/// still a document the run left.
fn document(line: &str) -> Value {
    serde_json::from_str(line).unwrap_or_else(|_| Value::String(line.to_string()))
}
