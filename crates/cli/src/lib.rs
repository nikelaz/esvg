//! Command line interface for ESVG.
//!
//! The binary is a thin wrapper: all of the behavior lives in these modules so
//! it can be tested directly.

pub mod cli;
pub mod inputs;
pub mod output;
pub mod plugins;
pub mod report;
pub mod run;
