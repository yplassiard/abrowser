//! Chromium browser backend using Chrome DevTools Protocol (CDP).
//!
//! This module provides the Chromium implementation of the browser backend traits.

mod client;
mod launcher;
mod session;
mod cdp_types;

pub use launcher::ChromiumLauncher;
pub use session::ChromiumSession;
pub use client::CdpClient;
