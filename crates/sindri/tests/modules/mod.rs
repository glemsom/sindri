#![allow(dead_code)]

pub fn of(file: &str) -> &str {
    let file = file.split(':').next().unwrap_or(file);
    file.strip_suffix("/tests.rs")
        .or_else(|| file.strip_suffix(".rs"))
        .unwrap_or(file)
}
