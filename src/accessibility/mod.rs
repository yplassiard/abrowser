//! Accessibility tree handling.
//!
//! This module receives the Chromium accessibility tree (AXTree)
//! and converts it to linearized, navigable content.

mod node;
mod tree;
mod cursor;

pub use node::*;
pub use tree::*;
pub use cursor::*;
