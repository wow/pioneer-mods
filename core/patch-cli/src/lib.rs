//! Library half of `patch-cli`: file-writing safety shared by its commands, kept in a library so
//! its refusal paths can be tested from `tests/`.

pub mod output;
