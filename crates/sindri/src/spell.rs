use serde_json::Value;
use sindri_core::tools::{BASH, OPEN, SKILL};

fn fact_keys(name: &str) -> &'static [&'static str] {
    match name {
        BASH => &["command"],
        SKILL => &["name"],
        OPEN => &["path", "url"],
        _ => &[],
    }
}

pub fn of(name: &str, arguments: &str) -> String {
    let written = arguments.trim();
    let Ok(said) = serde_json::from_str::<Value>(written) else {
        return unframed(written);
    };
    if let Value::String(text) = &said {
        return text.clone();
    }
    fact_keys(name)
        .iter()
        .find_map(|key| said.get(*key).and_then(Value::as_str))
        .map_or_else(|| spelled(&said), String::from)
}

pub fn file(name: &str, arguments: &str) -> Option<String> {
    if name != OPEN {
        return None;
    }
    let said = serde_json::from_str::<Value>(arguments.trim()).ok()?;
    said.get("path").and_then(Value::as_str).map(String::from)
}

fn spelled(value: &Value) -> String {
    let mut words = Vec::new();
    words_into(value, &mut words);
    words.retain(|word| !word.is_empty());
    words.join(" ")
}

fn unframed(text: &str) -> String {
    let words: String = text
        .chars()
        .filter(|glyph| !matches!(glyph, '{' | '}' | '[' | ']' | '"' | ':' | ','))
        .collect();
    words.split_whitespace().collect::<Vec<&str>>().join(" ")
}

fn words_into(value: &Value, words: &mut Vec<String>) {
    match value {
        Value::Null => {}
        Value::Bool(truth) => words.push(truth.to_string()),
        Value::Number(digits) => words.push(digits.to_string()),
        Value::String(text) => words.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| words_into(item, words)),
        Value::Object(entries) => {
            for (key, value) in entries {
                words.push(key.clone());
                words_into(value, words);
            }
        }
    }
}
