//! abrowser - Accessible terminal browser
//!
//! A Chromium-based browser optimized for screen readers and braille displays.
//! Provides semantic access to web content via the accessibility tree.

pub mod accessibility;
pub mod browser;
pub mod cdp;
pub mod cli;
pub mod input;
pub mod output;
pub mod shell;

mod utils;
