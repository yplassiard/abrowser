//! abrowser - Accessible terminal browser
//!
//! A browser optimized for screen readers and braille displays.
//! Provides semantic access to web content via the accessibility tree.
//!
//! Supports multiple browser backends:
//! - Chromium (via Chrome DevTools Protocol)
//! - Firefox (via Remote Debugging Protocol)

pub mod accessibility;
pub mod ai;
pub mod backend;
pub mod cdp;
pub mod chromium;
pub mod cli;
pub mod firefox;
pub mod input;
pub mod output;
pub mod shell;

mod utils;
