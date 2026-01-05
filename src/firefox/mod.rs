//! Firefox backend using the Remote Debugging Protocol (RDP)
//!
//! This module provides Firefox browser automation via the RDP,
//! which uses length-prefixed JSON messages over TCP.

mod actors;
mod client;
mod launcher;
mod session;

pub use launcher::FirefoxLauncher;
pub use session::FirefoxSession;
