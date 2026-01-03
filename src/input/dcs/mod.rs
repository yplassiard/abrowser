//! Device Control String (DCS) parsing.
//! Adapted from Carbonyl (MIT License).

mod control_flow;
mod parser;
mod resource;
mod status;

pub use parser::*;
