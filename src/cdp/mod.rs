// Chrome DevTools Protocol client for accessibility tree access

mod client;
mod accessibility;

pub use client::CdpClient;
pub use accessibility::{AXNode, AXTree, AccessibilityDomain};
