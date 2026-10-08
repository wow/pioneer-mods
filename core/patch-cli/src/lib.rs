//! Library half of `patch-cli`: file reading and writing safety shared by its commands, kept in a
//! library so its refusal paths can be tested from `tests/`.

pub mod input;
pub mod output;
