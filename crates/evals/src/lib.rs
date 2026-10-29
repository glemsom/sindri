//! The eval suite's parts, so its own reporting and its checks can be tested
//! without a process: the binary is the table, and this is everything under it.

pub mod case;
pub mod checks;
pub mod guard;
pub mod report;
pub mod run;
pub mod workspace;
