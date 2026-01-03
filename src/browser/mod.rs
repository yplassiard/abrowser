//! Browser launcher and control.
//!
//! This module launches headless Chrome and communicates via CDP (Chrome DevTools Protocol).
//! Unlike Carbonyl, we focus on the accessibility tree rather than pixel rendering.

mod launcher;

pub use launcher::BrowserLauncher;
