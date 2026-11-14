//! A call is drawn as the thing it names: the arguments a model sent are a spelling of one of a
//! tool's own facts, so a row says the words a reader would type and never the JSON around them.

use serde_json::Value;
use sindri_core::tools::{BASH, OPEN, SKILL};

/// The keys a tool spells its own fact under, the first it holds a string under being the one its
/// call is drawn as: an open call names a path where it can and a url where it must.
fn fact_keys(name: &str) -> &'static [&'static str] {
    match name {
        BASH => &["command"],
        SKILL => &["name"],
        OPEN => &["path", "url"],
        _ => &[],
    }
}

/// The words a call of `name` sent `arguments` is drawn as: a tool's own fact when the arguments
/// spell one, the whole of what was said when they spell no fact this frame knows, and the words a
/// model that sent words instead of arguments wrote.
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

/// Every word a value says — an object's key then the value under it, an array's items, a number's
/// digits and a boolean's own word — for an argument no tool of this product spells: no brace,
/// bracket, quote, comma or colon of the JSON that framed them survives.
fn spelled(value: &Value) -> String {
    let mut words = Vec::new();
    words_into(value, &mut words);
    words.retain(|word| !word.is_empty());
    words.join(" ")
}

/// The words of arguments this frame cannot read as JSON, with the brace, the bracket, the quote,
/// the comma and the colon a model frames them in taken out: a truncated argument is words a reader
/// can read, and never half a picture of the JSON it was.
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
