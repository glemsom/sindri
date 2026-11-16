//! What a source file is a file of: the module the tree reads, where `foo.rs` and the `tests.rs`
//! beside it are one module.
#![allow(dead_code)]

pub fn of(file: &str) -> &str {
    let file = file.split(':').next().unwrap_or(file);
    file.strip_suffix("/tests.rs")
        .or_else(|| file.strip_suffix(".rs"))
        .unwrap_or(file)
}
