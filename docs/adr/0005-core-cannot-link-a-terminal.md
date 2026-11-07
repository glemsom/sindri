# Two crates: the core cannot link a terminal

`crates/sindri-core` (lib) holds the run loop, policy, providers, records, skills and config.
`crates/sindri` (bin) holds clap, batch mode, the TUI and the OSC writers. The dependency graph
forbids core from knowing what a terminal is.

"Same core, several front ends" and "no front end owns a decision the core could make" are only
real if they are compile errors when violated. Two crates make them compile errors. Each crate
carries its own `tests/` tree, which §16's audits require to exist.
